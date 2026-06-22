use smithay::{
    desktop::{PopupKind, PopupManager, Space, Window},
    reexports::wayland_server::protocol::{wl_seat, wl_surface::WlSurface},
    utils::Serial,
    wayland::{
        compositor::with_states,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
    },
};
use tracing::debug;

use crate::{space_item::SpaceItem, state::LumaState, window::LumaWindow};

impl XdgShellHandler for LumaState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let position = self.next_cascade_position();
        debug!("nova janela top-level chegando em {position:?}");
        let wl_surface = surface.wl_surface().clone();
        let window = Window::new_wayland_window(surface);
        // Id estável pra a taskbar (luma-bar passo 2), no UserDataMap.
        let win_id = self.next_window_id;
        self.next_window_id += 1;
        window
            .user_data()
            .insert_if_missing(|| crate::state::WindowId(win_id));
        crate::decoration::ensure_decoration(&window);
        self.space.map_element(
            SpaceItem::Window(LumaWindow::new(window)),
            position,
            false,
        );
        // H36.3: registra no workspace atual.
        self.workspaces.add_to_current(wl_surface);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }

    fn move_request(
        &mut self,
        _surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        _serial: Serial,
    ) {
        // grabs interativos vêm na próxima etapa
    }

    fn resize_request(
        &mut self,
        _surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        _serial: Serial,
        _edges: smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge,
    ) {
        // grabs interativos vêm na próxima etapa
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
}

pub fn handle_commit(popups: &mut PopupManager, space: &Space<SpaceItem>, surface: &WlSurface) {
    if let Some(window) = space
        .elements()
        .filter_map(|i| i.as_window())
        .find(|w| w.window().toplevel().unwrap().wl_surface() == surface)
        .cloned()
    {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        if !initial_configure_sent {
            window.window().toplevel().unwrap().send_configure();
        }
    }

    popups.commit(surface);
    if let Some(popup) = popups.find_popup(surface) {
        if let PopupKind::Xdg(ref xdg) = popup {
            if !xdg.is_initial_configure_sent() {
                xdg.send_configure().expect("initial configure failed");
            }
        }
    }
}

