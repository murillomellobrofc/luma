use smithay::{
    reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    wayland::shell::xdg::{
        decoration::{XdgDecorationHandler, XdgDecorationState},
        ToplevelSurface,
    },
};

use crate::state::LumaState;

/// Luma força modo ServerSide em todas as janelas, sem opção pro cliente.
/// É o que faz sentido pra um DE que sempre desenha as próprias barras:
/// o cliente fica encarregado só do conteúdo. Volta a discutir quando
/// tivermos preferência de usuário (settings).
impl XdgDecorationHandler for LumaState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _requested: Mode) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }
}

impl LumaState {
    pub fn xdg_decoration_state(&mut self) -> &mut XdgDecorationState {
        &mut self.xdg_decoration_state
    }
}
