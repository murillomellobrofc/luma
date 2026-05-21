mod apps;
mod audio;
mod brightness;
mod places;
mod backends;
mod context_menu;
mod cursor;
mod decoration;
mod gfx;
mod handlers;
mod input;
mod menu;
mod media;
mod osd;
mod overview;
mod pager;
mod popup;
mod render;
mod search;
mod sni;
mod workspace;
mod space_item;
mod state;
mod taskbar;
mod text;
mod theme;
mod tray;
mod wallpaper;
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
    info!("luma pronto, entrando no loop principal");

    event_loop.run(None, &mut data, |data| {
        data.display.flush_clients().ok();
    })?;

    info!("luma encerrando");
    Ok(())
}
