//! Essencial: zoom, HUD de informações e amigos (lista, chat e convites)
//! para o Horizon. Escrito do zero; inspirado no que o mod Essential faz, sem
//! usar o código dele.
//!
//! - `zoom`: segure C (Opções > Controles) e use a roda do mouse.
//! - `hud`: FPS, coordenadas, direção e ping num canto da tela.
//! - `amigos`: conecta ao servidor de amigos (`servidor/` neste repositório),
//!   com a tela de amigos (tecla O) e os comandos `.amigos`, `.msg` etc.

mod amigos;
mod hud;
mod protocolo;
mod texto;
mod zoom;

use mod_api::{
    self, ClientHost, ClientStoppingEvent, ClientTickPreEvent, ComputeFovModifierEvent, Config,
    ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent, Flow, Hooks, HostSide,
    KeyConflictContext, KeyRegistration, MouseScrollingEvent, RegisterConfigsEvent, Registrar,
    RegistrationContext, RenderGuiPostEvent, ScreenCharTypedPreEvent, ScreenKeyPressedPreEvent,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent, ScreenRenderPostEvent,
    SettingSpec,
};

/// O id do mod, igual ao de `horizon-mod.toml`.
pub const ID: &str = "essencial";

/// O arquivo de configuração, em `config/` da instância.
const ARQUIVO_CONFIG: &str = "essencial.toml";

#[derive(Default)]
struct Essencial {
    zoom: zoom::Zoom,
    amigos: amigos::Amigos,
}

/// As opções em Opções > Mods > Essencial.
fn opcoes() -> Vec<SettingSpec> {
    let t = |k: &str| format!("essencial.opcao.{k}");
    let d = |k: &str| format!("essencial.opcao.{k}.dica");
    vec![
        SettingSpec::range("zoom_nivel", t("zoom_nivel"), 2.0, 10.0, 0.5, 4.0)
            .tooltip(d("zoom_nivel")),
        SettingSpec::toggle("zoom_rolagem", t("zoom_rolagem"), true).tooltip(d("zoom_rolagem")),
        SettingSpec::toggle("zoom_alternar", t("zoom_alternar"), false).tooltip(d("zoom_alternar")),
        SettingSpec::toggle("hud", t("hud"), true).tooltip(d("hud")),
        SettingSpec::toggle("hud_fps", t("hud_fps"), true),
        SettingSpec::toggle("hud_coordenadas", t("hud_coordenadas"), true),
        SettingSpec::toggle("hud_direcao", t("hud_direcao"), true),
        SettingSpec::toggle("hud_ping", t("hud_ping"), true),
        SettingSpec::choice(
            "hud_canto",
            t("hud_canto"),
            [
                t("hud_canto.superior_esquerdo"),
                t("hud_canto.superior_direito"),
                t("hud_canto.inferior_esquerdo"),
                t("hud_canto.inferior_direito"),
            ],
            0,
        ),
        SettingSpec::toggle("amigos", t("amigos"), true).tooltip(d("amigos")),
        SettingSpec::toggle(
            "amigos_mostrar_servidor",
            t("amigos_mostrar_servidor"),
            true,
        )
        .tooltip(d("amigos_mostrar_servidor")),
        SettingSpec::toggle("amigos_avisos", t("amigos_avisos"), true).tooltip(d("amigos_avisos")),
    ]
}

#[mod_api::horizon_mod]
impl Hooks for Essencial {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        for opcao in opcoes() {
            if let Err(e) = r.register_setting(opcao) {
                eprintln!("essencial: opção recusada: {e:?}");
            }
        }
        r.register_key_mapping(
            KeyRegistration::new("zoom", "key.keyboard.c")
                .label("essencial.tecla.zoom")
                .category("essencial.tecla.categoria")
                .context(KeyConflictContext::InGame),
        );
        r.register_key_mapping(
            KeyRegistration::new("amigos", "key.keyboard.o")
                .label("essencial.tecla.amigos")
                .category("essencial.tecla.categoria")
                .context(KeyConflictContext::InGame),
        );
    }

    fn register_configs(&self, _host: HostSide, event: &mut RegisterConfigsEvent<'_>) {
        event
            .configs
            .push(Config::new(ID, ConfigKind::Client, amigos::CONFIG_PADRAO).file(ARQUIVO_CONFIG));
    }

    fn config_loading(&self, _host: HostSide, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id == ID {
            self.amigos.configurar(&event.config.text);
        }
    }

    fn config_reloading(&self, _host: HostSide, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id == ID {
            self.amigos.configurar(&event.config.text);
        }
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _: &mut ClientTickPreEvent) {
        self.zoom.tick(host);
        self.amigos.tick(host);
    }

    fn client_stopping(&self, _host: &mut dyn ClientHost, _: &mut ClientStoppingEvent) {
        self.amigos.parar();
    }

    fn compute_fov_modifier(
        &self,
        _host: &dyn ClientHost,
        event: &mut ComputeFovModifierEvent<'_>,
    ) {
        self.zoom.fov(event);
    }

    fn mouse_scrolling(&self, host: &mut dyn ClientHost, event: &mut MouseScrollingEvent) -> Flow {
        self.zoom.rolar(host, event)
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        hud::desenhar(host, event, &self.zoom, &self.amigos);
    }

    fn client_chat(
        &self,
        host: &mut dyn ClientHost,
        event: &mut mod_api::ClientChatEvent<'_>,
    ) -> Flow {
        self.amigos.chat(host, event.message.as_str())
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        self.amigos.tela_desenhar(host, event);
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        let (at, button, modifiers) = (event.screen.mouse, event.button, event.modifiers);
        self.amigos
            .tela_entrada(host, |w| w.mouse_pressed(host, at, button, modifiers))
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        let button = event.button;
        self.amigos.tela_entrada(host, |w| w.mouse_released(button))
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        let (key, modifiers) = (event.key, event.modifiers);
        self.amigos
            .tela_entrada(host, |w| w.key_pressed(host, key, modifiers))
    }

    fn screen_char_typed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        let c = event.c;
        self.amigos.tela_entrada(host, |w| w.char_typed(host, c))
    }
}
