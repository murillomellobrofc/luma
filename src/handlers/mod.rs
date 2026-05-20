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

smithay::delegate_dispatch2!(LumaState);
