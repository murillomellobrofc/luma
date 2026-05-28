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

    // 3b: canal calloop entre a thread do servidor D-Bus e o loop
    // principal. Receiver registrado aqui (ocioso até a 3c); sender
    // entregue à thread que sobe o zbus.
    let (dbus_tx, dbus_rx) =
        smithay::reexports::calloop::channel::channel::<dbus_server::IpcCommand>();
    event_loop
        .handle()
        .insert_source(dbus_rx, |_event, _, _data| {
            // 3c em diante: despacha IpcCommand pro estado do compositor.
        })
        .map_err(|err| format!("registrar receiver D-Bus no calloop: {err}"))?;
    dbus_server::spawn(dbus_tx);

    info!("luma pronto, entrando no loop principal");

    event_loop.run(None, &mut data, |data| {
        data.display.flush_clients().ok();
    })?;

    info!("luma encerrando");
    Ok(())
}
