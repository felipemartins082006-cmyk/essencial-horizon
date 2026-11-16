//! Zoom: segure a tecla (C por padrão) para aproximar; a roda do mouse muda
//! o quanto. O jogo suaviza a mudança do campo de visão sozinho.

use crate::ID;
use mod_api::{ClientHost, ComputeFovModifierEvent, Flow, MouseScrollingEvent};
use std::sync::Mutex;

/// Menor e maior aproximação.
pub const MIN: f32 = 1.5;
pub const MAX: f32 = 50.0;
/// Quanto cada passo da roda multiplica ou divide a aproximação.
const PASSO: f32 = 1.2;

#[derive(Default)]
pub struct Zoom {
    estado: Mutex<Estado>,
}

#[derive(Default)]
struct Estado {
    ativo: bool,
    /// Quantas vezes mais perto (4 = um quarto do campo de visão).
    nivel: f32,
}

/// O novo nível depois de `passos` da roda (positivo aproxima).
pub fn rolar_nivel(nivel: f32, passos: i32) -> f32 {
    (nivel * PASSO.powi(passos)).clamp(MIN, MAX)
}

impl Zoom {
    /// Uma vez por tick: lê a tecla.
    pub fn tick(&self, host: &mut dyn ClientHost) {
        let alternar = host.setting_bool("zoom_alternar").unwrap_or(false);
        let jogando = host.playing();
        let mut e = self.estado.lock().unwrap();
        let antes = e.ativo;
        if alternar {
            while host.consume_click(ID, "zoom") {
                e.ativo = !e.ativo;
            }
        } else {
            // Os cliques não são usados neste modo; descartá-los evita que se acumulem.
            while host.consume_click(ID, "zoom") {}
            e.ativo = jogando && host.is_down(ID, "zoom");
        }
        if e.ativo && !antes {
            let padrao = host.setting_f64("zoom_nivel").unwrap_or(4.0) as f32;
            e.nivel = padrao.clamp(MIN, MAX);
        }
    }

    /// O campo de visão: dividido pelo nível enquanto o zoom está ativo.
    pub fn fov(&self, event: &mut ComputeFovModifierEvent<'_>) {
        let e = self.estado.lock().unwrap();
        if e.ativo && e.nivel > 0.0 {
            event.modifier = (event.modifier / e.nivel).clamp(0.01, 10.0);
        }
    }

    /// A roda do mouse: com o zoom ativo, muda o nível e não troca o item da mão.
    pub fn rolar(&self, host: &mut dyn ClientHost, event: &mut MouseScrollingEvent) -> Flow {
        let mut e = self.estado.lock().unwrap();
        if !e.ativo || !host.setting_bool("zoom_rolagem").unwrap_or(true) {
            return Flow::Continue;
        }
        let mut passos = event.accumulated[1];
        if passos == 0 && event.scroll[1] != 0.0 {
            passos = event.scroll[1].signum() as i32;
        }
        e.nivel = rolar_nivel(e.nivel, passos);
        Flow::Cancel
    }

    /// O nível atual, se o zoom está ativo (para o HUD).
    pub fn nivel(&self) -> Option<f32> {
        let e = self.estado.lock().unwrap();
        e.ativo.then_some(e.nivel)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn roda_aproxima_e_afasta_com_limites() {
        assert!(rolar_nivel(4.0, 1) > 4.0);
        assert!(rolar_nivel(4.0, -1) < 4.0);
        assert_eq!(rolar_nivel(4.0, 100), MAX);
        assert_eq!(rolar_nivel(4.0, -100), MIN);
        assert_eq!(rolar_nivel(4.0, 0), 4.0);
    }
}
