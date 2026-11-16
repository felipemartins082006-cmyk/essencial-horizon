//! O estado do servidor e as regras de amizade, sem rede: cada comando
//! devolve as mensagens a mandar e para quem, o que deixa tudo testável.

use crate::protocolo::{self, DoCliente, DoServidor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};

/// Máximo de pedidos de amizade esperando resposta, por pessoa.
pub const PEDIDOS_MAX: usize = 100;

/// Máximo de amigos por pessoa.
pub const AMIGOS_MAX: usize = 500;

/// Uma conta: o nome como foi escrito na primeira vez e o resumo SHA-256 do
/// segredo do computador dela. O segredo em si nunca é guardado.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usuario {
    pub nome: String,
    pub segredo_sha256: String,
    /// Amigos, pela chave (nome em minúsculas).
    pub amigos: BTreeSet<String>,
    /// Quem pediu amizade a esta pessoa e ainda espera.
    pub pedidos_recebidos: BTreeSet<String>,
}

/// O que é salvo em disco.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Salvo {
    pub usuarios: HashMap<String, Usuario>,
}

/// Uma pessoa conectada agora.
#[derive(Clone, Debug)]
struct Online {
    sessao: u64,
    status: String,
}

/// Uma mensagem a mandar para a sessão `sessao`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envio {
    pub sessao: u64,
    pub mensagem: DoServidor,
}

/// O resultado de um `OLA`.
#[derive(Debug, PartialEq, Eq)]
pub enum Entrada {
    /// Entrou; `chave` identifica a conta nos próximos comandos. `derrubar`
    /// é a sessão antiga da mesma conta, que deve ser fechada.
    Aceita {
        chave: String,
        derrubar: Option<u64>,
    },
    /// Recusada, com o motivo para mandar antes de fechar.
    Recusada(String),
}

/// Todo o estado: contas salvas e quem está online.
#[derive(Debug, Default)]
pub struct Estado {
    pub salvo: Salvo,
    online: HashMap<String, Online>,
    /// As contas mudaram desde o último salvamento.
    pub sujo: bool,
}

fn chave(nome: &str) -> String {
    nome.to_ascii_lowercase()
}

fn resumo(segredo: &str) -> String {
    let bytes = Sha256::digest(segredo.as_bytes());
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Comparação que não para no primeiro byte diferente.
fn iguais(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn erro(sessao: u64, texto: impl Into<String>) -> Envio {
    Envio {
        sessao,
        mensagem: DoServidor::Erro {
            texto: texto.into(),
        },
    }
}

impl Estado {
    /// O estado a partir do que estava salvo.
    pub fn novo(salvo: Salvo) -> Self {
        Self {
            salvo,
            online: HashMap::new(),
            sujo: false,
        }
    }

    /// Quantas pessoas estão online.
    pub fn quantos_online(&self) -> usize {
        self.online.len()
    }

    fn nome(&self, chave: &str) -> String {
        self.salvo
            .usuarios
            .get(chave)
            .map_or_else(|| chave.to_owned(), |u| u.nome.clone())
    }

    /// Como `chave` aparece para os amigos agora.
    fn como_amigo(&self, chave: &str) -> DoServidor {
        let online = self.online.get(chave);
        DoServidor::Amigo {
            nome: self.nome(chave),
            online: online.is_some(),
            status: online.map(|o| o.status.clone()).unwrap_or_default(),
        }
    }

    /// Manda `mensagem` para `chave` se estiver online.
    fn para(&self, chave: &str, mensagem: DoServidor, saida: &mut Vec<Envio>) {
        if let Some(o) = self.online.get(chave) {
            saida.push(Envio {
                sessao: o.sessao,
                mensagem,
            });
        }
    }

    /// Conta a todos os amigos online de `chave` como ela está agora.
    fn avisar_amigos(&self, chave: &str, saida: &mut Vec<Envio>) {
        let Some(u) = self.salvo.usuarios.get(chave) else {
            return;
        };
        let msg = self.como_amigo(chave);
        for amigo in &u.amigos {
            self.para(amigo, msg.clone(), saida);
        }
    }

    /// Um `OLA` da sessão `sessao`. Em caso de sucesso, `saida` recebe a
    /// lista inicial e os avisos aos amigos.
    pub fn entrar(
        &mut self,
        sessao: u64,
        nome: &str,
        segredo: &str,
        versao: u32,
        saida: &mut Vec<Envio>,
    ) -> Entrada {
        if versao != protocolo::VERSAO {
            return Entrada::Recusada(format!(
                "Versão do mod diferente da do servidor (mod {versao}, servidor {}). Atualize o Essencial.",
                protocolo::VERSAO
            ));
        }
        if !protocolo::nome_valido(nome) {
            return Entrada::Recusada(format!("Nome de jogador inválido: {nome}"));
        }
        if !(16..=128).contains(&segredo.len()) {
            return Entrada::Recusada("Segredo inválido.".into());
        }
        let k = chave(nome);
        let r = resumo(segredo);
        match self.salvo.usuarios.get_mut(&k) {
            Some(u) if !iguais(&u.segredo_sha256, &r) => {
                return Entrada::Recusada(format!(
                    "O nome {} já foi usado neste servidor por outro computador.",
                    u.nome
                ));
            }
            Some(u) => {
                if u.nome != nome {
                    // Mesma pessoa, maiúsculas diferentes (mudou o nome na conta).
                    u.nome = nome.to_owned();
                    self.sujo = true;
                }
            }
            None => {
                self.salvo.usuarios.insert(
                    k.clone(),
                    Usuario {
                        nome: nome.to_owned(),
                        segredo_sha256: r,
                        ..Usuario::default()
                    },
                );
                self.sujo = true;
            }
        }
        let derrubar = self
            .online
            .insert(
                k.clone(),
                Online {
                    sessao,
                    status: String::new(),
                },
            )
            .map(|o| o.sessao);
        if let Some(antiga) = derrubar {
            saida.push(erro(antiga, "Você entrou no Essencial em outro lugar."));
        }
        let eu = &self.salvo.usuarios[&k];
        saida.push(Envio {
            sessao,
            mensagem: DoServidor::BemVindo {
                nome: eu.nome.clone(),
            },
        });
        for amigo in &eu.amigos {
            saida.push(Envio {
                sessao,
                mensagem: self.como_amigo(amigo),
            });
        }
        for de in &eu.pedidos_recebidos {
            saida.push(Envio {
                sessao,
                mensagem: DoServidor::PedidoRecebido {
                    nome: self.nome(de),
                },
            });
        }
        for (outro, u) in &self.salvo.usuarios {
            if u.pedidos_recebidos.contains(&k) {
                saida.push(Envio {
                    sessao,
                    mensagem: DoServidor::PedidoEnviado {
                        nome: self.nome(outro),
                    },
                });
            }
        }
        self.avisar_amigos(&k, saida);
        Entrada::Aceita { chave: k, derrubar }
    }

    /// A sessão `sessao` da conta `chave` caiu.
    pub fn sair(&mut self, chave: &str, sessao: u64, saida: &mut Vec<Envio>) {
        if self.online.get(chave).is_some_and(|o| o.sessao == sessao) {
            self.online.remove(chave);
            self.avisar_amigos(chave, saida);
        }
    }

    fn amigos(&self, a: &str, b: &str) -> bool {
        self.salvo
            .usuarios
            .get(a)
            .is_some_and(|u| u.amigos.contains(b))
    }

    fn virar_amigos(&mut self, a: &str, b: &str, saida: &mut Vec<Envio>) {
        for (x, y) in [(a, b), (b, a)] {
            if let Some(u) = self.salvo.usuarios.get_mut(x) {
                u.pedidos_recebidos.remove(y);
                u.amigos.insert(y.to_owned());
            }
        }
        self.sujo = true;
        let (na, nb) = (self.nome(a), self.nome(b));
        self.para(a, DoServidor::PedidoRemovido { nome: nb }, saida);
        self.para(b, DoServidor::PedidoRemovido { nome: na }, saida);
        self.para(a, self.como_amigo(b), saida);
        self.para(b, self.como_amigo(a), saida);
    }

    /// Um comando da conta `eu` (já entrou), da sessão `sessao`.
    pub fn comando(&mut self, eu: &str, sessao: u64, cmd: DoCliente, saida: &mut Vec<Envio>) {
        if self.online.get(eu).is_none_or(|o| o.sessao != sessao) {
            return;
        }
        // O alvo do comando, se ele tem um: precisa ser outra conta existente.
        let alvo = match &cmd {
            DoCliente::Adicionar { nome }
            | DoCliente::Aceitar { nome }
            | DoCliente::Recusar { nome }
            | DoCliente::Remover { nome }
            | DoCliente::Mensagem { para: nome, .. }
            | DoCliente::Convite { para: nome, .. } => {
                let k = chave(nome);
                if k == eu {
                    saida.push(erro(sessao, "Esse é você."));
                    return;
                }
                if !self.salvo.usuarios.contains_key(&k) {
                    saida.push(erro(
                        sessao,
                        format!("Ninguém chamado {nome} usou o Essencial neste servidor ainda."),
                    ));
                    return;
                }
                Some(k)
            }
            _ => None,
        };
        let alvo = alvo.as_deref().unwrap_or_default();
        match cmd {
            DoCliente::Ola { .. } => saida.push(erro(sessao, "Você já entrou.")),
            DoCliente::Ping => saida.push(Envio {
                sessao,
                mensagem: DoServidor::Pong,
            }),
            DoCliente::Adicionar { .. } => {
                if self.amigos(eu, alvo) {
                    saida.push(erro(sessao, format!("{} já é seu amigo.", self.nome(alvo))));
                } else if self.salvo.usuarios[eu].pedidos_recebidos.contains(alvo) {
                    // A outra pessoa já tinha pedido: adicionar é aceitar.
                    self.virar_amigos(eu, alvo, saida);
                } else if self.salvo.usuarios[eu].amigos.len() >= AMIGOS_MAX {
                    saida.push(erro(sessao, "Você chegou ao limite de amigos."));
                } else if self.salvo.usuarios[alvo].pedidos_recebidos.contains(eu) {
                    saida.push(erro(sessao, "Você já pediu; espere a resposta."));
                } else if self.salvo.usuarios[alvo].pedidos_recebidos.len() >= PEDIDOS_MAX {
                    saida.push(erro(
                        sessao,
                        format!("{} tem pedidos demais esperando.", self.nome(alvo)),
                    ));
                } else {
                    if let Some(u) = self.salvo.usuarios.get_mut(alvo) {
                        u.pedidos_recebidos.insert(eu.to_owned());
                    }
                    self.sujo = true;
                    saida.push(Envio {
                        sessao,
                        mensagem: DoServidor::PedidoEnviado {
                            nome: self.nome(alvo),
                        },
                    });
                    self.para(
                        alvo,
                        DoServidor::PedidoRecebido {
                            nome: self.nome(eu),
                        },
                        saida,
                    );
                }
            }
            DoCliente::Aceitar { .. } => {
                if self.salvo.usuarios[eu].pedidos_recebidos.contains(alvo) {
                    self.virar_amigos(eu, alvo, saida);
                } else {
                    saida.push(erro(
                        sessao,
                        format!("{} não pediu sua amizade.", self.nome(alvo)),
                    ));
                }
            }
            DoCliente::Recusar { .. } => {
                if let Some(u) = self.salvo.usuarios.get_mut(eu)
                    && u.pedidos_recebidos.remove(alvo)
                {
                    self.sujo = true;
                    self.para(
                        eu,
                        DoServidor::PedidoRemovido {
                            nome: self.nome(alvo),
                        },
                        saida,
                    );
                    self.para(
                        alvo,
                        DoServidor::PedidoRemovido {
                            nome: self.nome(eu),
                        },
                        saida,
                    );
                }
            }
            DoCliente::Remover { .. } => {
                if self.amigos(eu, alvo) {
                    for (x, y) in [(eu, alvo), (alvo, eu)] {
                        if let Some(u) = self.salvo.usuarios.get_mut(x) {
                            u.amigos.remove(y);
                        }
                    }
                    self.sujo = true;
                    self.para(
                        eu,
                        DoServidor::AmigoRemovido {
                            nome: self.nome(alvo),
                        },
                        saida,
                    );
                    self.para(
                        alvo,
                        DoServidor::AmigoRemovido {
                            nome: self.nome(eu),
                        },
                        saida,
                    );
                } else if let Some(u) = self.salvo.usuarios.get_mut(alvo)
                    && u.pedidos_recebidos.remove(eu)
                {
                    // Cancelar um pedido que eu fiz.
                    self.sujo = true;
                    self.para(
                        eu,
                        DoServidor::PedidoRemovido {
                            nome: self.nome(alvo),
                        },
                        saida,
                    );
                    self.para(
                        alvo,
                        DoServidor::PedidoRemovido {
                            nome: self.nome(eu),
                        },
                        saida,
                    );
                }
            }
            DoCliente::Mensagem { texto, .. } => {
                let texto = protocolo::limpar(&texto, protocolo::TEXTO_MAX);
                if texto.is_empty() {
                } else if !self.amigos(eu, alvo) {
                    saida.push(erro(
                        sessao,
                        format!("{} não é seu amigo.", self.nome(alvo)),
                    ));
                } else if !self.online.contains_key(alvo) {
                    saida.push(erro(sessao, format!("{} está offline.", self.nome(alvo))));
                } else {
                    self.para(
                        alvo,
                        DoServidor::Mensagem {
                            de: self.nome(eu),
                            texto,
                        },
                        saida,
                    );
                }
            }
            DoCliente::Convite { codigo, .. } => {
                if !self.amigos(eu, alvo) {
                    saida.push(erro(
                        sessao,
                        format!("{} não é seu amigo.", self.nome(alvo)),
                    ));
                } else if !self.online.contains_key(alvo) {
                    saida.push(erro(sessao, format!("{} está offline.", self.nome(alvo))));
                } else if let Some(codigo) = protocolo::codigo_convite(&codigo) {
                    self.para(
                        alvo,
                        DoServidor::Convite {
                            de: self.nome(eu),
                            codigo,
                        },
                        saida,
                    );
                } else {
                    saida.push(erro(sessao, "Código de convite inválido."));
                }
            }
            DoCliente::Status { texto } => {
                let texto = protocolo::limpar(&texto, protocolo::STATUS_MAX);
                if let Some(o) = self.online.get_mut(eu)
                    && o.status != texto
                {
                    o.status = texto;
                    self.avisar_amigos(eu, saida);
                }
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const SEGREDO_A: &str = "segredo-da-ana-123456";
    const SEGREDO_B: &str = "segredo-do-bia-123456";

    fn entrar(e: &mut Estado, sessao: u64, nome: &str, segredo: &str) -> Vec<Envio> {
        let mut saida = Vec::new();
        let r = e.entrar(sessao, nome, segredo, protocolo::VERSAO, &mut saida);
        assert!(matches!(r, Entrada::Aceita { .. }), "{r:?}");
        saida
    }

    fn cmd(e: &mut Estado, eu: &str, sessao: u64, c: DoCliente) -> Vec<Envio> {
        let mut saida = Vec::new();
        e.comando(&chave(eu), sessao, c, &mut saida);
        saida
    }

    fn para(saida: &[Envio], sessao: u64) -> Vec<DoServidor> {
        saida
            .iter()
            .filter(|e| e.sessao == sessao)
            .map(|e| e.mensagem.clone())
            .collect()
    }

    #[test]
    fn nome_fica_com_o_primeiro_computador() {
        let mut e = Estado::default();
        entrar(&mut e, 1, "Ana", SEGREDO_A);
        let mut saida = Vec::new();
        let r = e.entrar(2, "ana", SEGREDO_B, protocolo::VERSAO, &mut saida);
        assert!(matches!(r, Entrada::Recusada(_)));
        assert!(saida.is_empty());
        assert!(!e.salvo.usuarios["ana"].segredo_sha256.contains("segredo"));
    }

    #[test]
    fn versao_errada_e_recusada() {
        let mut e = Estado::default();
        let mut saida = Vec::new();
        let r = e.entrar(1, "Ana", SEGREDO_A, protocolo::VERSAO + 1, &mut saida);
        assert!(matches!(r, Entrada::Recusada(_)));
    }

    #[test]
    fn entrar_de_novo_derruba_a_sessao_antiga() {
        let mut e = Estado::default();
        entrar(&mut e, 1, "Ana", SEGREDO_A);
        let mut saida = Vec::new();
        let r = e.entrar(2, "Ana", SEGREDO_A, protocolo::VERSAO, &mut saida);
        assert_eq!(
            r,
            Entrada::Aceita {
                chave: "ana".into(),
                derrubar: Some(1)
            }
        );
        // A sessão antiga cair depois não tira a nova do ar.
        e.sair("ana", 1, &mut saida);
        assert_eq!(e.quantos_online(), 1);
    }

    #[test]
    fn pedido_aceito_vira_amizade_e_mensagens_passam() {
        let mut e = Estado::default();
        entrar(&mut e, 1, "Ana", SEGREDO_A);
        entrar(&mut e, 2, "Bia", SEGREDO_B);

        let s = cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Adicionar { nome: "bia".into() },
        );
        assert_eq!(
            para(&s, 1),
            [DoServidor::PedidoEnviado { nome: "Bia".into() }]
        );
        assert_eq!(
            para(&s, 2),
            [DoServidor::PedidoRecebido { nome: "Ana".into() }]
        );

        // Mensagem antes de ser amigo: recusada.
        let s = cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Mensagem {
                para: "Bia".into(),
                texto: "oi".into(),
            },
        );
        assert!(matches!(para(&s, 1)[..], [DoServidor::Erro { .. }]));
        assert!(para(&s, 2).is_empty());

        let s = cmd(&mut e, "Bia", 2, DoCliente::Aceitar { nome: "Ana".into() });
        assert!(para(&s, 1).contains(&DoServidor::Amigo {
            nome: "Bia".into(),
            online: true,
            status: String::new()
        }));
        assert!(para(&s, 2).contains(&DoServidor::PedidoRemovido { nome: "Ana".into() }));
        assert!(e.salvo.usuarios["ana"].amigos.contains("bia"));
        assert!(e.salvo.usuarios["bia"].amigos.contains("ana"));
        assert!(e.salvo.usuarios["bia"].pedidos_recebidos.is_empty());

        let s = cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Mensagem {
                para: "Bia".into(),
                texto: " oi\n".into(),
            },
        );
        assert_eq!(
            para(&s, 2),
            [DoServidor::Mensagem {
                de: "Ana".into(),
                texto: "oi".into()
            }]
        );
    }

    #[test]
    fn pedidos_cruzados_viram_amizade() {
        let mut e = Estado::default();
        entrar(&mut e, 1, "Ana", SEGREDO_A);
        entrar(&mut e, 2, "Bia", SEGREDO_B);
        cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Adicionar { nome: "Bia".into() },
        );
        cmd(
            &mut e,
            "Bia",
            2,
            DoCliente::Adicionar { nome: "Ana".into() },
        );
        assert!(e.salvo.usuarios["ana"].amigos.contains("bia"));
    }

    #[test]
    fn convite_so_com_codigo_valido_e_amigo_online() {
        let mut e = Estado::default();
        entrar(&mut e, 1, "Ana", SEGREDO_A);
        entrar(&mut e, 2, "Bia", SEGREDO_B);
        cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Adicionar { nome: "Bia".into() },
        );
        cmd(&mut e, "Bia", 2, DoCliente::Aceitar { nome: "Ana".into() });

        let s = cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Convite {
                para: "Bia".into(),
                codigo: "não é código".into(),
            },
        );
        assert!(matches!(para(&s, 1)[..], [DoServidor::Erro { .. }]));
        let s = cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Convite {
                para: "Bia".into(),
                codigo: "K7QM-3XPD".into(),
            },
        );
        assert_eq!(
            para(&s, 2),
            [DoServidor::Convite {
                de: "Ana".into(),
                codigo: "K7QM-3XPD".into()
            }]
        );

        let mut saida = Vec::new();
        e.sair("bia", 2, &mut saida);
        assert_eq!(
            para(&saida, 1),
            [DoServidor::Amigo {
                nome: "Bia".into(),
                online: false,
                status: String::new()
            }]
        );
        let s = cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Convite {
                para: "Bia".into(),
                codigo: "K7QM-3XPD".into(),
            },
        );
        assert!(matches!(para(&s, 1)[..], [DoServidor::Erro { .. }]));
    }

    #[test]
    fn remover_e_recusar() {
        let mut e = Estado::default();
        entrar(&mut e, 1, "Ana", SEGREDO_A);
        entrar(&mut e, 2, "Bia", SEGREDO_B);
        cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Adicionar { nome: "Bia".into() },
        );
        let s = cmd(&mut e, "Bia", 2, DoCliente::Recusar { nome: "Ana".into() });
        assert_eq!(
            para(&s, 1),
            [DoServidor::PedidoRemovido { nome: "Bia".into() }]
        );
        assert!(e.salvo.usuarios["bia"].pedidos_recebidos.is_empty());

        // Cancelar um pedido enviado.
        cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Adicionar { nome: "Bia".into() },
        );
        cmd(&mut e, "Ana", 1, DoCliente::Remover { nome: "Bia".into() });
        assert!(e.salvo.usuarios["bia"].pedidos_recebidos.is_empty());

        cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Adicionar { nome: "Bia".into() },
        );
        cmd(&mut e, "Bia", 2, DoCliente::Aceitar { nome: "Ana".into() });
        let s = cmd(&mut e, "Bia", 2, DoCliente::Remover { nome: "Ana".into() });
        assert_eq!(
            para(&s, 1),
            [DoServidor::AmigoRemovido { nome: "Bia".into() }]
        );
        assert!(e.salvo.usuarios["ana"].amigos.is_empty());
    }

    #[test]
    fn status_e_lista_inicial() {
        let mut e = Estado::default();
        entrar(&mut e, 1, "Ana", SEGREDO_A);
        entrar(&mut e, 2, "Bia", SEGREDO_B);
        cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Adicionar { nome: "Bia".into() },
        );
        cmd(&mut e, "Bia", 2, DoCliente::Aceitar { nome: "Ana".into() });
        let s = cmd(
            &mut e,
            "Bia",
            2,
            DoCliente::Status {
                texto: "Jogando sozinho".into(),
            },
        );
        assert_eq!(
            para(&s, 1),
            [DoServidor::Amigo {
                nome: "Bia".into(),
                online: true,
                status: "Jogando sozinho".into()
            }]
        );
        // Ana entra de novo: recebe Bia online com o status.
        let s = entrar(&mut e, 3, "Ana", SEGREDO_A);
        assert!(para(&s, 3).contains(&DoServidor::Amigo {
            nome: "Bia".into(),
            online: true,
            status: "Jogando sozinho".into()
        }));
    }

    #[test]
    fn alvos_invalidos() {
        let mut e = Estado::default();
        entrar(&mut e, 1, "Ana", SEGREDO_A);
        let s = cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Adicionar { nome: "Ana".into() },
        );
        assert!(matches!(para(&s, 1)[..], [DoServidor::Erro { .. }]));
        let s = cmd(
            &mut e,
            "Ana",
            1,
            DoCliente::Adicionar {
                nome: "Ninguem".into(),
            },
        );
        assert!(matches!(para(&s, 1)[..], [DoServidor::Erro { .. }]));
        // Uma sessão que não é a atual não manda comandos.
        let s = cmd(&mut e, "Ana", 9, DoCliente::Ping);
        assert!(s.is_empty());
    }

    #[test]
    fn salvo_vira_json_e_volta() {
        let mut e = Estado::default();
        entrar(&mut e, 1, "Ana", SEGREDO_A);
        let json = serde_json::to_string(&e.salvo).unwrap();
        let de_volta: Salvo = serde_json::from_str(&json).unwrap();
        assert_eq!(de_volta, e.salvo);
    }
}
