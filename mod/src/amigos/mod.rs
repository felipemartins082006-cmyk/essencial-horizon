//! Amigos: a lista, os pedidos, as mensagens e os convites, ligados ao
//! servidor de amigos pela [`rede`]. O mundo em si é compartilhado pelo
//! "Jogar com amigos" do Horizon; aqui só passa o código do convite.

mod rede;
mod tela;

use crate::ID;
use crate::protocolo::{self, DoCliente, DoServidor};
use crate::texto;
use mod_api::text::TextClick;
use mod_api::{ClientHost, ConfigPath, Flow, ScreenRenderPostEvent, Text};
use rede::{Evento, Rede};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub use tela::Tela;

/// O `config/essencial.toml` que o jogo cria na primeira vez.
pub const CONFIG_PADRAO: &str = "\
# Essencial: servidor de amigos.
# Coloque aqui o endereço de quem roda o essencial-servidor, por exemplo
# servidor = \"192.168.0.10:25590\" ou servidor = \"amigos.meusite.com:25590\".
# Vazio: a lista de amigos fica desligada (zoom e HUD continuam funcionando).
servidor = \"\"
";

/// Quanto tempo o aviso de mensagem/convite fica no canto da tela.
const TEMPO_AVISO: Duration = Duration::from_secs(6);
/// Linhas guardadas para a tela de amigos.
const HISTORICO_MAX: usize = 50;
/// Linhas esperando para ir ao chat (quando ainda não há mundo aberto).
const PENDENTES_MAX: usize = 30;

/// Lê `servidor = "..."` do arquivo de configuração. `None` se vazio ou ausente.
pub fn ler_config(texto: &str) -> Option<String> {
    for linha in texto.lines() {
        let linha = linha.trim();
        let Some(resto) = linha.strip_prefix("servidor") else {
            continue;
        };
        let Some(valor) = resto.trim_start().strip_prefix('=') else {
            continue;
        };
        let valor = valor.trim();
        let valor = valor
            .strip_prefix('"')
            .and_then(|v| v.split_once('"').map(|(v, _)| v))
            .unwrap_or_else(|| valor.split('#').next().unwrap_or("").trim());
        let valor = valor.trim();
        if valor.is_empty() {
            return None;
        }
        // Sem porta, usa a padrão.
        let tem_porta = valor
            .rsplit_once(':')
            .is_some_and(|(_, p)| p.parse::<u16>().is_ok() && !valor.ends_with(']'));
        return Some(if tem_porta {
            valor.to_owned()
        } else {
            format!("{valor}:{}", protocolo::PORTA_PADRAO)
        });
    }
    None
}

/// Um segredo novo para este computador: 32 dígitos hexadecimais. Vem das
/// chaves aleatórias que a biblioteca padrão pede ao sistema para cada `RandomState`.
fn novo_segredo() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut s = String::new();
    for i in 0..4u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        s.push_str(&format!("{:08x}", h.finish() as u32));
    }
    s
}

/// Um amigo como a lista o mostra.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Amigo {
    pub nome: String,
    pub online: bool,
    pub status: String,
}

/// Uma linha do histórico da tela de amigos.
#[derive(Clone, Debug)]
pub struct LinhaHistorico {
    pub texto: String,
    pub cor: u32,
}

/// O estado da conexão, para mostrar ao jogador.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Conexao {
    /// Sem `servidor` na configuração, ou desligado nas opções.
    #[default]
    Desligado,
    /// Esperando entrar num mundo para saber o nome do jogador.
    EsperandoNome,
    Conectando,
    Conectado,
    /// Caiu; tentando de novo.
    Caiu(String),
    /// Recusado pelo servidor; não tenta mais.
    Recusado(String),
}

#[derive(Default)]
struct Interno {
    endereco: Option<String>,
    segredo: Option<String>,
    rede: Option<Rede>,
    conexao: Conexao,
    /// O nome com que entrei.
    nome: Option<String>,
    amigos: BTreeMap<String, Amigo>,
    recebidos: BTreeMap<String, String>,
    enviados: BTreeMap<String, String>,
    historico: VecDeque<LinhaHistorico>,
    pendentes: VecDeque<Text>,
    /// O último amigo que mandou mensagem (para `.r`).
    ultimo_de: Option<String>,
    /// O último convite recebido: de quem e o código.
    ultimo_convite: Option<(String, String)>,
    aviso: Option<(String, Instant)>,
    /// Muda sempre que a lista muda, para a tela se refazer.
    versao: u64,
    status_mandado: Option<String>,
    abrir_tela: bool,
}

fn chave(nome: &str) -> String {
    nome.to_ascii_lowercase()
}

const VERDE: u32 = 0xFF55_FF55;
const CINZA: u32 = 0xFFAA_AAAA;
const AZUL: u32 = 0xFF55_FFFF;
const VERMELHO: u32 = 0xFFFF_5555;
const DOURADO: u32 = 0xFFFF_AA00;

impl Interno {
    fn conectado(&self) -> bool {
        self.conexao == Conexao::Conectado
    }

    fn chat(&mut self, linha: Text) {
        if self.pendentes.len() >= PENDENTES_MAX {
            self.pendentes.pop_front();
        }
        self.pendentes.push_back(linha);
    }

    fn historico(&mut self, texto: impl Into<String>, cor: u32) {
        if self.historico.len() >= HISTORICO_MAX {
            self.historico.pop_front();
        }
        self.historico.push_back(LinhaHistorico {
            texto: texto.into(),
            cor,
        });
        self.versao += 1;
    }

    fn avisar(&mut self, texto: String) {
        self.aviso = Some((texto, Instant::now()));
    }

    /// Manda um comando, ou explica que não há conexão. `true` se foi.
    fn mandar(&mut self, cmd: DoCliente) -> bool {
        match (&self.rede, self.conectado()) {
            (Some(rede), true) => {
                rede.mandar(cmd);
                true
            }
            _ => {
                let motivo = match &self.conexao {
                    Conexao::Desligado => {
                        "A lista de amigos está desligada. Coloque o endereço do servidor em config/essencial.toml.".to_owned()
                    }
                    Conexao::Recusado(m) => format!("O servidor de amigos recusou a entrada: {m}"),
                    _ => "Sem conexão com o servidor de amigos agora. Tente daqui a pouco.".to_owned(),
                };
                self.chat(texto::erro(motivo.clone()));
                self.historico(motivo, VERMELHO);
                false
            }
        }
    }

    /// Trata uma mensagem do servidor.
    fn receber(&mut self, msg: DoServidor, avisos: bool) {
        match msg {
            DoServidor::BemVindo { nome } => {
                self.amigos.clear();
                self.recebidos.clear();
                self.enviados.clear();
                self.status_mandado = None;
                self.historico(format!("Conectado como {nome}."), VERDE);
                self.nome = Some(nome);
            }
            DoServidor::Erro { texto } => {
                self.chat(texto::erro(texto.clone()));
                self.historico(texto, VERMELHO);
            }
            DoServidor::Amigo {
                nome,
                online,
                status,
            } => {
                let k = chave(&nome);
                let antes = self.amigos.get(&k).map(|a| a.online);
                if antes == Some(false) && online {
                    self.historico(format!("{nome} entrou."), CINZA);
                } else if antes == Some(true) && !online {
                    self.historico(format!("{nome} saiu."), CINZA);
                } else if antes.is_none() && self.conectado() {
                    self.chat(texto::linha([
                        texto::cor(nome.clone(), "aqua"),
                        texto::cor(" agora é seu amigo!", "green"),
                    ]));
                    self.historico(format!("{nome} agora é seu amigo."), VERDE);
                }
                self.amigos.insert(
                    k,
                    Amigo {
                        nome,
                        online,
                        status,
                    },
                );
                self.versao += 1;
            }
            DoServidor::AmigoRemovido { nome } => {
                self.amigos.remove(&chave(&nome));
                self.historico(format!("{nome} não é mais seu amigo."), CINZA);
            }
            DoServidor::PedidoRecebido { nome } => {
                let novo = self.recebidos.insert(chave(&nome), nome.clone()).is_none();
                if novo {
                    self.chat(texto::linha([
                        texto::cor(nome.clone(), "aqua"),
                        texto::cor(" quer ser seu amigo. ", "gray"),
                        texto::botao(
                            "[Aceitar]",
                            "green",
                            "Aceitar o pedido",
                            TextClick::SuggestCommand(format!(".aceitar {nome}")),
                        ),
                        Text::literal(" "),
                        texto::botao(
                            "[Recusar]",
                            "red",
                            "Recusar o pedido",
                            TextClick::SuggestCommand(format!(".recusar {nome}")),
                        ),
                    ]));
                    self.historico(format!("Pedido de amizade de {nome}."), DOURADO);
                    if avisos {
                        self.avisar(format!("Pedido de amizade: {nome}"));
                    }
                }
                self.versao += 1;
            }
            DoServidor::PedidoEnviado { nome } => {
                self.enviados.insert(chave(&nome), nome);
                self.versao += 1;
            }
            DoServidor::PedidoRemovido { nome } => {
                let k = chave(&nome);
                self.recebidos.remove(&k);
                self.enviados.remove(&k);
                self.versao += 1;
            }
            DoServidor::Mensagem { de, texto: msg } => {
                self.chat(texto::linha([
                    texto::botao(
                        de.clone(),
                        "aqua",
                        "Clique para responder",
                        TextClick::SuggestCommand(format!(".msg {de} ")),
                    ),
                    texto::cor(" → você: ", "gray"),
                    Text::literal(msg.clone()),
                ]));
                self.historico(format!("{de}: {msg}"), AZUL);
                if avisos {
                    self.avisar(format!("{de}: {}", protocolo::limpar(&msg, 40)));
                }
                self.ultimo_de = Some(de);
            }
            DoServidor::Convite { de, codigo } => {
                self.chat(texto::linha([
                    texto::cor(de.clone(), "aqua"),
                    texto::cor(" convidou você para jogar no mundo. ", "gray"),
                    texto::botao(
                        "[Copiar código]",
                        "green",
                        "Copia o código do convite",
                        TextClick::CopyToClipboard(codigo.clone()),
                    ),
                ]));
                self.chat(texto::aviso(
                    "Depois cole o código em Multijogador > Conexão direta, ou no botão Amigos da tela de título (Junte-se a um amigo). Ou digite .entrar para copiar.",
                ));
                self.historico(format!("Convite de {de}: {codigo}"), DOURADO);
                if avisos {
                    self.avisar(format!("Convite de {de}! Veja o chat."));
                }
                self.ultimo_convite = Some((de, codigo));
            }
            DoServidor::Pong => {}
        }
    }
}

#[derive(Default)]
pub struct Amigos {
    interno: Mutex<Interno>,
    tela: Tela,
}

/// O nome do jogador local, pela lista de jogadores (Tab).
fn nome_local(host: &dyn ClientHost) -> Option<String> {
    let uuid = host.local_player()?.uuid();
    let lista = host.client_session().client_tab_list()?;
    lista
        .entries
        .into_iter()
        .find(|e| e.uuid == uuid)
        .map(|e| e.name)
        .filter(|n| protocolo::nome_valido(n))
}

/// O segredo deste computador, criado na primeira vez.
fn segredo(host: &dyn ClientHost) -> Option<String> {
    let arquivo = ConfigPath::new("segredo.txt").ok()?;
    let arquivos = host.data_files();
    if let Ok(bytes) = arquivos.read(&arquivo) {
        let s = String::from_utf8_lossy(&bytes).trim().to_owned();
        if s.len() >= 16 {
            return Some(s);
        }
    }
    let novo = novo_segredo();
    match arquivos.write(&arquivo, novo.as_bytes()) {
        Ok(()) => Some(novo),
        Err(e) => {
            eprintln!("essencial: não consegui salvar o segredo: {e:?}");
            None
        }
    }
}

/// O que mostrar aos amigos sobre onde estou.
fn status_atual(host: &dyn ClientHost) -> String {
    let Some(servidor) = host.client_session().client_server() else {
        return "No menu".into();
    };
    if servidor.integrated_local {
        return "Jogando no próprio mundo".into();
    }
    if host.setting_bool("amigos_mostrar_servidor").unwrap_or(true)
        && let Some(endereco) = servidor.address
        && !endereco.starts_with("friend:")
    {
        return protocolo::limpar(&format!("Jogando em {endereco}"), protocolo::STATUS_MAX);
    }
    "Jogando online".into()
}

const AJUDA: &[(&str, &str)] = &[
    (".amigos", "abre a tela de amigos (ou a tecla O)"),
    (".add <nome>", "pede amizade"),
    (".aceitar <nome> / .recusar <nome>", "responde a um pedido"),
    (".remover <nome>", "desfaz a amizade ou cancela o pedido"),
    (".msg <nome> <texto>", "mensagem privada"),
    (".r <texto>", "responde a última mensagem"),
    (
        ".convidar <nome>",
        "manda o código do seu mundo (copie antes em Esc > Amigos > Copiar código)",
    ),
    (".entrar", "copia o código do último convite"),
    (".lista", "mostra quem está online"),
];

impl Amigos {
    /// Lê a configuração (ao abrir o jogo e quando o arquivo muda).
    pub fn configurar(&self, texto_config: &str) {
        let endereco = ler_config(texto_config);
        let mut i = self.interno.lock().unwrap();
        if i.endereco != endereco {
            i.endereco = endereco;
            i.rede = None;
            i.conexao = Conexao::Desligado;
            i.versao += 1;
        }
    }

    pub fn parar(&self) {
        self.interno.lock().unwrap().rede = None;
    }

    /// O aviso do canto da tela, enquanto dura.
    pub fn aviso(&self) -> Option<String> {
        let mut i = self.interno.lock().unwrap();
        match &i.aviso {
            Some((t, quando)) if quando.elapsed() < TEMPO_AVISO => Some(t.clone()),
            Some(_) => {
                i.aviso = None;
                None
            }
            None => None,
        }
    }

    /// Uma vez por tick.
    pub fn tick(&self, host: &mut dyn ClientHost) {
        let ligado = host.setting_bool("amigos").unwrap_or(true);
        let avisos = host.setting_bool("amigos_avisos").unwrap_or(true);
        let abrir = {
            let mut i = self.interno.lock().unwrap();
            if !ligado {
                if i.rede.is_some() {
                    i.rede = None;
                    i.conexao = Conexao::Desligado;
                    i.versao += 1;
                }
            } else if let Some(endereco) = i.endereco.clone()
                && i.rede.is_none()
                && !matches!(i.conexao, Conexao::Recusado(_))
            {
                if i.segredo.is_none() {
                    i.segredo = segredo(host);
                }
                match (nome_local(host), i.segredo.clone()) {
                    (Some(nome), Some(seg)) => {
                        i.rede = Some(Rede::iniciar(endereco, nome, seg));
                        i.conexao = Conexao::Conectando;
                    }
                    _ => i.conexao = Conexao::EsperandoNome,
                }
                i.versao += 1;
            }
            let eventos = i.rede.as_ref().map(Rede::eventos).unwrap_or_default();
            for evento in eventos {
                match evento {
                    Evento::Conectado => {
                        i.conexao = Conexao::Conectado;
                        i.versao += 1;
                    }
                    Evento::Caiu(motivo) => {
                        if i.conexao == Conexao::Conectado {
                            i.historico(
                                "Conexão com o servidor de amigos caiu; tentando de novo.",
                                CINZA,
                            );
                            i.amigos.values_mut().for_each(|a| a.online = false);
                        }
                        i.conexao = Conexao::Caiu(motivo);
                        i.versao += 1;
                    }
                    Evento::Recusado(motivo) => {
                        i.chat(texto::erro(format!(
                            "O servidor de amigos recusou a entrada: {motivo}"
                        )));
                        i.conexao = Conexao::Recusado(motivo);
                        i.rede = None;
                        i.versao += 1;
                    }
                    Evento::Recebido(msg) => i.receber(msg, avisos),
                }
            }
            if i.conectado() {
                let status = status_atual(host);
                if i.status_mandado.as_ref() != Some(&status) {
                    i.status_mandado = Some(status.clone());
                    if let Some(rede) = &i.rede {
                        rede.mandar(DoCliente::Status { texto: status });
                    }
                }
            }
            if host.local_player().is_some() {
                while let Some(linha) = i.pendentes.pop_front() {
                    if host.chat_line(linha).is_err() {
                        break;
                    }
                }
            }
            std::mem::take(&mut i.abrir_tela)
        };
        if abrir || host.consume_click(ID, "amigos") {
            self.tela.abrir(host);
        }
        self.tela.tick(host);
    }

    /// Uma mensagem digitada no chat: as que começam com `.` e são nossas
    /// não vão ao servidor do jogo.
    pub fn chat(&self, host: &mut dyn ClientHost, mensagem: &str) -> Flow {
        let Some(resto) = mensagem.strip_prefix('.') else {
            return Flow::Continue;
        };
        let (cmd, args) = resto.split_once(' ').unwrap_or((resto, ""));
        let args = args.trim();
        let (alvo, texto_msg) = args.split_once(' ').unwrap_or((args, ""));
        let mut i = self.interno.lock().unwrap();
        let precisa_nome = |i: &mut Interno| {
            if alvo.is_empty() {
                i.chat(texto::erro(format!("Uso: .{cmd} <nome>")));
                false
            } else {
                true
            }
        };
        match cmd.to_ascii_lowercase().as_str() {
            "amigos" | "e" => i.abrir_tela = true,
            "ajuda" | "essencial" => {
                i.chat(texto::linha([texto::cor("Comandos:", "yellow")]));
                for (c, d) in AJUDA {
                    i.chat(
                        Text::literal("  ")
                            .append(texto::cor(*c, "aqua"))
                            .append(texto::cor(format!(" — {d}"), "gray")),
                    );
                }
            }
            "add" | "adicionar" => {
                if precisa_nome(&mut i) {
                    i.mandar(DoCliente::Adicionar { nome: alvo.into() });
                }
            }
            "aceitar" => {
                if precisa_nome(&mut i) {
                    i.mandar(DoCliente::Aceitar { nome: alvo.into() });
                }
            }
            "recusar" => {
                if precisa_nome(&mut i) {
                    i.mandar(DoCliente::Recusar { nome: alvo.into() });
                }
            }
            "remover" => {
                if precisa_nome(&mut i) {
                    i.mandar(DoCliente::Remover { nome: alvo.into() });
                }
            }
            "msg" | "m" => {
                if alvo.is_empty() || texto_msg.trim().is_empty() {
                    i.chat(texto::erro("Uso: .msg <nome> <texto>"));
                } else {
                    let t = protocolo::limpar(texto_msg, protocolo::TEXTO_MAX);
                    if i.mandar(DoCliente::Mensagem {
                        para: alvo.into(),
                        texto: t.clone(),
                    }) {
                        i.chat(texto::linha([
                            texto::cor("você → ", "gray"),
                            texto::cor(alvo, "aqua"),
                            texto::cor(": ", "gray"),
                            Text::literal(t.clone()),
                        ]));
                        i.historico(format!("você → {alvo}: {t}"), CINZA);
                    }
                }
            }
            "r" => match i.ultimo_de.clone() {
                None => i.chat(texto::erro("Ninguém mandou mensagem ainda.")),
                Some(_) if args.is_empty() => i.chat(texto::erro("Uso: .r <texto>")),
                Some(de) => {
                    let t = protocolo::limpar(args, protocolo::TEXTO_MAX);
                    if i.mandar(DoCliente::Mensagem {
                        para: de.clone(),
                        texto: t.clone(),
                    }) {
                        i.chat(texto::linha([
                            texto::cor("você → ", "gray"),
                            texto::cor(de.clone(), "aqua"),
                            texto::cor(": ", "gray"),
                            Text::literal(t.clone()),
                        ]));
                        i.historico(format!("você → {de}: {t}"), CINZA);
                    }
                }
            },
            "convidar" => {
                if precisa_nome(&mut i) {
                    let area = host.clipboard_text().unwrap_or_default();
                    convidar(&mut i, alvo, &area);
                }
            }
            "entrar" => match i.ultimo_convite.clone() {
                None => i.chat(texto::erro("Nenhum convite recebido ainda.")),
                Some((de, codigo)) => {
                    if host.set_clipboard_text(&codigo) {
                        i.chat(texto::aviso(format!(
                            "Código do convite de {de} copiado. Cole em Multijogador > Conexão direta."
                        )));
                    } else {
                        i.chat(texto::erro(format!(
                            "Não consegui copiar. O código é {codigo}"
                        )));
                    }
                }
            },
            "lista" => {
                let online: Vec<String> = i
                    .amigos
                    .values()
                    .filter(|a| a.online)
                    .map(|a| a.nome.clone())
                    .collect();
                let linha = if online.is_empty() {
                    format!("Nenhum amigo online ({} no total).", i.amigos.len())
                } else {
                    format!(
                        "Online ({}/{}): {}",
                        online.len(),
                        i.amigos.len(),
                        online.join(", ")
                    )
                };
                i.chat(texto::aviso(linha));
            }
            // Não é nosso: deixa a mensagem ir normalmente.
            _ => return Flow::Continue,
        }
        Flow::Cancel
    }

    pub fn tela_desenhar(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        self.tela.desenhar(self, host, event);
    }

    pub fn tela_entrada(
        &self,
        host: &dyn ClientHost,
        entrada: impl FnOnce(
            &mut mod_api::client::widgets::Widgets,
        ) -> mod_api::client::widgets::Outcome,
    ) -> Flow {
        self.tela.entrada(self, host, entrada)
    }
}

/// Manda o código do meu mundo (lido da área de transferência) para `alvo`.
fn convidar(i: &mut Interno, alvo: &str, area_de_transferencia: &str) {
    match protocolo::codigo_convite(area_de_transferencia) {
        Some(codigo) => {
            if i.mandar(DoCliente::Convite {
                para: alvo.into(),
                codigo,
            }) {
                i.chat(texto::aviso(format!("Convite mandado para {alvo}.")));
                i.historico(format!("Convite mandado para {alvo}."), VERDE);
            }
        }
        None => {
            let t = "Primeiro copie o código do seu mundo: Esc > botão Amigos (Jogue com amigos) > Comece a compartilhar > Copiar código. Depois convide de novo.";
            i.chat(texto::erro(t));
            i.historico(t, VERMELHO);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_endereco_da_config() {
        assert_eq!(ler_config(CONFIG_PADRAO), None);
        assert_eq!(
            ler_config("servidor = \"192.168.0.10:25590\""),
            Some("192.168.0.10:25590".into())
        );
        assert_eq!(
            ler_config("# x\nservidor=\"amigos.exemplo.com\" # comentário"),
            Some("amigos.exemplo.com:25590".into())
        );
        assert_eq!(
            ler_config("servidor = 10.0.0.1:4000"),
            Some("10.0.0.1:4000".into())
        );
        assert_eq!(ler_config("servidores = \"x\""), None);
    }

    #[test]
    fn segredos_sao_diferentes() {
        let a = novo_segredo();
        let b = novo_segredo();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn mensagens_do_servidor_atualizam_a_lista() {
        let mut i = Interno {
            conexao: Conexao::Conectado,
            ..Interno::default()
        };
        i.receber(DoServidor::BemVindo { nome: "Eu".into() }, true);
        i.receber(DoServidor::PedidoRecebido { nome: "Ana".into() }, true);
        assert!(i.recebidos.contains_key("ana"));
        assert!(i.aviso.is_some());
        i.receber(DoServidor::PedidoRemovido { nome: "Ana".into() }, true);
        i.receber(
            DoServidor::Amigo {
                nome: "Ana".into(),
                online: true,
                status: "No menu".into(),
            },
            true,
        );
        assert!(i.recebidos.is_empty());
        assert!(i.amigos["ana"].online);
        i.receber(
            DoServidor::Mensagem {
                de: "Ana".into(),
                texto: "oi".into(),
            },
            false,
        );
        assert_eq!(i.ultimo_de.as_deref(), Some("Ana"));
        i.receber(
            DoServidor::Convite {
                de: "Ana".into(),
                codigo: "K7QM-3XPD".into(),
            },
            false,
        );
        assert_eq!(i.ultimo_convite, Some(("Ana".into(), "K7QM-3XPD".into())));
        i.receber(DoServidor::AmigoRemovido { nome: "ana".into() }, false);
        assert!(i.amigos.is_empty());
    }

    #[test]
    fn convite_sem_codigo_nao_e_mandado() {
        let mut i = Interno {
            conexao: Conexao::Conectado,
            ..Interno::default()
        };
        convidar(&mut i, "Ana", "texto qualquer");
        assert_eq!(i.historico.back().map(|l| l.cor), Some(VERMELHO));
    }

    #[test]
    fn sem_conexao_explica() {
        let mut i = Interno::default();
        assert!(!i.mandar(DoCliente::Ping));
        assert_eq!(i.pendentes.len(), 1);
    }
}
