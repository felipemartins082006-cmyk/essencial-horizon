//! A conexão com o servidor de amigos, numa thread própria: o jogo nunca
//! espera a rede. Reconecta sozinha, com espera crescente, se cair.

use crate::protocolo::{self, DoCliente, DoServidor};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};

/// A cada quanto mandar `PING`.
const PING: Duration = Duration::from_secs(30);
/// Sem nada do servidor nesse tempo, a conexão é dada como caída.
const SILENCIO: Duration = Duration::from_secs(75);
/// Quanto a leitura espera antes de olhar se há algo para mandar.
const ESPERA_LEITURA: Duration = Duration::from_millis(100);

/// O que a thread conta ao jogo.
#[derive(Debug)]
pub enum Evento {
    /// Entrou no servidor (já recebeu `BEMVINDO`, que vem em `Recebido`).
    Conectado,
    /// A conexão caiu ou não abriu; tenta de novo sozinha.
    Caiu(String),
    /// O servidor recusou a entrada (nome de outro computador, versão
    /// diferente): não tenta de novo até o jogo reiniciar ou a config mudar.
    Recusado(String),
    /// Uma mensagem do servidor.
    Recebido(DoServidor),
}

/// A ponta do jogo.
pub struct Rede {
    tx: Sender<DoCliente>,
    rx: Receiver<Evento>,
    parar: Arc<AtomicBool>,
}

impl Rede {
    /// Começa a conectar em `endereco` (`host:porta`) como `nome`.
    pub fn iniciar(endereco: String, nome: String, segredo: String) -> Self {
        let (tx, rx_cmd) = channel();
        let (tx_ev, rx) = channel();
        let parar = Arc::new(AtomicBool::new(false));
        let p = Arc::clone(&parar);
        let resultado = std::thread::Builder::new()
            .name("essencial-amigos".into())
            .spawn(move || laco(&endereco, &nome, &segredo, &rx_cmd, &tx_ev, &p));
        if let Err(e) = resultado {
            eprintln!("essencial: não consegui criar a thread de rede: {e}");
        }
        Self { tx, rx, parar }
    }

    /// Manda um comando (descartado se a conexão não estiver aberta).
    pub fn mandar(&self, cmd: DoCliente) {
        let _ = self.tx.send(cmd);
    }

    /// Os eventos que chegaram desde a última vez.
    pub fn eventos(&self) -> Vec<Evento> {
        self.rx.try_iter().collect()
    }
}

impl Drop for Rede {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::Relaxed);
    }
}

/// Espera `tempo`, acordando para ver se é para parar. `false` se for.
fn dormir(tempo: Duration, parar: &AtomicBool) -> bool {
    let fim = Instant::now() + tempo;
    while Instant::now() < fim {
        if parar.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    !parar.load(Ordering::Relaxed)
}

fn conectar(endereco: &str) -> Result<TcpStream, String> {
    let enderecos = endereco
        .to_socket_addrs()
        .map_err(|e| format!("endereço {endereco}: {e}"))?;
    let mut ultimo = format!("{endereco} não tem endereço");
    for a in enderecos {
        match TcpStream::connect_timeout(&a, Duration::from_secs(5)) {
            Ok(s) => return Ok(s),
            Err(e) => ultimo = format!("{endereco}: {e}"),
        }
    }
    Err(ultimo)
}

/// Como uma sessão terminou.
enum Fim {
    Parar,
    Caiu(String),
    Recusado(String),
}

fn laco(
    endereco: &str,
    nome: &str,
    segredo: &str,
    rx: &Receiver<DoCliente>,
    tx: &Sender<Evento>,
    parar: &AtomicBool,
) {
    let mut espera = Duration::from_secs(2);
    while !parar.load(Ordering::Relaxed) {
        let fim = match conectar(endereco) {
            Ok(socket) => sessao(socket, nome, segredo, rx, tx, parar, &mut espera),
            Err(e) => Fim::Caiu(e),
        };
        match fim {
            Fim::Parar => return,
            Fim::Recusado(motivo) => {
                let _ = tx.send(Evento::Recusado(motivo));
                return;
            }
            Fim::Caiu(motivo) => {
                if tx.send(Evento::Caiu(motivo)).is_err() {
                    return;
                }
            }
        }
        // Comandos feitos enquanto estava fora não valem mais.
        while rx.try_recv().is_ok() {}
        if !dormir(espera, parar) {
            return;
        }
        espera = (espera * 2).min(Duration::from_secs(60));
    }
}

fn sessao(
    socket: TcpStream,
    nome: &str,
    segredo: &str,
    rx: &Receiver<DoCliente>,
    tx: &Sender<Evento>,
    parar: &AtomicBool,
    espera: &mut Duration,
) -> Fim {
    let _ = socket.set_nodelay(true);
    if socket.set_read_timeout(Some(ESPERA_LEITURA)).is_err() {
        return Fim::Caiu("não consegui configurar a conexão".into());
    }
    let mut escrita = match socket.try_clone() {
        Ok(s) => s,
        Err(e) => return Fim::Caiu(e.to_string()),
    };
    let _ = escrita.set_write_timeout(Some(Duration::from_secs(10)));
    let ola = DoCliente::Ola {
        nome: nome.to_owned(),
        segredo: segredo.to_owned(),
        versao: protocolo::VERSAO,
    };
    if let Err(e) = escrita.write_all(ola.codificar().as_bytes()) {
        return Fim::Caiu(e.to_string());
    }
    let mut leitor = BufReader::new(socket);
    let mut buf = Vec::new();
    let mut entrou = false;
    // O último `ERRO` e quando chegou: um erro logo antes de o servidor
    // fechar a conexão é o motivo (nome recusado, entrou em outro lugar).
    let mut ultimo_erro: Option<(String, Instant)> = None;
    let mut ultimo_ping = Instant::now();
    let mut ultima_vez = Instant::now();
    loop {
        if parar.load(Ordering::Relaxed) {
            return Fim::Parar;
        }
        // Mandar o que o jogo pediu.
        loop {
            match rx.try_recv() {
                Ok(cmd) => {
                    if let Err(e) = escrita.write_all(cmd.codificar().as_bytes()) {
                        return Fim::Caiu(e.to_string());
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Fim::Parar,
            }
        }
        if entrou && ultimo_ping.elapsed() >= PING {
            ultimo_ping = Instant::now();
            if let Err(e) = escrita.write_all(DoCliente::Ping.codificar().as_bytes()) {
                return Fim::Caiu(e.to_string());
            }
        }
        if ultima_vez.elapsed() >= SILENCIO {
            return Fim::Caiu("o servidor parou de responder".into());
        }
        // Ler. O que já chegou de uma linha fica em `buf` entre as tentativas.
        let limite = (protocolo::LINHA_MAX + 1 - buf.len().min(protocolo::LINHA_MAX)) as u64;
        match (&mut leitor).take(limite).read_until(b'\n', &mut buf) {
            Ok(0) => {
                // Conexão fechada pelo servidor.
                return match ultimo_erro {
                    Some((motivo, quando))
                        if !entrou || quando.elapsed() < Duration::from_secs(3) =>
                    {
                        Fim::Recusado(motivo)
                    }
                    _ => Fim::Caiu("o servidor fechou a conexão".into()),
                };
            }
            Ok(_) => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(e) => return Fim::Caiu(e.to_string()),
        }
        if !buf.ends_with(b"\n") {
            if buf.len() > protocolo::LINHA_MAX {
                return Fim::Caiu("linha grande demais do servidor".into());
            }
            continue;
        }
        buf.pop();
        let linha = String::from_utf8_lossy(&buf).into_owned();
        buf.clear();
        ultima_vez = Instant::now();
        let msg = match DoServidor::decodificar(&linha) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("essencial: mensagem do servidor não entendida ({e}): {linha:?}");
                continue;
            }
        };
        match &msg {
            DoServidor::BemVindo { .. } => {
                entrou = true;
                *espera = Duration::from_secs(2);
                if tx.send(Evento::Conectado).is_err() {
                    return Fim::Parar;
                }
            }
            DoServidor::Erro { texto } => ultimo_erro = Some((texto.clone(), Instant::now())),
            _ => {}
        }
        if tx.send(Evento::Recebido(msg)).is_err() {
            return Fim::Parar;
        }
    }
}
