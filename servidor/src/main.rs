//! Servidor de amigos do Essencial para Horizon.
//!
//! Guarda as contas (nome + resumo do segredo de cada computador), as
//! amizades e os pedidos num arquivo JSON, e repassa mensagens e convites
//! entre amigos online. Não vê nem repassa o jogo: o mundo em si passa pelo
//! "Jogar com amigos" do próprio Horizon.
//!
//! Uso: `essencial-servidor [--endereco 0.0.0.0] [--porta 25590] [--dados essencial-dados.json]`

#[path = "../../mod/src/protocolo.rs"]
mod protocolo;

mod estado;

/// O cliente de rede do próprio mod, usado nos testes de ponta a ponta.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../mod/src/amigos/rede.rs"]
mod rede;

use estado::{Entrada, Envio, Estado, Salvo};
use protocolo::{DoCliente, DoServidor};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Máximo de conexões abertas ao mesmo tempo.
const CONEXOES_MAX: usize = 2000;
/// Máximo de conexões abertas por endereço IP.
const CONEXOES_POR_IP: usize = 8;
/// Tempo para mandar o `OLA` depois de conectar.
const TEMPO_OLA: Duration = Duration::from_secs(15);
/// Sem nada recebido nesse tempo, a conexão é fechada (o mod manda `PING` a cada 30 s).
const TEMPO_SILENCIO: Duration = Duration::from_secs(90);
/// Mensagens por segundo que uma conexão pode mandar, e a rajada permitida.
const TAXA: f64 = 8.0;
const RAJADA: f64 = 24.0;

struct Servidor {
    estado: Mutex<Estado>,
    /// Para onde mandar as mensagens de cada sessão.
    sessoes: Mutex<HashMap<u64, Sessao>>,
    proxima: AtomicU64,
    abertas: AtomicUsize,
    por_ip: Mutex<HashMap<std::net::IpAddr, usize>>,
    dados: PathBuf,
}

struct Sessao {
    tx: Sender<String>,
    socket: TcpStream,
}

impl Servidor {
    /// Manda os envios e salva se as contas mudaram.
    fn despachar(&self, estado: &mut Estado, envios: Vec<Envio>) {
        {
            let sessoes = self.sessoes.lock().unwrap();
            for envio in envios {
                if let Some(s) = sessoes.get(&envio.sessao) {
                    let _ = s.tx.send(envio.mensagem.codificar());
                }
            }
        }
        if estado.sujo {
            match salvar(&self.dados, &estado.salvo) {
                Ok(()) => estado.sujo = false,
                Err(e) => eprintln!("erro ao salvar {}: {e}", self.dados.display()),
            }
        }
    }

    /// Fecha a sessão `id` (outra entrada da mesma conta a substituiu).
    fn derrubar(&self, id: u64) {
        if let Some(s) = self.sessoes.lock().unwrap().remove(&id) {
            let _ = s.socket.shutdown(Shutdown::Both);
        }
    }
}

fn carregar(caminho: &Path) -> Result<Salvo, String> {
    match std::fs::read(caminho) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Salvo::default()),
        Err(e) => Err(e.to_string()),
    }
}

/// Salva num arquivo temporário e troca, para nunca deixar um arquivo pela metade.
fn salvar(caminho: &Path, salvo: &Salvo) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(salvo).map_err(std::io::Error::other)?;
    let mut tmp = caminho.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, caminho)
}

/// Lê uma linha de no máximo `LINHA_MAX` bytes. `Ok(None)` quando a conexão fechou.
fn ler_linha(leitor: &mut impl BufRead) -> std::io::Result<Option<String>> {
    let mut buf = Vec::new();
    let lidos = leitor
        .take(protocolo::LINHA_MAX as u64 + 1)
        .read_until(b'\n', &mut buf)?;
    if lidos == 0 {
        return Ok(None);
    }
    if buf.last() != Some(&b'\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "linha grande demais",
        ));
    }
    buf.pop();
    String::from_utf8(buf)
        .map(Some)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "texto não é UTF-8"))
}

fn atender(servidor: Arc<Servidor>, socket: TcpStream, endereco: SocketAddr) {
    let id = servidor.proxima.fetch_add(1, Ordering::Relaxed);
    let _ = socket.set_nodelay(true);
    let Ok(escrita) = socket.try_clone() else {
        return;
    };
    let (tx, rx) = channel::<String>();
    servidor.sessoes.lock().unwrap().insert(
        id,
        Sessao {
            tx: tx.clone(),
            socket: socket.try_clone().expect("socket"),
        },
    );
    // Quem escreve: uma thread por conexão, para um amigo lento não travar os outros.
    let escritor = std::thread::spawn(move || {
        let mut escrita = escrita;
        let _ = escrita.set_write_timeout(Some(Duration::from_secs(30)));
        for linha in rx {
            if escrita.write_all(linha.as_bytes()).is_err() {
                break;
            }
        }
        let _ = escrita.shutdown(Shutdown::Both);
    });

    let mut leitor = BufReader::new(socket);
    let mut conta: Option<String> = None;
    let mut fichas = RAJADA;
    let mut ultima = Instant::now();
    let _ = leitor.get_ref().set_read_timeout(Some(TEMPO_OLA));
    while let Ok(Some(linha)) = ler_linha(&mut leitor) {
        // Limite de taxa: fichas voltam com o tempo.
        let agora = Instant::now();
        fichas = (fichas + agora.duration_since(ultima).as_secs_f64() * TAXA).min(RAJADA);
        ultima = agora;
        if fichas < 1.0 {
            let _ = tx.send(
                DoServidor::Erro {
                    texto: "Devagar! Mensagens demais.".into(),
                }
                .codificar(),
            );
            continue;
        }
        fichas -= 1.0;

        let cmd = match DoCliente::decodificar(&linha) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(
                    DoServidor::Erro {
                        texto: format!("Não entendi ({e})."),
                    }
                    .codificar(),
                );
                continue;
            }
        };
        let mut envios = Vec::new();
        let mut estado = servidor.estado.lock().unwrap();
        match (conta.clone(), cmd) {
            (
                None,
                DoCliente::Ola {
                    nome,
                    segredo,
                    versao,
                },
            ) => match estado.entrar(id, &nome, &segredo, versao, &mut envios) {
                Entrada::Aceita { chave, derrubar } => {
                    println!(
                        "{nome} entrou ({endereco}); {} online",
                        estado.quantos_online()
                    );
                    conta = Some(chave);
                    let _ = leitor.get_ref().set_read_timeout(Some(TEMPO_SILENCIO));
                    servidor.despachar(&mut estado, envios);
                    drop(estado);
                    if let Some(antiga) = derrubar {
                        servidor.derrubar(antiga);
                    }
                }
                Entrada::Recusada(motivo) => {
                    let _ = tx.send(DoServidor::Erro { texto: motivo }.codificar());
                    break;
                }
            },
            (None, _) => {
                let _ = tx.send(
                    DoServidor::Erro {
                        texto: "Mande OLA primeiro.".into(),
                    }
                    .codificar(),
                );
                break;
            }
            (Some(chave), cmd) => {
                estado.comando(&chave, id, cmd, &mut envios);
                servidor.despachar(&mut estado, envios);
            }
        }
    }

    if let Some(chave) = &conta {
        let mut envios = Vec::new();
        let mut estado = servidor.estado.lock().unwrap();
        estado.sair(chave, id, &mut envios);
        servidor.despachar(&mut estado, envios);
        println!("{chave} saiu; {} online", estado.quantos_online());
    }
    servidor.sessoes.lock().unwrap().remove(&id);
    drop(tx);
    let _ = escritor.join();
}

fn argumentos() -> Result<(String, u16, PathBuf), String> {
    let mut endereco = "0.0.0.0".to_owned();
    let mut porta = protocolo::PORTA_PADRAO;
    let mut dados = PathBuf::from("essencial-dados.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut valor = || args.next().ok_or(format!("{arg} precisa de um valor"));
        match arg.as_str() {
            "--endereco" => endereco = valor()?,
            "--porta" => porta = valor()?.parse().map_err(|_| "porta inválida".to_owned())?,
            "--dados" => dados = PathBuf::from(valor()?),
            "-h" | "--help" | "--ajuda" => {
                println!(
                    "essencial-servidor [--endereco 0.0.0.0] [--porta {}] [--dados essencial-dados.json]",
                    protocolo::PORTA_PADRAO
                );
                std::process::exit(0);
            }
            outro => return Err(format!("argumento desconhecido: {outro}")),
        }
    }
    Ok((endereco, porta, dados))
}

fn main() {
    let (endereco, porta, dados) = match argumentos() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let salvo = match carregar(&dados) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("não consegui ler {}: {e}", dados.display());
            std::process::exit(1);
        }
    };
    println!(
        "{} contas carregadas de {}",
        salvo.usuarios.len(),
        dados.display()
    );
    let ouvinte = match TcpListener::bind((endereco.as_str(), porta)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("não consegui abrir {endereco}:{porta}: {e}");
            std::process::exit(1);
        }
    };
    println!("Servidor do Essencial ouvindo em {endereco}:{porta}");
    servir(ouvinte, salvo, dados);
}

/// Atende conexões em `ouvinte` para sempre.
fn servir(ouvinte: TcpListener, salvo: Salvo, dados: PathBuf) {
    let servidor = Arc::new(Servidor {
        estado: Mutex::new(Estado::novo(salvo)),
        sessoes: Mutex::new(HashMap::new()),
        proxima: AtomicU64::new(1),
        abertas: AtomicUsize::new(0),
        por_ip: Mutex::new(HashMap::new()),
        dados,
    });
    for conexao in ouvinte.incoming() {
        let Ok(socket) = conexao else { continue };
        let Ok(endereco) = socket.peer_addr() else {
            continue;
        };
        let ip = endereco.ip();
        {
            let mut por_ip = servidor.por_ip.lock().unwrap();
            let n = por_ip.entry(ip).or_default();
            if *n >= CONEXOES_POR_IP || servidor.abertas.load(Ordering::Relaxed) >= CONEXOES_MAX {
                let _ = socket.shutdown(Shutdown::Both);
                continue;
            }
            *n += 1;
        }
        servidor.abertas.fetch_add(1, Ordering::Relaxed);
        let s = Arc::clone(&servidor);
        std::thread::spawn(move || {
            atender(Arc::clone(&s), socket, endereco);
            s.abertas.fetch_sub(1, Ordering::Relaxed);
            let mut por_ip = s.por_ip.lock().unwrap();
            if let Some(n) = por_ip.get_mut(&ip) {
                *n -= 1;
                if *n == 0 {
                    por_ip.remove(&ip);
                }
            }
        });
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_linhas_com_limite() {
        let mut ok = std::io::Cursor::new(b"PING\nPONG\n".to_vec());
        assert_eq!(ler_linha(&mut ok).unwrap().as_deref(), Some("PING"));
        assert_eq!(ler_linha(&mut ok).unwrap().as_deref(), Some("PONG"));
        assert_eq!(ler_linha(&mut ok).unwrap(), None);
        let grande = vec![b'a'; protocolo::LINHA_MAX + 10];
        assert!(ler_linha(&mut std::io::Cursor::new(grande)).is_err());
        assert!(ler_linha(&mut std::io::Cursor::new(b"\xff\n".to_vec())).is_err());
    }

    #[test]
    fn salvar_e_carregar() {
        let dir = std::env::temp_dir().join(format!("essencial-teste-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let caminho = dir.join("dados.json");
        assert_eq!(carregar(&caminho).unwrap(), Salvo::default());
        let mut salvo = Salvo::default();
        salvo.usuarios.insert(
            "ana".into(),
            estado::Usuario {
                nome: "Ana".into(),
                ..Default::default()
            },
        );
        salvar(&caminho, &salvo).unwrap();
        assert_eq!(carregar(&caminho).unwrap(), salvo);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// O cliente de rede do mod contra o servidor de verdade, numa porta local.
#[cfg(test)]
mod ponta_a_ponta {
    use super::*;
    use crate::rede::{Evento, Rede};

    /// Espera até 10 s por um evento que satisfaça `ok`.
    fn esperar(rede: &Rede, quem: &str, ok: impl Fn(&Evento) -> bool) {
        let fim = Instant::now() + Duration::from_secs(10);
        let mut vistos = Vec::new();
        while Instant::now() < fim {
            for e in rede.eventos() {
                if ok(&e) {
                    return;
                }
                vistos.push(e);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("{quem} não recebeu o esperado; recebeu {vistos:?}");
    }

    fn recebeu(e: &Evento, m: &DoServidor) -> bool {
        matches!(e, Evento::Recebido(x) if x == m)
    }

    #[test]
    fn dois_amigos_conversam_e_convidam() {
        let dir = std::env::temp_dir().join(format!("essencial-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dados = dir.join("dados.json");
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        let endereco = ouvinte.local_addr().unwrap().to_string();
        let d = dados.clone();
        std::thread::spawn(move || servir(ouvinte, Salvo::default(), d));

        let ana = Rede::iniciar(endereco.clone(), "Ana".into(), "segredo-da-ana-0001".into());
        let bia = Rede::iniciar(endereco.clone(), "Bia".into(), "segredo-da-bia-0002".into());
        esperar(&ana, "Ana", |e| matches!(e, Evento::Conectado));
        esperar(&bia, "Bia", |e| matches!(e, Evento::Conectado));

        ana.mandar(DoCliente::Adicionar { nome: "bia".into() });
        esperar(&bia, "Bia", |e| {
            recebeu(e, &DoServidor::PedidoRecebido { nome: "Ana".into() })
        });
        bia.mandar(DoCliente::Aceitar { nome: "Ana".into() });
        esperar(&ana, "Ana", |e| {
            recebeu(
                e,
                &DoServidor::Amigo {
                    nome: "Bia".into(),
                    online: true,
                    status: String::new(),
                },
            )
        });

        bia.mandar(DoCliente::Status {
            texto: "Jogando no próprio mundo".into(),
        });
        esperar(&ana, "Ana", |e| {
            recebeu(
                e,
                &DoServidor::Amigo {
                    nome: "Bia".into(),
                    online: true,
                    status: "Jogando no próprio mundo".into(),
                },
            )
        });

        ana.mandar(DoCliente::Mensagem {
            para: "Bia".into(),
            texto: "bora jogar?".into(),
        });
        esperar(&bia, "Bia", |e| {
            recebeu(
                e,
                &DoServidor::Mensagem {
                    de: "Ana".into(),
                    texto: "bora jogar?".into(),
                },
            )
        });
        bia.mandar(DoCliente::Convite {
            para: "Ana".into(),
            codigo: "K7QM-3XPD".into(),
        });
        esperar(&ana, "Ana", |e| {
            recebeu(
                e,
                &DoServidor::Convite {
                    de: "Bia".into(),
                    codigo: "K7QM-3XPD".into(),
                },
            )
        });

        // Bia fecha o jogo: Ana a vê offline.
        drop(bia);
        esperar(&ana, "Ana", |e| {
            recebeu(
                e,
                &DoServidor::Amigo {
                    nome: "Bia".into(),
                    online: false,
                    status: String::new(),
                },
            )
        });

        // Outro computador tentando usar o nome da Ana é recusado e não insiste.
        let falsa = Rede::iniciar(endereco.clone(), "ana".into(), "segredo-de-outro-pc".into());
        esperar(&falsa, "impostor", |e| matches!(e, Evento::Recusado(_)));

        // Bia volta e recebe a lista de amigos com a Ana online.
        let bia = Rede::iniciar(endereco, "Bia".into(), "segredo-da-bia-0002".into());
        esperar(&bia, "Bia", |e| {
            recebeu(
                e,
                &DoServidor::Amigo {
                    nome: "Ana".into(),
                    online: true,
                    status: String::new(),
                },
            )
        });

        // A amizade foi salva em disco.
        let salvo = carregar(&dados).unwrap();
        assert!(salvo.usuarios["ana"].amigos.contains("bia"));
        assert!(salvo.usuarios["bia"].amigos.contains("ana"));
        drop((ana, bia, falsa));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
