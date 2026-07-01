mod compositor;
mod layer_shell;
mod xdg_decoration;
mod xdg_shell;

use smithay::{
    input::{
        dnd::{DndGrabHandler, GrabType, Source},
        Seat, SeatHandler, SeatState,
    },
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::Serial,
    wayland::{
        output::OutputHandler,
        selection::{
            data_device::{
                set_data_device_focus, DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler,
            },
            SelectionHandler,
        },
    },
};

use crate::state::LumaState;

impl SeatHandler for LumaState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<LumaState> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        image: smithay::input::pointer::CursorImageStatus,
    ) {
        self.cursor_status = image;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}

impl SelectionHandler for LumaState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for LumaState {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl DndGrabHandler for LumaState {}

impl WaylandDndGrabHandler for LumaState {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        _seat: Seat<Self>,
        _serial: Serial,
        _type_: GrabType,
    ) {
        // dnd interativo vem em fase posterior, cancela por agora
        source.cancel();
    }
}

impl OutputHandler for LumaState {}

impl smithay::wayland::session_lock::SessionLockHandler for LumaState {
    fn lock_state(&mut self)
        -> &mut smithay::wayland::session_lock::SessionLockManagerState
    {
        &mut self.session_lock_state
    }

    fn lock(
        &mut self,
        confirmation: smithay::wayland::session_lock::SessionLocker,
    ) {
        // H58 Fatia 1: confirma o lock imediatamente. Sem render
        // próprio ainda — render path nas próximas fatias suprime as
        // surfaces normais. Por ora, marca o flag.
        tracing::info!("session lock: client requested lock");
        self.locked = true;
        confirmation.lock();
    }

    fn unlock(&mut self) {
        // Segurança: um lock interno protegido por PAM NÃO pode ser
        // destravado pelo protocolo ext-session-lock — senão qualquer
        // cliente Wayland limparia o lock autenticado do compositor.
        // Só o sucesso do PAM (backends/winit.rs) limpa esse caso.
        if self.secure_lock {
            tracing::warn!(
                "session lock: unlock ignorado — lock PAM ativo"
            );
            return;
        }
        tracing::info!("session lock: unlock");
        self.locked = false;
        self.lock_surfaces.clear();
    }

    fn new_surface(
        &mut self,
        surface: smithay::wayland::session_lock::LockSurface,
        _output: smithay::reexports::wayland_server::protocol::wl_output::WlOutput,
    ) {
        tracing::info!("session lock: new lock surface");
        self.lock_surfaces.push(surface);
    }
}

smithay::delegate_dispatch2!(LumaState);
