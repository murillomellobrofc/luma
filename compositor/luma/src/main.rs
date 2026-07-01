mod apps;
mod audio;
mod auth;
mod brightness;
mod places;
mod backends;
mod bluetooth;
mod bt_agent;
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
    // Barra velha aposentada (LUMA_LEGACY_BAR off): não reserva a zona dela
    // (taskbar_height→0), pra não sobrar faixa fantasma embaixo. O desenho já
    // é gateado à parte no render.
    taskbar::set_bar_off(std::env::var("LUMA_LEGACY_BAR").is_err());

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
                    dbus_server::IpcCommand::ActivateWindow(id) => {
                        data.state.activate_window_by_id(id);
                    }
                    dbus_server::IpcCommand::MinimizeWindow(id) => {
                        data.state.minimize_window_by_id(id);
                    }
                    dbus_server::IpcCommand::CloseWindow(id) => {
                        data.state.close_window_by_id(id);
                    }
                    dbus_server::IpcCommand::ShowDesktop => {
                        data.state.toggle_show_desktop();
                    }
                    dbus_server::IpcCommand::SetVolume(pct) => {
                        crate::input::shortcuts::set_volume_abs(&mut data.state, pct);
                    }
                    dbus_server::IpcCommand::ToggleMute => {
                        crate::input::shortcuts::volume_mute_toggle(&mut data.state);
                    }
                    dbus_server::IpcCommand::Lock => {
                        crate::input::shortcuts::lock_session(&mut data.state);
                    }
                    dbus_server::IpcCommand::CtxAction(action) => {
                        crate::input::pointer::run_desktop_action(&mut data.state, action);
                    }
                    dbus_server::IpcCommand::WorkspaceSwitch(idx) => {
                        while data.state.workspaces.count() <= idx {
                            data.state.workspaces.create();
                        }
                        data.state.workspaces.switch_to(idx);
                        data.state.refresh_workspace_visibility();
                    }
                    dbus_server::IpcCommand::WorkspaceCreate => {
                        data.state.create_workspace();
                    }
                    dbus_server::IpcCommand::WorkspaceMoveWindow { window_id, target } => {
                        data.state.move_window_to_workspace_by_id(window_id, target);
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
    // Slot de janelas (luma-bar passo 2): o servidor D-Bus lê via list_windows.
    let windows_slot = data.state.windows.clone();
    // Receiver da bateria (luma-bar passo 3): o servidor emite o sinal no
    // `changed()`; o compositor manda no refresh quando muda.
    let battery_rx = data.state.battery_tx.subscribe();
    // Receiver do volume (luma-bar passo 4): mesmo esquema da bateria.
    let volume_rx = data.state.volume_tx.subscribe();
    let network_rx = data.state.network_tx.subscribe();
    let bluetooth_rx = data.state.bluetooth_tx.subscribe();
    let menu_toggle_rx = data.state.menu_toggle_tx.subscribe();
    let desktop_menu_rx = data.state.desktop_menu_tx.subscribe();
    let sni_list = data.state.sni_list.clone();
    let workspace_slot = data.state.workspace_slot.clone();
    let workspace_changed_rx = data.state.workspace_changed_tx.subscribe();
    dbus_server::spawn(
        dbus_tx,
        available,
        notifications_slot,
        windows_slot,
        battery_rx,
        volume_rx,
        network_rx,
        bluetooth_rx,
        menu_toggle_rx,
        desktop_menu_rx,
        sni_list,
        workspace_slot,
        workspace_changed_rx,
    );

    info!("luma pronto, entrando no loop principal");

    event_loop.run(None, &mut data, |data| {
        data.display.flush_clients().ok();
    })?;

    info!("luma encerrando");
    Ok(())
}
