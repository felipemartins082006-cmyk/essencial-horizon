//! Amigos sem o jogo: a lista, os pedidos, as mensagens, os convites e os
//! comandos `/e`. Fala com o servidor pela [`crate::rede`] e devolve avisos
//! para o jogo mostrar (barra de ação ou notificação).

use crate::protocolo::{self, DoCliente, DoServidor};
use crate::rede::{Evento, Rede};
use std::collections::{BTreeMap, VecDeque};

/// Linhas guardadas para a tela de amigos.
const HISTORICO_MAX: usize = 50;
/// Avisos esperando para aparecer.
const AVISOS_MAX: usize = 20;

pub const VERDE: u32 = 0x55FF55;
pub const CINZA: u32 = 0xAAAAAA;
pub const AZUL: u32 = 0x55FFFF;
pub const VERMELHO: u32 = 0xFF5555;
pub const DOURADO: u32 = 0xFFAA00;

/// Um aviso para o jogador.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Aviso {
    /// Uma linha curta na barra de ação (respostas de comandos, erros).
    Barra(String),
    /// Uma notificação no canto (mensagem, convite, pedido), com título e texto.
    Notificacao { titulo: String, texto: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Amigo {
    pub nome: String,
    pub online: bool,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct Linha {
    pub texto: String,
    pub cor: u32,
}

/// O estado da conexão, para mostrar ao jogador.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Conexao {
    /// Sem `servidor` na configuração, ou desligado nas opções.
    #[default]
    Desligado,
    /// Esperando saber o nome do jogador.
    SemNome,
    Conectando,
    Conectado,
    /// Caiu; tentando de novo.
    Caiu(String),
    /// Recusado pelo servidor; não tenta mais.
    Recusado(String),
}

/// O que um comando `/e` pede ao jogo além dos avisos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pedido {
    Nada,
    AbrirTela,
    /// Ler a área de transferência e convidar este amigo.
    Convidar(String),
    /// Copiar este texto para a área de transferência.
    Copiar(String),
}

#[derive(Default)]
pub struct Amigos {
    pub rede: Option<Rede>,
    pub conexao: Conexao,
    pub nome: Option<String>,
    pub amigos: BTreeMap<String, Amigo>,
    pub recebidos: BTreeMap<String, String>,
    pub enviados: BTreeMap<String, String>,
    pub historico: VecDeque<Linha>,
    pub avisos: VecDeque<Aviso>,
    pub ultimo_de: Option<String>,
    pub ultimo_convite: Option<(String, String)>,
    pub status_mandado: Option<String>,
}

fn chave(nome: &str) -> String {
    nome.to_ascii_lowercase()
}

pub const AJUDA: &[(&str, &str)] = &[
    ("/amigos", "abre a tela de amigos (ou a tecla O)"),
    ("/e add <nome>", "pede amizade"),
    ("/e aceitar <nome>", "aceita um pedido"),
    ("/e recusar <nome>", "recusa um pedido"),
    ("/e remover <nome>", "desfaz a amizade ou cancela o pedido"),
    ("/e msg <nome> <texto>", "mensagem privada"),
    ("/e r <texto>", "responde a última mensagem"),
    (
        "/e convidar <nome>",
        "manda o código do seu mundo (copie antes)",
    ),
    ("/e entrar", "copia o código do último convite"),
    ("/e lista", "quem está online"),
];

impl Amigos {
    pub fn conectado(&self) -> bool {
        self.conexao == Conexao::Conectado
    }

    pub fn avisar(&mut self, aviso: Aviso) {
        if self.avisos.len() >= AVISOS_MAX {
            self.avisos.pop_front();
        }
        self.avisos.push_back(aviso);
    }

    pub fn historico(&mut self, texto: impl Into<String>, cor: u32) {
        if self.historico.len() >= HISTORICO_MAX {
            self.historico.pop_front();
        }
        self.historico.push_back(Linha {
            texto: texto.into(),
            cor,
        });
    }

    fn erro(&mut self, texto: impl Into<String>) {
        let texto = texto.into();
        self.historico(texto.clone(), VERMELHO);
        self.avisar(Aviso::Barra(texto));
    }

    /// Para de usar a conexão (amigos desligados ou servidor trocado).
    pub fn desligar(&mut self) {
        self.rede = None;
        self.conexao = Conexao::Desligado;
    }

    /// Manda um comando, ou explica por que não dá. `true` se foi.
    pub fn mandar(&mut self, cmd: DoCliente) -> bool {
        if let (Some(rede), true) = (&self.rede, self.conectado()) {
            rede.mandar(cmd);
            return true;
        }
        let motivo = match &self.conexao {
            Conexao::Desligado => {
                "Amigos desligados: coloque o servidor em config/essencial.toml.".to_owned()
            }
            Conexao::SemNome => {
                "Não sei seu nome: entre com a conta no launcher ou coloque nome em config/essencial.toml.".to_owned()
            }
            Conexao::Recusado(m) => format!("Servidor de amigos recusou: {m}"),
            _ => "Sem conexão com o servidor de amigos agora.".to_owned(),
        };
        self.erro(motivo);
        false
    }

    /// Os eventos da rede desde a última vez.
    pub fn processar(&mut self, avisos_ligados: bool) {
        let eventos = self.rede.as_ref().map(Rede::eventos).unwrap_or_default();
        for evento in eventos {
            match evento {
                Evento::Conectado => self.conexao = Conexao::Conectado,
                Evento::Caiu(motivo) => {
                    if self.conectado() {
                        self.historico(
                            "Conexão com o servidor de amigos caiu; tentando de novo.",
                            CINZA,
                        );
                        self.amigos.values_mut().for_each(|a| a.online = false);
                    }
                    self.conexao = Conexao::Caiu(motivo);
                }
                Evento::Recusado(motivo) => {
                    self.erro(format!("Servidor de amigos recusou: {motivo}"));
                    self.conexao = Conexao::Recusado(motivo);
                    self.rede = None;
                }
                Evento::Recebido(msg) => self.receber(msg, avisos_ligados),
            }
        }
    }

    /// Uma mensagem do servidor.
    pub fn receber(&mut self, msg: DoServidor, avisos: bool) {
        let notificar = |a: &mut Self, titulo: String, texto: String| {
            if avisos {
                a.avisar(Aviso::Notificacao { titulo, texto });
            }
        };
        match msg {
            DoServidor::BemVindo { nome } => {
                self.amigos.clear();
                self.recebidos.clear();
                self.enviados.clear();
                self.status_mandado = None;
                self.historico(format!("Conectado como {nome}."), VERDE);
                self.nome = Some(nome);
            }
            DoServidor::Erro { texto } => self.erro(texto),
            DoServidor::Amigo {
                nome,
                online,
                status,
            } => {
                let k = chave(&nome);
                match self.amigos.get(&k).map(|a| a.online) {
                    Some(false) if online => self.historico(format!("{nome} entrou."), CINZA),
                    Some(true) if !online => self.historico(format!("{nome} saiu."), CINZA),
                    None if self.conectado() => {
                        self.historico(format!("{nome} agora é seu amigo."), VERDE);
                        notificar(
                            self,
                            "Novo amigo".into(),
                            format!("{nome} agora é seu amigo!"),
                        );
                    }
                    _ => {}
                }
                self.amigos.insert(
                    k,
                    Amigo {
                        nome,
                        online,
                        status,
                    },
                );
            }
            DoServidor::AmigoRemovido { nome } => {
                self.amigos.remove(&chave(&nome));
                self.historico(format!("{nome} não é mais seu amigo."), CINZA);
            }
            DoServidor::PedidoRecebido { nome } => {
                if self.recebidos.insert(chave(&nome), nome.clone()).is_none() {
                    self.historico(format!("Pedido de amizade de {nome}."), DOURADO);
                    notificar(
                        self,
                        "Pedido de amizade".into(),
                        format!("{nome} quer ser seu amigo. Aperte O para responder."),
                    );
                }
            }
            DoServidor::PedidoEnviado { nome } => {
                self.enviados.insert(chave(&nome), nome);
            }
            DoServidor::PedidoRemovido { nome } => {
                let k = chave(&nome);
                self.recebidos.remove(&k);
                self.enviados.remove(&k);
            }
            DoServidor::Mensagem { de, texto } => {
                self.historico(format!("{de}: {texto}"), AZUL);
                notificar(
                    self,
                    format!("Mensagem de {de}"),
                    protocolo::limpar(&texto, 80),
                );
                self.ultimo_de = Some(de);
            }
            DoServidor::Convite { de, codigo } => {
                self.historico(format!("Convite de {de}: {codigo}"), DOURADO);
                notificar(
                    self,
                    format!("Convite de {de}"),
                    "Aperte O e clique em Copiar convite, ou digite /e entrar.".into(),
                );
                self.ultimo_convite = Some((de, codigo));
            }
            DoServidor::Pong => {}
        }
    }

    /// Manda o código do meu mundo (de `area`, a área de transferência) para `alvo`.
    pub fn convidar(&mut self, alvo: &str, area: &str) {
        match protocolo::codigo_convite(area) {
            Some(codigo) => {
                if self.mandar(DoCliente::Convite {
                    para: alvo.into(),
                    codigo,
                }) {
                    self.historico(format!("Convite mandado para {alvo}."), VERDE);
                    self.avisar(Aviso::Barra(format!("Convite mandado para {alvo}.")));
                }
            }
            None => self.erro(
                "Copie antes o código do seu mundo: Esc > Amigos > Comece a compartilhar > Copiar código.",
            ),
        }
    }

    /// Manda uma mensagem e guarda no histórico.
    pub fn mensagem(&mut self, para: &str, texto: &str) {
        let texto = protocolo::limpar(texto, protocolo::TEXTO_MAX);
        if texto.is_empty() {
            return;
        }
        if self.mandar(DoCliente::Mensagem {
            para: para.into(),
            texto: texto.clone(),
        }) {
            self.historico(format!("você → {para}: {texto}"), CINZA);
        }
    }

    /// Um comando `/e ...` (sem a barra e sem o `e`): `args` é o resto.
    pub fn comando(&mut self, args: &str) -> Pedido {
        let args = args.trim();
        let (sub, resto) = args.split_once(' ').unwrap_or((args, ""));
        let resto = resto.trim();
        let (alvo, texto) = resto.split_once(' ').unwrap_or((resto, ""));
        let sub = sub.to_ascii_lowercase();
        let precisa_alvo = [
            "add",
            "adicionar",
            "aceitar",
            "recusar",
            "remover",
            "convidar",
        ];
        if precisa_alvo.contains(&sub.as_str()) && alvo.is_empty() {
            self.erro(format!("Uso: /e {sub} <nome>"));
            return Pedido::Nada;
        }
        match sub.as_str() {
            "" | "amigos" => return Pedido::AbrirTela,
            "add" | "adicionar" => {
                self.mandar(DoCliente::Adicionar { nome: alvo.into() });
            }
            "aceitar" => {
                self.mandar(DoCliente::Aceitar { nome: alvo.into() });
            }
            "recusar" => {
                self.mandar(DoCliente::Recusar { nome: alvo.into() });
            }
            "remover" => {
                self.mandar(DoCliente::Remover { nome: alvo.into() });
            }
            "msg" | "m" => {
                if alvo.is_empty() || texto.trim().is_empty() {
                    self.erro("Uso: /e msg <nome> <texto>");
                } else {
                    self.mensagem(alvo, texto);
                }
            }
            "r" => match self.ultimo_de.clone() {
                None => self.erro("Ninguém mandou mensagem ainda."),
                Some(_) if resto.is_empty() => self.erro("Uso: /e r <texto>"),
                Some(de) => self.mensagem(&de, resto),
            },
            "convidar" => return Pedido::Convidar(alvo.to_owned()),
            "entrar" => match self.ultimo_convite.clone() {
                None => self.erro("Nenhum convite recebido ainda."),
                Some((de, codigo)) => {
                    self.avisar(Aviso::Barra(format!(
                        "Convite de {de} copiado: cole em Multijogador > Conexão direta."
                    )));
                    return Pedido::Copiar(codigo);
                }
            },
            "lista" => {
                let online: Vec<String> = self
                    .amigos
                    .values()
                    .filter(|a| a.online)
                    .map(|a| a.nome.clone())
                    .collect();
                let linha = if online.is_empty() {
                    format!("Nenhum amigo online ({} no total).", self.amigos.len())
                } else {
                    format!("Online: {}", online.join(", "))
                };
                self.historico(linha.clone(), CINZA);
                self.avisar(Aviso::Barra(linha));
            }
            _ => {
                self.historico("Comandos do Essencial:", DOURADO);
                for (c, d) in AJUDA {
                    self.historico(format!("{c} — {d}"), CINZA);
                }
                self.avisar(Aviso::Barra("Comandos na tela de amigos (tecla O).".into()));
                return Pedido::AbrirTela;
            }
        }
        Pedido::Nada
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn conectado() -> Amigos {
        Amigos {
            conexao: Conexao::Conectado,
            ..Amigos::default()
        }
    }

    #[test]
    fn mensagens_do_servidor_atualizam_a_lista() {
        let mut a = conectado();
        a.receber(DoServidor::BemVindo { nome: "Eu".into() }, true);
        a.receber(DoServidor::PedidoRecebido { nome: "Ana".into() }, true);
        assert!(a.recebidos.contains_key("ana"));
        assert!(matches!(a.avisos.back(), Some(Aviso::Notificacao { .. })));
        a.receber(DoServidor::PedidoRemovido { nome: "Ana".into() }, true);
        a.receber(
            DoServidor::Amigo {
                nome: "Ana".into(),
                online: true,
                status: String::new(),
            },
            true,
        );
        assert!(a.recebidos.is_empty() && a.amigos["ana"].online);
        a.receber(
            DoServidor::Mensagem {
                de: "Ana".into(),
                texto: "oi".into(),
            },
            false,
        );
        assert_eq!(a.ultimo_de.as_deref(), Some("Ana"));
        a.receber(
            DoServidor::Convite {
                de: "Ana".into(),
                codigo: "K7QM-3XPD".into(),
            },
            false,
        );
        assert_eq!(a.comando("entrar"), Pedido::Copiar("K7QM-3XPD".into()));
        a.receber(DoServidor::AmigoRemovido { nome: "ana".into() }, false);
        assert!(a.amigos.is_empty());
    }

    #[test]
    fn comandos() {
        let mut a = conectado();
        assert_eq!(a.comando(""), Pedido::AbrirTela);
        assert_eq!(a.comando("convidar Ana"), Pedido::Convidar("Ana".into()));
        assert_eq!(a.comando("add"), Pedido::Nada);
        assert!(matches!(a.avisos.back(), Some(Aviso::Barra(t)) if t.contains("Uso")));
        assert_eq!(a.comando("ajuda"), Pedido::AbrirTela);
        assert_eq!(a.comando("entrar"), Pedido::Nada);
    }

    #[test]
    fn sem_conexao_explica_e_convite_precisa_de_codigo() {
        let mut a = Amigos::default();
        assert!(!a.mandar(DoCliente::Ping));
        assert!(matches!(a.avisos.back(), Some(Aviso::Barra(t)) if t.contains("essencial.toml")));
        let mut a = conectado();
        a.convidar("Ana", "não é código");
        assert_eq!(a.historico.back().map(|l| l.cor), Some(VERMELHO));
    }
}
