//! A tela de amigos (tecla O ou `.amigos`): adicionar, aceitar, mandar
//! mensagem, convidar e remover, com as últimas mensagens embaixo.

use super::{Amigos, Conexao, Interno};
use crate::ID;
use crate::protocolo::{self, DoCliente};
use mod_api::client::widgets::{Button, Outcome, TextField, WidgetId, Widgets};
use mod_api::{ClientHost, Flow, ScreenRenderPostEvent, Text};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

const LINHA: f32 = 22.0;
const TOPO_LISTA: f32 = 62.0;
const LINHAS_HISTORICO: usize = 5;
const BRANCO: u32 = 0xFFFF_FFFF;
const CINZA: u32 = 0xFFAA_AAAA;
const VERDE: u32 = 0xFF55_FF55;
const DOURADO: u32 = 0xFFFF_AA00;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Acao {
    Adicionar,
    Aceitar(String),
    Recusar(String),
    Mensagem(String),
    Convidar(String),
    Remover(String),
    Cancelar(String),
    Enviar,
    Fechar,
    Anterior,
    Proxima,
}

/// Uma linha da lista.
#[derive(Clone, Debug)]
enum Item {
    Pedido(String),
    Amigo {
        nome: String,
        online: bool,
        status: String,
    },
    Enviado(String),
}

/// As peças da tela montadas para um tamanho e um estado da lista.
struct Montada {
    widgets: Widgets,
    acoes: Vec<(WidgetId, Acao)>,
    campo_nome: WidgetId,
    campo_msg: WidgetId,
    /// O que a montagem considerou: refaz quando mudar.
    chave: (u64, [u32; 2], usize, Option<String>, Option<String>),
    /// As linhas visíveis e onde ficam.
    itens: Vec<(f32, Item)>,
    paginas: usize,
}

#[derive(Default)]
struct Estado {
    montada: Option<Montada>,
    pagina: usize,
    /// Para quem vai a mensagem do campo de baixo.
    para: Option<String>,
    /// O amigo cujo X foi clicado uma vez: o segundo clique remove.
    confirmar: Option<String>,
}

#[derive(Default)]
pub struct Tela {
    estado: Mutex<Estado>,
    fechar: AtomicBool,
}

fn itens(i: &Interno) -> Vec<Item> {
    let mut v: Vec<Item> = i.recebidos.values().cloned().map(Item::Pedido).collect();
    let mut amigos: Vec<_> = i.amigos.values().collect();
    amigos.sort_by_key(|a| (!a.online, a.nome.to_ascii_lowercase()));
    v.extend(amigos.into_iter().map(|a| Item::Amigo {
        nome: a.nome.clone(),
        online: a.online,
        status: a.status.clone(),
    }));
    v.extend(i.enviados.values().cloned().map(Item::Enviado));
    v
}

fn painel(largura: f32) -> (f32, f32) {
    let w = (largura - 20.0).clamp(200.0, 340.0);
    (((largura - w) / 2.0).floor(), w)
}

fn botao(
    widgets: &mut Widgets,
    acoes: &mut Vec<(WidgetId, Acao)>,
    rect: [f32; 4],
    rotulo: &str,
    acao: Acao,
) {
    let id = widgets.add_button(Button::new(rect, Text::literal(rotulo)));
    acoes.push((id, acao));
}

impl Tela {
    pub fn abrir(&self, host: &mut dyn ClientHost) {
        {
            let mut e = self.estado.lock().unwrap();
            e.montada = None;
            e.confirmar = None;
        }
        host.mod_screen_open(ID);
    }

    pub fn tick(&self, host: &mut dyn ClientHost) {
        if self.fechar.swap(false, Ordering::Relaxed) {
            host.mod_screen_close(ID);
        }
    }

    fn montar(e: &mut Estado, i: &Interno, host: &dyn ClientHost, tamanho: [f32; 2]) {
        let [largura, altura] = tamanho;
        let chave = (
            i.versao,
            [largura as u32, altura as u32],
            e.pagina,
            e.para.clone(),
            e.confirmar.clone(),
        );
        if e.montada.as_ref().is_some_and(|m| m.chave == chave) {
            return;
        }
        // Guardar o que estava digitado e o foco.
        let (nome_antes, msg_antes, foco_antes) = match &e.montada {
            Some(m) => (
                m.widgets
                    .text_field(m.campo_nome)
                    .map(|f| f.value().to_owned()),
                m.widgets
                    .text_field(m.campo_msg)
                    .map(|f| f.value().to_owned()),
                m.widgets.focused().and_then(|f| {
                    if f == m.campo_nome {
                        Some(0)
                    } else if f == m.campo_msg {
                        Some(1)
                    } else {
                        None
                    }
                }),
            ),
            None => (None, None, None),
        };

        let (x0, w) = painel(largura);
        let mut widgets = Widgets::new();
        let mut acoes = Vec::new();

        let mut campo = TextField::new([x0, 34.0, w - 80.0, 20.0], 16);
        campo.suggestion = Some("Nome do jogador".into());
        let campo_nome = widgets.add_text_field(campo);
        botao(
            &mut widgets,
            &mut acoes,
            [x0 + w - 76.0, 34.0, 76.0, 20.0],
            "Adicionar",
            Acao::Adicionar,
        );

        // A lista, em páginas.
        let fim_lista = altura - 34.0 - LINHAS_HISTORICO as f32 * 10.0 - 8.0;
        let por_pagina = (((fim_lista - TOPO_LISTA) / LINHA).floor() as usize).max(1);
        let todos = itens(i);
        let paginas = todos.len().div_ceil(por_pagina).max(1);
        e.pagina = e.pagina.min(paginas - 1);
        let mut visiveis = Vec::new();
        for (n, item) in todos
            .into_iter()
            .skip(e.pagina * por_pagina)
            .take(por_pagina)
            .enumerate()
        {
            let y = TOPO_LISTA + n as f32 * LINHA;
            let direita = x0 + w;
            match &item {
                Item::Pedido(nome) => {
                    botao(
                        &mut widgets,
                        &mut acoes,
                        [direita - 110.0, y, 54.0, 20.0],
                        "Aceitar",
                        Acao::Aceitar(nome.clone()),
                    );
                    botao(
                        &mut widgets,
                        &mut acoes,
                        [direita - 54.0, y, 54.0, 20.0],
                        "Recusar",
                        Acao::Recusar(nome.clone()),
                    );
                }
                Item::Amigo { nome, online, .. } => {
                    let confirmando = e.confirmar.as_deref() == Some(nome.as_str());
                    if *online {
                        botao(
                            &mut widgets,
                            &mut acoes,
                            [direita - 136.0, y, 40.0, 20.0],
                            "Msg",
                            Acao::Mensagem(nome.clone()),
                        );
                        botao(
                            &mut widgets,
                            &mut acoes,
                            [direita - 94.0, y, 64.0, 20.0],
                            "Convidar",
                            Acao::Convidar(nome.clone()),
                        );
                    }
                    let rotulo = if confirmando { "?" } else { "X" };
                    botao(
                        &mut widgets,
                        &mut acoes,
                        [direita - 26.0, y, 26.0, 20.0],
                        rotulo,
                        Acao::Remover(nome.clone()),
                    );
                }
                Item::Enviado(nome) => {
                    botao(
                        &mut widgets,
                        &mut acoes,
                        [direita - 64.0, y, 64.0, 20.0],
                        "Cancelar",
                        Acao::Cancelar(nome.clone()),
                    );
                }
            }
            visiveis.push((y, item));
        }
        if paginas > 1 {
            botao(
                &mut widgets,
                &mut acoes,
                [x0 + w - 44.0, 8.0, 20.0, 20.0],
                "<",
                Acao::Anterior,
            );
            botao(
                &mut widgets,
                &mut acoes,
                [x0 + w - 20.0, 8.0, 20.0, 20.0],
                ">",
                Acao::Proxima,
            );
        }

        // Embaixo: a mensagem e fechar.
        let y = altura - 28.0;
        let mut campo = TextField::new([x0, y, w - 120.0, 20.0], protocolo::TEXTO_MAX);
        campo.suggestion = Some(match &e.para {
            Some(p) => format!("Mensagem para {p}"),
            None => "Clique em Msg num amigo".into(),
        });
        campo.active = e.para.is_some();
        let campo_msg = widgets.add_text_field(campo);
        botao(
            &mut widgets,
            &mut acoes,
            [x0 + w - 116.0, y, 56.0, 20.0],
            "Enviar",
            Acao::Enviar,
        );
        botao(
            &mut widgets,
            &mut acoes,
            [x0 + w - 56.0, y, 56.0, 20.0],
            "Fechar",
            Acao::Fechar,
        );

        if let (Some(v), Some(f)) = (nome_antes, widgets.text_field_mut(campo_nome)) {
            f.set_value(&v);
        }
        if let (Some(v), Some(f)) = (msg_antes, widgets.text_field_mut(campo_msg)) {
            f.set_value(&v);
        }
        match foco_antes {
            Some(0) => widgets.set_focus(host, Some(campo_nome)),
            Some(_) if e.para.is_some() => widgets.set_focus(host, Some(campo_msg)),
            _ => {}
        }
        e.montada = Some(Montada {
            widgets,
            acoes,
            campo_nome,
            campo_msg,
            chave,
            itens: visiveis,
            paginas,
        });
    }

    pub(super) fn desenhar(
        &self,
        amigos: &Amigos,
        host: &dyn ClientHost,
        event: &mut ScreenRenderPostEvent<'_>,
    ) {
        if host.mod_screen() != Some(ID) {
            return;
        }
        let tamanho = event.draw.size();
        let mut e = self.estado.lock().unwrap();
        let i = amigos.interno.lock().unwrap();
        Self::montar(&mut e, &i, host, tamanho);
        let [largura, altura] = tamanho;
        let (x0, w) = painel(largura);
        let d = &mut *event.draw;
        d.fill([0.0, 0.0, largura, altura], 0xA000_0000);

        d.text("Amigos — Essencial", [x0, 10.0], BRANCO, true);
        let (estado, cor) = match &i.conexao {
            Conexao::Desligado => (
                "Desligado: coloque o servidor em config/essencial.toml".to_owned(),
                CINZA,
            ),
            Conexao::EsperandoNome => ("Entre num mundo para conectar".to_owned(), CINZA),
            Conexao::Conectando => ("Conectando...".to_owned(), CINZA),
            Conexao::Conectado => (
                format!(
                    "Conectado como {} · {}/{} online",
                    i.nome.as_deref().unwrap_or("?"),
                    i.amigos.values().filter(|a| a.online).count(),
                    i.amigos.len()
                ),
                VERDE,
            ),
            Conexao::Caiu(m) => (format!("Sem conexão ({m}); tentando de novo"), DOURADO),
            Conexao::Recusado(m) => (format!("Recusado: {m}"), 0xFFFF_5555),
        };
        d.text(&protocolo::limpar(&estado, 70), [x0, 22.0], cor, true);

        let pagina = e.pagina;
        let Some(m) = e.montada.as_mut() else {
            return;
        };
        if m.itens.is_empty() {
            d.text(
                "Nenhum amigo ainda. Escreva um nome acima e clique em Adicionar.",
                [x0, TOPO_LISTA + 6.0],
                CINZA,
                true,
            );
        }
        for (y, item) in &m.itens {
            let ty = y + 6.0;
            match item {
                Item::Pedido(nome) => d.text(&format!("Pedido de {nome}"), [x0, ty], DOURADO, true),
                Item::Amigo {
                    nome,
                    online,
                    status,
                } => {
                    let bolinha = if *online { VERDE } else { 0xFF55_5555 };
                    d.fill([x0, ty + 1.0, 5.0, 5.0], bolinha);
                    d.text(
                        nome,
                        [x0 + 9.0, ty],
                        if *online { BRANCO } else { CINZA },
                        true,
                    );
                    if *online && !status.is_empty() {
                        let nx = x0
                            + 9.0
                            + host.text_width(&Text::literal(nome.as_str())).unwrap_or(60) as f32
                            + 6.0;
                        let espaco = (x0 + w - 140.0 - nx).max(0.0);
                        let max = (espaco / 6.0) as usize;
                        if max > 3 {
                            d.text(&protocolo::limpar(status, max), [nx, ty], CINZA, true);
                        }
                    }
                }
                Item::Enviado(nome) => d.text(&format!("Aguardando {nome}"), [x0, ty], CINZA, true),
            }
        }
        if m.paginas > 1 {
            d.text(
                &format!("{}/{}", pagina + 1, m.paginas),
                [x0 + w - 80.0, 14.0],
                CINZA,
                true,
            );
        }

        // As últimas linhas do histórico.
        let topo = altura - 34.0 - LINHAS_HISTORICO as f32 * 10.0;
        for (n, linha) in i
            .historico
            .iter()
            .rev()
            .take(LINHAS_HISTORICO)
            .rev()
            .enumerate()
        {
            d.text(
                &protocolo::limpar(&linha.texto, 60),
                [x0, topo + n as f32 * 10.0],
                linha.cor,
                true,
            );
        }
        drop(i);
        let _ = m.widgets.draw(d, host, event.screen.mouse);
    }

    /// Uma entrada (mouse ou teclado) na tela aberta.
    pub(super) fn entrada(
        &self,
        amigos: &Amigos,
        host: &dyn ClientHost,
        entrada: impl FnOnce(&mut Widgets) -> Outcome,
    ) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let mut e = self.estado.lock().unwrap();
        let Some(m) = e.montada.as_mut() else {
            return Flow::Continue;
        };
        let resultado = entrada(&mut m.widgets);
        let acao = match resultado {
            Outcome::Pressed(id) => m
                .acoes
                .iter()
                .find(|(w, _)| *w == id)
                .map(|(_, a)| a.clone()),
            Outcome::Submitted(id) if id == m.campo_nome => Some(Acao::Adicionar),
            Outcome::Submitted(id) if id == m.campo_msg => Some(Acao::Enviar),
            _ => None,
        };
        if let Some(acao) = acao {
            self.agir(&mut e, amigos, host, acao);
        }
        if resultado.handled() {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn agir(&self, e: &mut Estado, amigos: &Amigos, host: &dyn ClientHost, acao: Acao) {
        let mut i = amigos.interno.lock().unwrap();
        if !matches!(acao, Acao::Remover(_)) {
            e.confirmar = None;
        }
        let campo = |e: &mut Estado, nome: bool| -> Option<String> {
            let m = e.montada.as_mut()?;
            let id = if nome { m.campo_nome } else { m.campo_msg };
            let f = m.widgets.text_field_mut(id)?;
            let v = f.value().trim().to_owned();
            f.set_value("");
            Some(v)
        };
        match acao {
            Acao::Adicionar => {
                if let Some(nome) = campo(e, true).filter(|n| !n.is_empty()) {
                    if protocolo::nome_valido(&nome) {
                        i.mandar(DoCliente::Adicionar { nome });
                    } else {
                        i.historico(format!("Nome inválido: {nome}"), 0xFFFF_5555);
                    }
                }
            }
            Acao::Aceitar(nome) => {
                i.mandar(DoCliente::Aceitar { nome });
            }
            Acao::Recusar(nome) => {
                i.mandar(DoCliente::Recusar { nome });
            }
            Acao::Cancelar(nome) => {
                i.mandar(DoCliente::Remover { nome });
            }
            Acao::Remover(nome) => {
                if e.confirmar.as_deref() == Some(nome.as_str()) {
                    e.confirmar = None;
                    i.mandar(DoCliente::Remover { nome });
                } else {
                    i.historico(format!("Clique em ? de novo para remover {nome}."), CINZA);
                    e.confirmar = Some(nome);
                }
            }
            Acao::Mensagem(nome) => {
                e.para = Some(nome);
            }
            Acao::Convidar(nome) => {
                let area = host.clipboard_text().unwrap_or_default();
                super::convidar(&mut i, &nome, &area);
            }
            Acao::Enviar => {
                if let (Some(para), Some(texto)) = (e.para.clone(), campo(e, false))
                    && !texto.is_empty()
                {
                    let texto = protocolo::limpar(&texto, protocolo::TEXTO_MAX);
                    if i.mandar(DoCliente::Mensagem {
                        para: para.clone(),
                        texto: texto.clone(),
                    }) {
                        i.historico(format!("você → {para}: {texto}"), CINZA);
                    }
                }
            }
            Acao::Fechar => self.fechar.store(true, Ordering::Relaxed),
            Acao::Anterior => e.pagina = e.pagina.saturating_sub(1),
            Acao::Proxima => e.pagina += 1,
        }
    }
}
