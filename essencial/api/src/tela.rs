//! A tela de amigos, desenhada pelo mod (tecla O ou `/amigos`): campos de
//! texto e botões feitos com retângulos e texto, os cliques testados pelos
//! retângulos guardados no último desenho.

use crate::amigos::{Amigos, CINZA, Conexao, DOURADO, VERDE, VERMELHO};
use crate::protocolo::{self, DoCliente};

const LINHA: f32 = 22.0;
const TOPO_LISTA: f32 = 62.0;
const LINHAS_HISTORICO: usize = 6;
const BRANCO: u32 = 0xFFFFFF;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Acao {
    FocoNome,
    FocoMsg,
    Adicionar,
    Aceitar(String),
    Recusar(String),
    Mensagem(String),
    Convidar(String),
    Remover(String),
    Cancelar(String),
    Enviar,
    CopiarConvite,
    Fechar,
    Anterior,
    Proxima,
}

/// O que desenhar: retângulos, textos e onde cada botão fica.
#[derive(Default, Debug)]
pub struct Desenho {
    /// `[x, y, w, h]` e cor ARGB.
    pub retangulos: Vec<([f32; 4], u32)>,
    /// Texto, posição, cor RGB.
    pub textos: Vec<(String, [f32; 2], u32)>,
}

#[derive(Default)]
pub struct Tela {
    pub nome: String,
    pub msg: String,
    /// 0 = campo do nome, 1 = campo da mensagem.
    pub foco: Option<u8>,
    pub para: Option<String>,
    pub pagina: usize,
    confirmar: Option<String>,
    botoes: Vec<([f32; 4], Acao)>,
}

/// O que um clique ou uma tecla pede ao jogo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Efeito {
    Nada,
    Fechar,
    /// Convidar este amigo com o código da área de transferência.
    Convidar(String),
    Copiar(String),
}

fn dentro(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

/// Itens da lista: pedidos, amigos (online primeiro) e pedidos enviados.
enum Item {
    Pedido(String),
    Amigo(String, bool, String),
    Enviado(String),
}

impl Tela {
    pub fn abrir(&mut self) {
        self.confirmar = None;
        self.foco = Some(0);
    }

    fn botao(
        &mut self,
        d: &mut Desenho,
        r: [f32; 4],
        rotulo: &str,
        acao: Acao,
        mouse: [f32; 2],
        largura: impl Fn(&str) -> f32,
    ) {
        let cor = if dentro(r, mouse) {
            0xE060_6070
        } else {
            0xE030_3038
        };
        d.retangulos.push((r, cor));
        d.retangulos
            .push(([r[0], r[1] + r[3] - 1.0, r[2], 1.0], 0xFF10_1014));
        let w = largura(rotulo);
        d.textos.push((
            rotulo.into(),
            [r[0] + ((r[2] - w) / 2.0).max(2.0), r[1] + 6.0],
            BRANCO,
        ));
        self.botoes.push((r, acao));
    }

    #[allow(clippy::too_many_arguments)]
    fn campo(
        &mut self,
        d: &mut Desenho,
        r: [f32; 4],
        valor: &str,
        dica: &str,
        focado: bool,
        acao: Acao,
        largura: impl Fn(&str) -> f32,
    ) {
        d.retangulos
            .push((r, if focado { 0xFFA0_A0A0 } else { 0xFF60_6060 }));
        d.retangulos.push((
            [r[0] + 1.0, r[1] + 1.0, r[2] - 2.0, r[3] - 2.0],
            0xFF00_0000,
        ));
        // O fim do texto, se não couber.
        let mut mostrar: String = valor.to_owned();
        while !mostrar.is_empty() && largura(&mostrar) > r[2] - 10.0 {
            mostrar.remove(0);
        }
        if valor.is_empty() && !focado {
            d.textos
                .push((dica.into(), [r[0] + 4.0, r[1] + 6.0], 0x707070));
        } else {
            let cursor = if focado { "_" } else { "" };
            d.textos.push((
                format!("{mostrar}{cursor}"),
                [r[0] + 4.0, r[1] + 6.0],
                0xE0E0E0,
            ));
        }
        self.botoes.push((r, acao));
    }

    /// Monta a tela para o tamanho `[largura, altura]` e o mouse em `mouse`.
    pub fn desenhar(
        &mut self,
        a: &Amigos,
        tamanho: [f32; 2],
        mouse: [f32; 2],
        largura: impl Fn(&str) -> f32 + Copy,
    ) -> Desenho {
        let [lw, lh] = tamanho;
        let mut d = Desenho::default();
        self.botoes.clear();
        d.retangulos.push(([0.0, 0.0, lw, lh], 0xB000_0000));
        let w = (lw - 20.0).clamp(200.0, 340.0);
        let x0 = ((lw - w) / 2.0).floor();

        d.textos
            .push(("Amigos — Essencial".into(), [x0, 10.0], BRANCO));
        let (estado, cor) = match &a.conexao {
            Conexao::Desligado => (
                "Desligado: coloque o servidor em config/essencial.toml".to_owned(),
                CINZA,
            ),
            Conexao::SemNome => (
                "Coloque seu nome em config/essencial.toml".to_owned(),
                CINZA,
            ),
            Conexao::Conectando => ("Conectando...".to_owned(), CINZA),
            Conexao::Conectado => (
                format!(
                    "Conectado como {} · {}/{} online",
                    a.nome.as_deref().unwrap_or("?"),
                    a.amigos.values().filter(|x| x.online).count(),
                    a.amigos.len()
                ),
                VERDE,
            ),
            Conexao::Caiu(m) => (format!("Sem conexão ({m}); tentando de novo"), DOURADO),
            Conexao::Recusado(m) => (format!("Recusado: {m}"), VERMELHO),
        };
        d.textos
            .push((protocolo::limpar(&estado, 64), [x0, 22.0], cor));

        let nome = self.nome.clone();
        self.campo(
            &mut d,
            [x0, 34.0, w - 80.0, 20.0],
            &nome,
            "Nome do jogador",
            self.foco == Some(0),
            Acao::FocoNome,
            largura,
        );
        self.botao(
            &mut d,
            [x0 + w - 76.0, 34.0, 76.0, 20.0],
            "Adicionar",
            Acao::Adicionar,
            mouse,
            largura,
        );

        // A lista, em páginas.
        let mut itens: Vec<Item> = a.recebidos.values().cloned().map(Item::Pedido).collect();
        let mut lista: Vec<_> = a.amigos.values().collect();
        lista.sort_by_key(|x| (!x.online, x.nome.to_ascii_lowercase()));
        itens.extend(
            lista
                .into_iter()
                .map(|x| Item::Amigo(x.nome.clone(), x.online, x.status.clone())),
        );
        itens.extend(a.enviados.values().cloned().map(Item::Enviado));
        let fim = lh - 34.0 - LINHAS_HISTORICO as f32 * 10.0 - 8.0;
        let por_pagina = (((fim - TOPO_LISTA) / LINHA).floor() as usize).max(1);
        let paginas = itens.len().div_ceil(por_pagina).max(1);
        self.pagina = self.pagina.min(paginas - 1);
        if paginas > 1 {
            d.textos.push((
                format!("{}/{}", self.pagina + 1, paginas),
                [x0 + w - 80.0, 14.0],
                CINZA,
            ));
            self.botao(
                &mut d,
                [x0 + w - 44.0, 8.0, 20.0, 20.0],
                "<",
                Acao::Anterior,
                mouse,
                largura,
            );
            self.botao(
                &mut d,
                [x0 + w - 20.0, 8.0, 20.0, 20.0],
                ">",
                Acao::Proxima,
                mouse,
                largura,
            );
        }
        if itens.is_empty() {
            d.textos.push((
                "Nenhum amigo ainda. Escreva um nome e clique em Adicionar.".into(),
                [x0, TOPO_LISTA + 6.0],
                CINZA,
            ));
        }
        let direita = x0 + w;
        for (n, item) in itens
            .into_iter()
            .skip(self.pagina * por_pagina)
            .take(por_pagina)
            .enumerate()
        {
            let y = TOPO_LISTA + n as f32 * LINHA;
            match item {
                Item::Pedido(nome) => {
                    d.textos
                        .push((format!("Pedido de {nome}"), [x0, y + 6.0], DOURADO));
                    self.botao(
                        &mut d,
                        [direita - 110.0, y, 54.0, 20.0],
                        "Aceitar",
                        Acao::Aceitar(nome.clone()),
                        mouse,
                        largura,
                    );
                    self.botao(
                        &mut d,
                        [direita - 54.0, y, 54.0, 20.0],
                        "Recusar",
                        Acao::Recusar(nome),
                        mouse,
                        largura,
                    );
                }
                Item::Amigo(nome, online, status) => {
                    d.retangulos.push((
                        [x0, y + 7.0, 5.0, 5.0],
                        if online { 0xFF55_FF55 } else { 0xFF55_5555 },
                    ));
                    d.textos.push((
                        nome.clone(),
                        [x0 + 9.0, y + 6.0],
                        if online { BRANCO } else { CINZA },
                    ));
                    if online && !status.is_empty() {
                        let nx = x0 + 15.0 + largura(&nome);
                        let max = ((direita - 140.0 - nx) / 6.0).max(0.0) as usize;
                        if max > 3 {
                            d.textos
                                .push((protocolo::limpar(&status, max), [nx, y + 6.0], CINZA));
                        }
                    }
                    if online {
                        self.botao(
                            &mut d,
                            [direita - 136.0, y, 40.0, 20.0],
                            "Msg",
                            Acao::Mensagem(nome.clone()),
                            mouse,
                            largura,
                        );
                        self.botao(
                            &mut d,
                            [direita - 94.0, y, 64.0, 20.0],
                            "Convidar",
                            Acao::Convidar(nome.clone()),
                            mouse,
                            largura,
                        );
                    }
                    let rotulo = if self.confirmar.as_deref() == Some(nome.as_str()) {
                        "?"
                    } else {
                        "X"
                    };
                    self.botao(
                        &mut d,
                        [direita - 26.0, y, 26.0, 20.0],
                        rotulo,
                        Acao::Remover(nome),
                        mouse,
                        largura,
                    );
                }
                Item::Enviado(nome) => {
                    d.textos
                        .push((format!("Aguardando {nome}"), [x0, y + 6.0], CINZA));
                    self.botao(
                        &mut d,
                        [direita - 64.0, y, 64.0, 20.0],
                        "Cancelar",
                        Acao::Cancelar(nome),
                        mouse,
                        largura,
                    );
                }
            }
        }

        // O histórico e a linha de baixo.
        let topo = lh - 34.0 - LINHAS_HISTORICO as f32 * 10.0;
        let n = a.historico.len();
        for (i, l) in a
            .historico
            .iter()
            .skip(n.saturating_sub(LINHAS_HISTORICO))
            .enumerate()
        {
            d.textos.push((
                protocolo::limpar(&l.texto, 60),
                [x0, topo + i as f32 * 10.0],
                l.cor,
            ));
        }
        let y = lh - 28.0;
        let dica = match &self.para {
            Some(p) => format!("Mensagem para {p}"),
            None => "Clique em Msg num amigo".to_owned(),
        };
        let msg = self.msg.clone();
        let tem_convite = a.ultimo_convite.is_some();
        let largura_msg = if tem_convite { w - 196.0 } else { w - 120.0 };
        self.campo(
            &mut d,
            [x0, y, largura_msg, 20.0],
            &msg,
            &dica,
            self.foco == Some(1),
            Acao::FocoMsg,
            largura,
        );
        let mut bx = x0 + largura_msg + 4.0;
        if tem_convite {
            self.botao(
                &mut d,
                [bx, y, 72.0, 20.0],
                "Copiar convite",
                Acao::CopiarConvite,
                mouse,
                largura,
            );
            bx += 76.0;
        }
        self.botao(
            &mut d,
            [bx, y, 56.0, 20.0],
            "Enviar",
            Acao::Enviar,
            mouse,
            largura,
        );
        self.botao(
            &mut d,
            [bx + 60.0, y, 56.0, 20.0],
            "Fechar",
            Acao::Fechar,
            mouse,
            largura,
        );
        d
    }

    /// Um clique em `p`.
    pub fn clicar(&mut self, a: &mut Amigos, p: [f32; 2]) -> Option<Efeito> {
        let acao = self
            .botoes
            .iter()
            .rev()
            .find(|(r, _)| dentro(*r, p))
            .map(|(_, x)| x.clone())?;
        Some(self.agir(a, acao))
    }

    fn agir(&mut self, a: &mut Amigos, acao: Acao) -> Efeito {
        if !matches!(acao, Acao::Remover(_)) {
            self.confirmar = None;
        }
        match acao {
            Acao::FocoNome => self.foco = Some(0),
            Acao::FocoMsg => self.foco = Some(1),
            Acao::Adicionar => {
                let nome = std::mem::take(&mut self.nome).trim().to_owned();
                if protocolo::nome_valido(&nome) {
                    a.mandar(DoCliente::Adicionar { nome });
                } else if !nome.is_empty() {
                    a.historico(format!("Nome inválido: {nome}"), VERMELHO);
                }
            }
            Acao::Aceitar(nome) => {
                a.mandar(DoCliente::Aceitar { nome });
            }
            Acao::Recusar(nome) => {
                a.mandar(DoCliente::Recusar { nome });
            }
            Acao::Cancelar(nome) => {
                a.mandar(DoCliente::Remover { nome });
            }
            Acao::Remover(nome) => {
                if self.confirmar.as_deref() == Some(nome.as_str()) {
                    self.confirmar = None;
                    a.mandar(DoCliente::Remover { nome });
                } else {
                    a.historico(format!("Clique em ? de novo para remover {nome}."), CINZA);
                    self.confirmar = Some(nome);
                }
            }
            Acao::Mensagem(nome) => {
                self.para = Some(nome);
                self.foco = Some(1);
            }
            Acao::Convidar(nome) => return Efeito::Convidar(nome),
            Acao::Enviar => {
                if let Some(para) = self.para.clone() {
                    let texto = std::mem::take(&mut self.msg);
                    a.mensagem(&para, &texto);
                }
            }
            Acao::CopiarConvite => {
                if let Some((de, codigo)) = a.ultimo_convite.clone() {
                    a.historico(
                        format!("Convite de {de} copiado: cole em Multijogador > Conexão direta."),
                        VERDE,
                    );
                    return Efeito::Copiar(codigo);
                }
            }
            Acao::Fechar => return Efeito::Fechar,
            Acao::Anterior => self.pagina = self.pagina.saturating_sub(1),
            Acao::Proxima => self.pagina += 1,
        }
        Efeito::Nada
    }

    /// Um caractere digitado. `true` se a tela usou.
    pub fn digitar(&mut self, c: char) -> bool {
        let (campo, max) = match self.foco {
            Some(0) => (&mut self.nome, 16),
            Some(1) if self.para.is_some() => (&mut self.msg, protocolo::TEXTO_MAX),
            _ => return false,
        };
        if !c.is_control() && campo.chars().count() < max {
            campo.push(c);
        }
        true
    }

    /// Cola `texto` no campo com foco.
    pub fn colar(&mut self, texto: &str) {
        for c in texto.chars().filter(|c| !c.is_control()) {
            self.digitar(c);
        }
    }

    /// Uma tecla (nome do Minecraft, `key.keyboard.backspace`). `None` se a
    /// tela não usou; senão o efeito.
    pub fn tecla(&mut self, a: &mut Amigos, tecla: &str) -> Option<Efeito> {
        match tecla {
            "key.keyboard.backspace" => {
                match self.foco {
                    Some(0) => drop(self.nome.pop()),
                    Some(1) => drop(self.msg.pop()),
                    _ => return None,
                }
                Some(Efeito::Nada)
            }
            "key.keyboard.enter" | "key.keyboard.keypad.enter" => match self.foco {
                Some(0) => Some(self.agir(a, Acao::Adicionar)),
                Some(1) => Some(self.agir(a, Acao::Enviar)),
                _ => None,
            },
            "key.keyboard.tab" => {
                self.foco = match self.foco {
                    Some(0) if self.para.is_some() => Some(1),
                    _ => Some(0),
                };
                Some(Efeito::Nada)
            }
            // Com um campo em foco, as letras são do campo (não abrem o inventário etc.).
            t if self.foco.is_some()
                && t.starts_with("key.keyboard.")
                && t != "key.keyboard.escape" =>
            {
                Some(Efeito::Nada)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::amigos::Amigo;

    fn largura(t: &str) -> f32 {
        t.chars().count() as f32 * 6.0
    }

    fn com_amiga() -> Amigos {
        let mut a = Amigos {
            conexao: Conexao::Conectado,
            ..Amigos::default()
        };
        a.amigos.insert(
            "ana".into(),
            Amigo {
                nome: "Ana".into(),
                online: true,
                status: "No menu".into(),
            },
        );
        a
    }

    /// O centro do botão com este rótulo no desenho.
    fn centro(d: &Desenho, rotulo: &str) -> [f32; 2] {
        let (_, at, _) = d.textos.iter().find(|(t, _, _)| t == rotulo).expect(rotulo);
        [at[0] + 1.0, at[1] + 1.0]
    }

    #[test]
    fn clicar_em_convidar_pede_o_codigo() {
        let mut t = Tela::default();
        let mut a = com_amiga();
        let d = t.desenhar(&a, [400.0, 300.0], [0.0, 0.0], largura);
        assert_eq!(
            t.clicar(&mut a, centro(&d, "Convidar")),
            Some(Efeito::Convidar("Ana".into()))
        );
        assert_eq!(t.clicar(&mut a, centro(&d, "Fechar")), Some(Efeito::Fechar));
        assert_eq!(t.clicar(&mut a, [-5.0, -5.0]), None);
    }

    #[test]
    fn remover_pede_confirmacao() {
        let mut t = Tela::default();
        let mut a = com_amiga();
        let d = t.desenhar(&a, [400.0, 300.0], [0.0, 0.0], largura);
        let x = centro(&d, "X");
        t.clicar(&mut a, x);
        let d = t.desenhar(&a, [400.0, 300.0], [0.0, 0.0], largura);
        assert!(d.textos.iter().any(|(s, _, _)| s == "?"));
    }

    #[test]
    fn digitar_no_campo_de_nome() {
        let mut t = Tela::default();
        let mut a = com_amiga();
        t.abrir();
        for c in "Bia".chars() {
            assert!(t.digitar(c));
        }
        assert_eq!(
            t.tecla(&mut a, "key.keyboard.backspace"),
            Some(Efeito::Nada)
        );
        assert_eq!(t.nome, "Bi");
        assert_eq!(t.tecla(&mut a, "key.keyboard.e"), Some(Efeito::Nada));
        assert_eq!(t.tecla(&mut a, "key.keyboard.escape"), None);
        // Sem amigo escolhido, o campo de mensagem não aceita texto.
        t.foco = Some(1);
        assert!(!t.digitar('x'));
    }
}
