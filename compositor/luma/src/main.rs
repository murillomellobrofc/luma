mod apps;
mod audio;
mod auth;
mod brightness;
mod places;
mod backends;
mod bluetooth;
mod context_menu;
mod cursor;
mod dbus_server;
mod decoration;
mod desktop_icons;
mod gfx;
mod handlers;
mod input;
mod menu;
mod notifications;
mod media;
mod osd;
mod overview;
mod pager;
mod popup;
mod power;
mod render;
mod search;
mod settings;
mod sni;
mod sound;
mod workspace;
mod space_item;
mod state;
mod taskbar;
mod text;
mod theme;
mod tray;
mod wallpaper;
mod widgets;
mod wifi;
mod window;
mod window_shadow;

use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::Display,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::{
    backends::winit::init_winit,
    state::{init_socket, CalloopData, LumaState},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,luma=debug")),
        )
        .init();

    info!("luma {} iniciando", env!("CARGO_PKG_VERSION"));
    theme::init();

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let mut display: Display<LumaState> = Display::new()?;

    let state = LumaState::new(&mut event_loop, &mut display)?;
    let mut data = CalloopData { state, display };

    init_winit(&mut event_loop, &mut data)?;

    data.state.socket_name = init_socket(&mut event_loop)?;
    info!("socket wayland: {}", data.state.socket_name);

    // 3b/3c.1: canal calloop entre a thread do servidor D-Bus e o
    // loop principal. Receiver registrado aqui despacha cada IpcCommand
    // pro `LumaState`, e responde pelo oneshot do command.
    let (dbus_tx, dbus_rx) =
        smithay::reexports::calloop::channel::channel::<dbus_server::IpcCommand>();
    event_loop
        .handle()
        .insert_source(dbus_rx, |event, _, data| {
            if let smithay::reexports::calloop::channel::Event::Msg(cmd) = event {
                match cmd {
                    dbus_server::IpcCommand::AddWidget {
                        type_name,
                        target,
                        reply,
                    } => {
                        let result = data.state.add_widget(&type_name, target);
                        // Reply pode falhar se o lado D-Bus desistiu —
                        // não é fatal pro compositor, só log.
                        if reply.send(result).is_err() {
                            tracing::warn!(
                                "D-Bus AddWidget: reply oneshot fechado antes da resposta"
                            );
                        }
                    }
                }
            }
        })
        .map_err(|err| format!("registrar receiver D-Bus no calloop: {err}"))?;

    // Catálogo estático (3c.1): clonado do registry pra entregar pra
    // thread D-Bus. Sem registry (TextRenderer falhou) = catálogo vazio,
    // ListAvailableWidgets ainda funciona mas devolve [].
    let available: Vec<luma_ipc::WidgetInfo> = data
        .state
        .widget_registry
        .as_ref()
        .map(|r| {
            r.available()
                .map(|(type_name, meta)| luma_ipc::WidgetInfo {
                    type_name: type_name.to_string(),
                    display_name: meta.display_name.clone(),
                    description: meta.description.clone(),
                    icon_name: meta.icon_name.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    // Slot de notificações compartilhado (clone do Arc) pro sino visual (3a):
    // o servidor D-Bus lê/escreve `flash_enabled` direto dele.
    let notifications_slot = data.state.notifications.clone();
    dbus_server::spawn(dbus_tx, available, notifications_slot);

    info!("luma pronto, entrando no loop principal");

    event_loop.run(None, &mut data, |data| {
        data.display.flush_clients().ok();
    })?;

    info!("luma encerrando");
    Ok(())
}
