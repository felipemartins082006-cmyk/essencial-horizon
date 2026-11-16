//! As opções do mod: os botões em Opções > Mods > Essencial (guardados com
//! `Host::setting`) e o arquivo `config/essencial.toml` (servidor de amigos e nome).

use crate::protocolo;

/// Níveis de zoom inicial que o botão percorre.
pub const NIVEIS: [f32; 6] = [2.0, 3.0, 4.0, 6.0, 8.0, 10.0];

/// Cantos do HUD, na ordem do botão.
pub const CANTOS: [&str; 4] = [
    "superior esquerdo",
    "superior direito",
    "inferior esquerdo",
    "inferior direito",
];

/// O `config/essencial.toml` criado na primeira vez.
pub const CONFIG_PADRAO: &str = "\
# Essencial: servidor de amigos.
# Coloque aqui o endereço de quem roda o essencial-servidor, por exemplo
# servidor = \"192.168.0.10:25590\" ou servidor = \"amigos.meusite.com:25590\".
# Vazio: a lista de amigos fica desligada (zoom e HUD continuam funcionando).
servidor = \"\"

# Seu nome de jogador para os amigos. Vazio: o nome da conta conectada no launcher.
nome = \"\"
";

/// As opções dos botões.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opcoes {
    /// Índice em [`NIVEIS`].
    pub zoom_nivel: usize,
    /// Apertar a tecla liga e desliga (senão, segurar).
    pub zoom_alternar: bool,
    pub zoom_rolagem: bool,
    pub hud: bool,
    /// Índice em [`CANTOS`].
    pub hud_canto: usize,
    pub amigos: bool,
    pub mostrar_servidor: bool,
    pub avisos: bool,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self {
            zoom_nivel: 2,
            zoom_alternar: false,
            zoom_rolagem: true,
            hud: true,
            hud_canto: 0,
            amigos: true,
            mostrar_servidor: true,
            avisos: true,
        }
    }
}

/// As chaves com que cada opção é guardada.
const CHAVES: [&str; 8] = [
    "zoom_nivel",
    "zoom_alternar",
    "zoom_rolagem",
    "hud",
    "hud_canto",
    "amigos",
    "amigos_mostrar_servidor",
    "amigos_avisos",
];

fn sim(b: bool) -> &'static str {
    if b { "SIM" } else { "NÃO" }
}

impl Opcoes {
    /// Lê as opções com `ler(chave)`; o que faltar fica no padrão.
    pub fn ler(ler: impl Fn(&str) -> Option<String>) -> Self {
        let p = Self::default();
        let b = |k: &str, d: bool| ler(k).map_or(d, |v| v.trim() != "false");
        let n = |k: &str, d: usize, max: usize| {
            ler(k)
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|v| *v < max)
                .unwrap_or(d)
        };
        Self {
            zoom_nivel: n(CHAVES[0], p.zoom_nivel, NIVEIS.len()),
            zoom_alternar: b(CHAVES[1], p.zoom_alternar),
            zoom_rolagem: b(CHAVES[2], p.zoom_rolagem),
            hud: b(CHAVES[3], p.hud),
            hud_canto: n(CHAVES[4], p.hud_canto, CANTOS.len()),
            amigos: b(CHAVES[5], p.amigos),
            mostrar_servidor: b(CHAVES[6], p.mostrar_servidor),
            avisos: b(CHAVES[7], p.avisos),
        }
    }

    /// Os rótulos dos botões da página de opções, na ordem dos cliques.
    pub fn botoes(&self) -> Vec<String> {
        vec![
            format!("Zoom inicial: {}x", NIVEIS[self.zoom_nivel]),
            format!(
                "Tecla do zoom: {}",
                if self.zoom_alternar {
                    "liga e desliga"
                } else {
                    "segurar"
                }
            ),
            format!("Roda do mouse no zoom: {}", sim(self.zoom_rolagem)),
            format!("HUD de informações: {}", sim(self.hud)),
            format!("Canto do HUD: {}", CANTOS[self.hud_canto]),
            format!("Amigos: {}", sim(self.amigos)),
            format!(
                "Mostrar meu servidor aos amigos: {}",
                sim(self.mostrar_servidor)
            ),
            format!("Avisos de amigos na tela: {}", sim(self.avisos)),
        ]
    }

    /// O clique no botão `i`: muda a opção e devolve a chave e o valor a guardar.
    pub fn clicar(&mut self, i: usize) -> Option<(&'static str, String)> {
        let valor = match i {
            0 => {
                self.zoom_nivel = (self.zoom_nivel + 1) % NIVEIS.len();
                self.zoom_nivel.to_string()
            }
            1 => {
                self.zoom_alternar = !self.zoom_alternar;
                self.zoom_alternar.to_string()
            }
            2 => {
                self.zoom_rolagem = !self.zoom_rolagem;
                self.zoom_rolagem.to_string()
            }
            3 => {
                self.hud = !self.hud;
                self.hud.to_string()
            }
            4 => {
                self.hud_canto = (self.hud_canto + 1) % CANTOS.len();
                self.hud_canto.to_string()
            }
            5 => {
                self.amigos = !self.amigos;
                self.amigos.to_string()
            }
            6 => {
                self.mostrar_servidor = !self.mostrar_servidor;
                self.mostrar_servidor.to_string()
            }
            7 => {
                self.avisos = !self.avisos;
                self.avisos.to_string()
            }
            _ => return None,
        };
        Some((CHAVES[i], valor))
    }
}

/// O que o `essencial.toml` diz.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// `host:porta` do servidor de amigos; `None` desliga os amigos.
    pub servidor: Option<String>,
    /// O nome a usar; `None` usa o da conta.
    pub nome: Option<String>,
}

/// O valor de `chave = "valor"` (ou sem aspas) numa linha, se for essa chave.
fn valor<'a>(linha: &'a str, chave: &str) -> Option<&'a str> {
    let resto = linha.trim().strip_prefix(chave)?;
    let valor = resto.trim_start().strip_prefix('=')?.trim();
    let valor = match valor.strip_prefix('"') {
        Some(v) => v.split_once('"').map_or(v, |(v, _)| v),
        None => valor.split('#').next().unwrap_or(""),
    };
    Some(valor.trim())
}

impl Config {
    pub fn ler(texto: &str) -> Self {
        let mut c = Self::default();
        for linha in texto.lines() {
            if let Some(v) = valor(linha, "servidor").filter(|v| !v.is_empty()) {
                let tem_porta = v
                    .rsplit_once(':')
                    .is_some_and(|(_, p)| p.parse::<u16>().is_ok() && !v.ends_with(']'));
                c.servidor = Some(if tem_porta {
                    v.to_owned()
                } else {
                    format!("{v}:{}", protocolo::PORTA_PADRAO)
                });
            } else if let Some(v) = valor(linha, "nome").filter(|v| protocolo::nome_valido(v)) {
                c.nome = Some(v.to_owned());
            }
        }
        c
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn config_le_servidor_e_nome() {
        assert_eq!(Config::ler(CONFIG_PADRAO), Config::default());
        let c = Config::ler("servidor = \"amigos.exemplo.com\"\nnome = \"Felipe_08\"");
        assert_eq!(c.servidor.as_deref(), Some("amigos.exemplo.com:25590"));
        assert_eq!(c.nome.as_deref(), Some("Felipe_08"));
        let c = Config::ler("servidor = 10.0.0.1:4000 # casa\nnome = \"com espaço\"");
        assert_eq!(c.servidor.as_deref(), Some("10.0.0.1:4000"));
        assert_eq!(c.nome, None);
        assert_eq!(Config::ler("servidores = \"x\"").servidor, None);
    }

    #[test]
    fn opcoes_guardadas_voltam_iguais() {
        let mut o = Opcoes::default();
        let mut guardado = HashMap::new();
        for i in 0..CHAVES.len() {
            let (k, v) = o.clicar(i).unwrap();
            guardado.insert(k.to_owned(), v);
        }
        assert_ne!(o, Opcoes::default());
        assert_eq!(Opcoes::ler(|k| guardado.get(k).cloned()), o);
        assert_eq!(o.clicar(99), None);
        assert_eq!(o.botoes().len(), CHAVES.len());
    }

    #[test]
    fn opcoes_invalidas_ficam_no_padrao() {
        let o = Opcoes::ler(|k| (k == "zoom_nivel").then(|| "99".to_owned()));
        assert_eq!(o, Opcoes::default());
    }
}
