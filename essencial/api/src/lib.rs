//! Essencial para Horizon: zoom, HUD de informações e amigos (lista, mensagens
//! e convites). Escrito do zero; inspirado no que o mod Essential faz, sem usar
//! o código dele. O launcher compila esta biblioteca junto com o jogo e chama
//! [`hooks`].

mod amigos;
mod opcoes;
mod protocolo;
mod rede;
mod tela;

use amigos::{Amigos, Aviso, Conexao, Pedido};
use horizon_client::clipboard::{Clipboard, SystemClipboard};
use horizon_client::extras::Corner;
use horizon_client::mod_api::{
    ClientTickPreEvent, CommandEvent, CommandsEvent, Flow, Hooks, Host, HudEvent, KeyEvent,
    ModsEvent, NativeMod, NativeZoom, ScreenCharTypedPreEvent, ScreenKeyPressedPreEvent,
    ScreenMouseButtonPressedPreEvent, ScreenRenderPostEvent, SettingsClickEvent, SettingsEvent,
    Text, WheelEvent, ZoomEvent,
};
use opcoes::{Config, Opcoes};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// O id do mod, igual ao de `horizon-mixin.toml`.
pub const ID: &str = "essencial";
const VERSAO: &str = "0.1.0";
const TECLA_ZOOM: &str = "key.keyboard.c";
const TECLA_AMIGOS: &str = "key.keyboard.o";

/// Menor e maior aproximação do zoom.
const ZOOM_MIN: f32 = 1.5;
const ZOOM_MAX: f32 = 50.0;

/// O novo nível depois de `passos` da roda (positivo aproxima).
fn rolar_nivel(nivel: f32, passos: f32) -> f32 {
    (nivel * 1.2f32.powf(passos)).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// O ponto cardeal: yaw 0 olha para o sul (+Z), 90 para o oeste (-X).
fn direcao(yaw: f32) -> &'static str {
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
    NOMES[((yaw.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8]
}

/// Um segredo novo para este computador: 32 dígitos hexadecimais, das chaves
/// aleatórias que a biblioteca padrão pede ao sistema para cada `RandomState`.
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

#[derive(Default)]
struct Zoom {
    ativo: bool,
    nivel: f32,
}

#[derive(Default)]
struct Estado {
    opcoes: Option<Opcoes>,
    config: Config,
    /// Quando o `essencial.toml` foi lido e sua data de modificação.
    config_lida: Option<(Instant, Option<std::time::SystemTime>)>,
    segredo: Option<String>,
    zoom: Zoom,
    amigos: Amigos,
    tela: tela::Tela,
    abrir_tela: bool,
    fechar_tela: bool,
    /// O aviso mostrado no canto do HUD e até quando.
    aviso: Option<(String, Instant)>,
    /// Quando a barra de ação pode mostrar o próximo aviso.
    proxima_barra: Option<Instant>,
    /// Quadros desenhados no último segundo, para o FPS.
    quadros: u32,
    fps: u32,
    desde: Option<Instant>,
    area: Option<SystemClipboard>,
}

impl Estado {
    fn opcoes(&mut self, host: &dyn Host) -> &mut Opcoes {
        self.opcoes
            .get_or_insert_with(|| Opcoes::ler(|k| host.setting(ID, k)))
    }

    fn area(&mut self) -> &mut SystemClipboard {
        self.area.get_or_insert_with(SystemClipboard::default)
    }

    /// Lê (e cria) o `config/essencial.toml` a cada 2 segundos, se mudou.
    fn reler_config(&mut self, host: &dyn Host) {
        if self
            .config_lida
            .is_some_and(|(quando, _)| quando.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        let Ok(pasta) = host.config_dir() else {
            return;
        };
        let arquivo = pasta.join("essencial.toml");
        if !arquivo.exists() {
            let _ = std::fs::write(&arquivo, opcoes::CONFIG_PADRAO);
        }
        let data = std::fs::metadata(&arquivo).and_then(|m| m.modified()).ok();
        let mudou = self.config_lida.is_none_or(|(_, antes)| antes != data);
        self.config_lida = Some((Instant::now(), data));
        if !mudou {
            return;
        }
        let config = Config::ler(&std::fs::read_to_string(&arquivo).unwrap_or_default());
        if config != self.config {
            self.config = config;
            self.amigos.desligar();
        }
    }

    /// O segredo deste computador, criado na primeira vez.
    fn segredo(&mut self, host: &dyn Host) -> Option<String> {
        if self.segredo.is_none() {
            let pasta = host.data_dir(ID);
            let arquivo = pasta.join("segredo.txt");
            let lido = std::fs::read_to_string(&arquivo)
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| s.len() >= 16);
            self.segredo = lido.or_else(|| {
                let novo = novo_segredo();
                let _ = std::fs::create_dir_all(&pasta);
                std::fs::write(&arquivo, &novo).ok().map(|()| novo)
            });
        }
        self.segredo.clone()
    }

    /// O que mostrar aos amigos sobre onde estou.
    fn status(&mut self, host: &dyn Host) -> String {
        let mostrar = self.opcoes(host).mostrar_servidor;
        match host.client_state() {
            Some(s) if s.logged_in => match s.server_address {
                Some(endereco) if mostrar && !endereco.starts_with("friend:") => {
                    protocolo::limpar(&format!("Jogando em {endereco}"), protocolo::STATUS_MAX)
                }
                Some(_) => "Jogando online".into(),
                None => "Jogando no próprio mundo".into(),
            },
            _ => "No menu".into(),
        }
    }
}

#[derive(Default)]
struct Essencial {
    estado: Mutex<Estado>,
}

/// O jogo chama esta função para ter os ganchos do mod.
pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Essencial::default())
}

impl Essencial {
    fn estado(&self) -> std::sync::MutexGuard<'_, Estado> {
        self.estado.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Hooks for Essencial {
    fn mods(&self, _host: &mut dyn Host, event: &mut ModsEvent<'_>) {
        event.mods.push(NativeMod {
            id: ID.into(),
            title: format!("Essencial {VERSAO}"),
            description: "Zoom (C), HUD de informações e amigos com mensagens e convites (O). /e ajuda lista os comandos.".into(),
            keys: vec![
                ["zoom".into(), "Zoom".into(), TECLA_ZOOM.into()],
                ["amigos".into(), "Abrir amigos".into(), TECLA_AMIGOS.into()],
            ],
        });
    }

    fn settings(&self, host: &mut dyn Host, event: &mut SettingsEvent<'_>) {
        if event.mod_id == ID {
            let mut e = self.estado();
            event.buttons.extend(e.opcoes(host).botoes());
        }
    }

    fn settings_click(&self, host: &mut dyn Host, event: &mut SettingsClickEvent<'_>) {
        if event.mod_id != ID {
            return;
        }
        let mut e = self.estado();
        if let Some((chave, valor)) = e.opcoes(host).clicar(event.button) {
            host.set_setting(ID, chave, &valor);
        }
    }

    fn client_tick_pre(&self, host: &mut dyn Host, _event: &mut ClientTickPreEvent) {
        if !host.on(ID) {
            return;
        }
        let mut e = self.estado();
        let e = &mut *e;
        let opcoes = e.opcoes(host).clone();
        e.reler_config(host);

        // A conexão com o servidor de amigos.
        if !opcoes.amigos || e.config.servidor.is_none() {
            if e.amigos.rede.is_some() || e.amigos.conexao != Conexao::Desligado {
                e.amigos.desligar();
            }
        } else if e.amigos.rede.is_none() && !matches!(e.amigos.conexao, Conexao::Recusado(_)) {
            let nome = e
                .config
                .nome
                .clone()
                .or_else(|| horizon_client::auth::current().map(|s| s.name.clone()))
                .filter(|n| protocolo::nome_valido(n));
            match (nome, e.segredo(host), e.config.servidor.clone()) {
                (Some(nome), Some(segredo), Some(servidor)) => {
                    e.amigos.rede = Some(rede::Rede::iniciar(servidor, nome, segredo));
                    e.amigos.conexao = Conexao::Conectando;
                }
                _ => e.amigos.conexao = Conexao::SemNome,
            }
        }
        e.amigos.processar(opcoes.avisos);
        if e.amigos.conectado() {
            let status = e.status(host);
            if e.amigos.status_mandado.as_ref() != Some(&status) {
                e.amigos.status_mandado = Some(status.clone());
                if let Some(rede) = &e.amigos.rede {
                    rede.mandar(protocolo::DoCliente::Status { texto: status });
                }
            }
        }

        // Os avisos: um por vez na barra de ação; notificações no canto.
        if host.player().is_some() {
            let pode_barra = e.proxima_barra.is_none_or(|t| Instant::now() >= t);
            let primeiro_e_barra = matches!(e.amigos.avisos.front(), Some(Aviso::Barra(_)));
            if !primeiro_e_barra || pode_barra {
                match e.amigos.avisos.pop_front() {
                    Some(Aviso::Barra(t)) => {
                        host.client_action_bar(Text::literal(format!("[Essencial] {t}")), false);
                        e.proxima_barra = Some(Instant::now() + Duration::from_secs(3));
                    }
                    Some(Aviso::Notificacao { titulo, texto }) => {
                        let mostrou = host.add_toast(
                            "essencial:amigos",
                            "minecraft:player_head",
                            Text::literal(titulo.clone()),
                            Text::literal(texto.clone()),
                            false,
                        );
                        if !mostrou {
                            e.aviso = Some((format!("{titulo}: {texto}"), Instant::now()));
                        }
                    }
                    None => {}
                }
            }
        }

        // Abrir e fechar a tela de amigos fora dos ganchos de tela.
        if std::mem::take(&mut e.fechar_tela) {
            host.mod_screen_close(ID);
        }
        if e.abrir_tela && host.mod_screen().is_none() && host.playing() {
            e.abrir_tela = false;
            if host.mod_screen_open(ID) {
                e.tela.abrir();
            }
        }
    }

    fn key(&self, host: &mut dyn Host, event: &mut KeyEvent<'_>) {
        if !host.on(ID) {
            return;
        }
        let mut e = self.estado();
        let opcoes = e.opcoes(host).clone();
        let jogando = host.playing();
        if event.key == host.mod_key(ID, "zoom", TECLA_ZOOM) {
            let antes = e.zoom.ativo;
            if opcoes.zoom_alternar {
                if event.pressed && jogando {
                    e.zoom.ativo = !e.zoom.ativo;
                }
            } else {
                e.zoom.ativo = event.pressed && jogando;
            }
            if e.zoom.ativo && !antes {
                e.zoom.nivel = opcoes::NIVEIS[opcoes.zoom_nivel];
            }
        } else if event.key == host.mod_key(ID, "amigos", TECLA_AMIGOS) && event.pressed && jogando
        {
            e.abrir_tela = true;
            if host.mod_screen_open(ID) {
                e.abrir_tela = false;
                e.tela.abrir();
            }
        }
    }

    fn wheel(&self, host: &mut dyn Host, event: &mut WheelEvent) -> Flow {
        let mut e = self.estado();
        let rolagem = e.opcoes(host).zoom_rolagem;
        if !e.zoom.ativo || !rolagem || !host.on(ID) || event.steps == 0.0 {
            return Flow::Continue;
        }
        e.zoom.nivel = rolar_nivel(e.zoom.nivel, event.steps.signum());
        Flow::Cancel
    }

    fn zoom(&self, host: &dyn Host, _event: &mut ZoomEvent) -> Option<NativeZoom> {
        let e = self.estado();
        (e.zoom.ativo && host.on(ID) && host.playing()).then(|| NativeZoom {
            factor: e.zoom.nivel,
            smooth: true,
            turn: 1.0 / e.zoom.nivel,
        })
    }

    fn hud(&self, host: &mut dyn Host, event: &mut HudEvent<'_>) {
        if !host.on(ID) {
            return;
        }
        let mut e = self.estado();
        // O FPS: quadros desenhados no último segundo.
        e.quadros += 1;
        let desde = *e.desde.get_or_insert_with(Instant::now);
        if desde.elapsed() >= Duration::from_secs(1) {
            e.fps = e.quadros;
            e.quadros = 0;
            e.desde = Some(Instant::now());
        }
        let opcoes = e.opcoes(host).clone();
        let canto = [
            Corner::TopLeft,
            Corner::TopRight,
            Corner::BottomLeft,
            Corner::BottomRight,
        ][opcoes.hud_canto];
        let mut linhas: Vec<(String, u32)> = Vec::new();
        if opcoes.hud {
            linhas.push((format!("FPS: {}", e.fps), 0xFFFFFF));
            if let Some(p) = host.player() {
                linhas.push((
                    format!("XYZ: {:.1} / {:.1} / {:.1}", p.x, p.y, p.z),
                    0xFFFFFF,
                ));
                linhas.push((format!("Direção: {}", direcao(p.yaw)), 0xFFFFFF));
            }
        }
        if e.zoom.ativo {
            linhas.push((format!("Zoom: {:.1}x", e.zoom.nivel), 0xFFDD55));
        }
        caixa(host, event, canto, &linhas);
        let aviso = match &e.aviso {
            Some((t, quando)) if quando.elapsed() < Duration::from_secs(6) => Some(t.clone()),
            _ => None,
        };
        if let Some(aviso) = aviso {
            let outro = if canto == Corner::TopRight {
                Corner::TopLeft
            } else {
                Corner::TopRight
            };
            caixa(
                host,
                event,
                outro,
                &[(protocolo::limpar(&aviso, 60), 0xFFDD55)],
            );
        }
    }

    fn commands(&self, _host: &mut dyn Host, event: &mut CommandsEvent<'_>) {
        event.commands.push("amigos");
        event.commands.push("e");
    }

    fn command(&self, host: &mut dyn Host, event: &mut CommandEvent<'_>) -> Flow {
        if !host.on(ID) {
            return Flow::Continue;
        }
        let (nome, args) = event.command.split_once(' ').unwrap_or((event.command, ""));
        let mut e = self.estado();
        let pedido = match nome {
            "amigos" => Pedido::AbrirTela,
            "e" => e.amigos.comando(args),
            _ => return Flow::Continue,
        };
        match pedido {
            Pedido::Nada => {}
            Pedido::AbrirTela => e.abrir_tela = true,
            Pedido::Convidar(alvo) => {
                let area = e.area().get();
                e.amigos.convidar(&alvo, &area);
            }
            Pedido::Copiar(texto) => e.area().set(&texto),
        }
        Flow::Cancel
    }

    fn screen_render_post(&self, host: &dyn Host, event: &mut ScreenRenderPostEvent<'_>) {
        if host.mod_screen() != Some(ID) {
            return;
        }
        let tamanho = host
            .client_state()
            .map_or([event.screen.rect[2], event.screen.rect[3]], |s| s.gui_size);
        let mut e = self.estado();
        let e = &mut *e;
        let desenho = e
            .tela
            .desenhar(&e.amigos, tamanho, event.screen.mouse, |t| {
                host.text_width(t)
            });
        for (r, cor) in desenho.retangulos {
            host.fill(event.batch, r, cor);
        }
        for (t, at, cor) in desenho.textos {
            host.text(event.batch, &t, at, cor, true);
        }
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn Host,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(ID) || event.button != 0 {
            return Flow::Continue;
        }
        let mut e = self.estado();
        let e = &mut *e;
        match e.tela.clicar(&mut e.amigos, event.screen.mouse) {
            None => Flow::Continue,
            Some(efeito) => {
                aplicar(e, efeito);
                Flow::Cancel
            }
        }
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn Host,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let mut e = self.estado();
        let e = &mut *e;
        // Ctrl+V cola no campo.
        if event.key == "key.keyboard.v" && event.modifiers & 2 != 0 {
            let texto = e.area().get();
            e.tela.colar(&texto);
            return Flow::Cancel;
        }
        match e.tela.tecla(&mut e.amigos, event.key) {
            None => Flow::Continue,
            Some(efeito) => {
                aplicar(e, efeito);
                Flow::Cancel
            }
        }
    }

    fn screen_char_typed_pre(
        &self,
        host: &dyn Host,
        event: &mut ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        if self.estado().tela.digitar(event.c) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }
}

/// Faz o que um clique ou tecla da tela pediu.
fn aplicar(e: &mut Estado, efeito: tela::Efeito) {
    match efeito {
        tela::Efeito::Nada => {}
        tela::Efeito::Fechar => e.fechar_tela = true,
        tela::Efeito::Convidar(alvo) => {
            let area = e.area().get();
            e.amigos.convidar(&alvo, &area);
        }
        tela::Efeito::Copiar(texto) => e.area().set(&texto),
    }
}

/// Uma caixa com linhas num canto, empilhada com o que já estiver lá.
fn caixa(host: &dyn Host, event: &mut HudEvent<'_>, canto: Corner, linhas: &[(String, u32)]) {
    if linhas.is_empty() {
        return;
    }
    let w = linhas
        .iter()
        .map(|(t, _)| host.text_width(t))
        .fold(0.0, f32::max)
        + 6.0;
    let h = linhas.len() as f32 * 10.0 + 5.0;
    let (x, y) = event.corners.place(canto, w, h);
    host.fill(event.batch, [x, y, w, h], 0x9000_0000);
    for (i, (t, cor)) in linhas.iter().enumerate() {
        host.text(
            event.batch,
            t,
            [x + 3.0, y + 3.0 + i as f32 * 10.0],
            *cor,
            true,
        );
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn roda_aproxima_e_afasta_com_limites() {
        assert!(rolar_nivel(4.0, 1.0) > 4.0);
        assert!(rolar_nivel(4.0, -1.0) < 4.0);
        assert_eq!(rolar_nivel(4.0, 100.0), ZOOM_MAX);
        assert_eq!(rolar_nivel(4.0, -100.0), ZOOM_MIN);
    }

    #[test]
    fn pontos_cardeais() {
        assert_eq!(direcao(0.0), "Sul (+Z)");
        assert_eq!(direcao(90.0), "Oeste (-X)");
        assert_eq!(direcao(180.0), "Norte (-Z)");
        assert_eq!(direcao(-90.0), "Leste (+X)");
        assert_eq!(direcao(359.0), "Sul (+Z)");
    }

    #[test]
    fn segredos_sao_diferentes() {
        let a = novo_segredo();
        assert_eq!(a.len(), 32);
        assert_ne!(a, novo_segredo());
    }
}
