//! Handler do protocolo wlr-layer-shell. Permite que clientes externos
//! (waybar, swaybg, mako, etc) abram surfaces ancoradas em camadas
//! (background, bottom, top, overlay) por output.
//!
//! H34.4: stub funcional. Registra o global, aceita layer surfaces e
//! resolve corretamente o output (do `wl_output` se passado pelo
//! cliente, ou `primary_output()` como fallback). NÃO mapeia/renderiza
//! ainda — render layer surfaces fica pra etapa futura quando o
//! `SpaceItem` ganhar variante `LayerSurface`.

use smithay::{
    output::Output,
    reexports::wayland_server::protocol::wl_output::WlOutput,
    wayland::shell::wlr_layer::{
        Layer, LayerSurface as WlrLayerSurface, WlrLayerShellHandler, WlrLayerShellState,
    },
};
use tracing::info;

use crate::state::LumaState;

impl WlrLayerShellHandler for LumaState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        _surface: WlrLayerSurface,
        wl_output: Option<WlOutput>,
        layer: Layer,
        namespace: String,
    ) {
        // Resolve o output destino. Se o cliente passou `wl_output`,
        // usa ele; senão cai no `primary_output()` (1 monitor por
        // enquanto). Esse é o ponto crítico do refactor multi-monitor:
        // cada layer surface PERTENCE a 1 output específico.
        let output = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .unwrap_or_else(|| self.primary_output().clone());
        info!(
            "nova layer surface (namespace={namespace:?}, layer={layer:?}, output={:?}) — ainda não renderizada",
            output.name()
        );
        // Sem map/render — isso vai numa etapa futura quando
        // SpaceItem ganhar variante LayerSurface ou tivermos um
        // layer_map por output.
    }

    fn layer_destroyed(&mut self, _surface: WlrLayerSurface) {
        info!("layer surface destruída");
    }
}

// `delegate_dispatch2!(LumaState)` em handlers/mod.rs cobre o dispatch
// das mensagens deste protocolo (smithay master usa macro unificado em
// vez de uma macro `delegate_layer_shell!` específica).
