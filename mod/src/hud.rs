//! O HUD: FPS, coordenadas, direção, ping e zoom num canto, e o aviso de
//! mensagens e convites de amigos no outro canto de cima.

use crate::{amigos::Amigos, zoom::Zoom};
use mod_api::{ClientHost, Corner, RenderGuiPostEvent, Text};

const FUNDO: u32 = 0x9000_0000;
const BRANCO: u32 = 0xFFFF_FFFF;
const AMARELO: u32 = 0xFFFF_DD55;
const ALTURA_LINHA: f32 = 10.0;
const MARGEM: f32 = 3.0;

/// O ponto cardeal para onde a câmera olha. No Minecraft, yaw 0 olha para o
/// sul (+Z), 90 para o oeste (-X), 180 para o norte (-Z) e 270 para o leste (+X).
pub fn direcao(yaw: f32) -> &'static str {
    let graus = yaw.rem_euclid(360.0);
    const NOMES: [&str; 8] = [
        "Sul (+Z)",
        "Sudoeste",
        "Oeste (-X)",
        "Noroeste",
        "Norte (-Z)",
        "Nordeste",
        "Leste (+X)",
        "Sudeste",
    ];
    NOMES[(((graus + 22.5) / 45.0) as usize) % 8]
}

fn ligado(host: &dyn ClientHost, chave: &str) -> bool {
    host.setting_bool(chave).unwrap_or(true)
}

/// As linhas do painel de informações.
fn linhas(host: &dyn ClientHost, zoom: &Zoom) -> Vec<(String, u32)> {
    let mut l = Vec::new();
    let sessao = host.client_session();
    if ligado(host, "hud_fps")
        && let Some(stats) = sessao.client_frame_stats()
    {
        l.push((format!("FPS: {:.0}", stats.fps), BRANCO));
    }
    if let Some(camera) = host.camera() {
        if ligado(host, "hud_coordenadas") {
            let [x, y, z] = camera.position;
            l.push((format!("XYZ: {x:.1} / {y:.1} / {z:.1}"), BRANCO));
        }
        if ligado(host, "hud_direcao") {
            l.push((format!("Direção: {}", direcao(camera.yaw)), BRANCO));
        }
    }
    if ligado(host, "hud_ping") {
        let local = sessao.client_server().is_some_and(|s| s.integrated_local);
        if !local && let Some(ms) = sessao.client_latency() {
            l.push((format!("Ping: {ms} ms"), BRANCO));
        }
    }
    if let Some(nivel) = zoom.nivel() {
        l.push((format!("Zoom: {nivel:.1}x"), AMARELO));
    }
    l
}

fn largura(host: &dyn ClientHost, texto: &str) -> f32 {
    host.text_width(&Text::literal(texto))
        .map_or(texto.chars().count() as f32 * 6.0, |w| w as f32)
}

/// Desenha uma caixa com as linhas no canto, empilhando com o que já estiver lá.
fn caixa(
    host: &dyn ClientHost,
    event: &mut RenderGuiPostEvent<'_>,
    canto: Corner,
    linhas: &[(String, u32)],
) {
    if linhas.is_empty() {
        return;
    }
    let w = linhas
        .iter()
        .map(|(t, _)| largura(host, t))
        .fold(0.0, f32::max)
        + MARGEM * 2.0;
    let h = linhas.len() as f32 * ALTURA_LINHA + MARGEM * 2.0 - 1.0;
    let (x, y) = event.corners.place(canto, w, h);
    event.draw.fill([x, y, w, h], FUNDO);
    for (i, (texto, cor)) in linhas.iter().enumerate() {
        let at = [x + MARGEM, y + MARGEM + i as f32 * ALTURA_LINHA];
        event.draw.text(texto, at, *cor, true);
    }
}

pub fn desenhar(
    host: &dyn ClientHost,
    event: &mut RenderGuiPostEvent<'_>,
    zoom: &Zoom,
    amigos: &Amigos,
) {
    let canto = match host.setting_choice("hud_canto").unwrap_or(0) {
        1 => Corner::TopRight,
        2 => Corner::BottomLeft,
        3 => Corner::BottomRight,
        _ => Corner::TopLeft,
    };
    if host.setting_bool("hud").unwrap_or(true) {
        let l = linhas(host, zoom);
        caixa(host, event, canto, &l);
    }
    if let Some(aviso) = amigos.aviso() {
        let outro = if canto == Corner::TopRight {
            Corner::TopLeft
        } else {
            Corner::TopRight
        };
        caixa(host, event, outro, &[(aviso, AMARELO)]);
    }
}

#[cfg(test)]
mod testes {
    use super::direcao;

    #[test]
    fn pontos_cardeais() {
        assert_eq!(direcao(0.0), "Sul (+Z)");
        assert_eq!(direcao(90.0), "Oeste (-X)");
        assert_eq!(direcao(180.0), "Norte (-Z)");
        assert_eq!(direcao(-90.0), "Leste (+X)");
        assert_eq!(direcao(270.0), "Leste (+X)");
        assert_eq!(direcao(359.0), "Sul (+Z)");
        assert_eq!(direcao(-180.0), "Norte (-Z)");
        assert_eq!(direcao(44.0), "Sudoeste");
    }
}
