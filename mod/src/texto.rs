//! Textos com cor e clique para o chat.

use mod_api::text::{TextClick, TextStyle};
use mod_api::{Text, TextHover};

/// `texto` na cor `cor` (nome do Minecraft: `gold`, `aqua`, `gray`...).
pub fn cor(texto: impl Into<String>, cor: &str) -> Text {
    let mut estilo = TextStyle::default();
    estilo.color = Some(cor.to_owned());
    Text::literal(texto).with_style(estilo)
}

/// Um "botão" no chat: sublinhado, com dica ao passar o mouse e uma ação ao clicar.
pub fn botao(texto: impl Into<String>, cor_texto: &str, dica: &str, clique: TextClick) -> Text {
    let mut estilo = TextStyle::default();
    estilo.color = Some(cor_texto.to_owned());
    estilo.underlined = Some(true);
    estilo.click = Some(clique);
    estilo.hover = Some(TextHover::Text(Box::new(Text::literal(dica))));
    Text::literal(texto).with_style(estilo)
}

/// A linha de chat do Essencial: `[Essencial] ` em dourado e as partes depois.
pub fn linha(partes: impl IntoIterator<Item = Text>) -> Text {
    let mut t = cor("[Essencial] ", "gold");
    for parte in partes {
        t = t.append(parte);
    }
    t
}

/// Uma linha simples, em cinza claro.
pub fn aviso(texto: impl Into<String>) -> Text {
    linha([cor(texto, "gray")])
}

/// Uma linha de erro, em vermelho.
pub fn erro(texto: impl Into<String>) -> Text {
    linha([cor(texto, "red")])
}
