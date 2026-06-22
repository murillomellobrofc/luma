//! Handler do protocolo wlr-layer-shell. Permite que clientes externos
//! (a `luma-bar`, e no geral waybar/swaybg/mako) abram surfaces ancoradas em
//! camadas (background, bottom, top, overlay) por output, com zona exclusiva.
//!
//! Implementação real (luma-bar passo 1): mapeia a surface num `LayerMap` por
//! output (via `layer_map_for_output`), manda o configure inicial e arranja
//! no commit. O **render é de graça** — o `space::render_output` do smithay já
//! desenha os layers do `layer_map_for_output` na ordem z certa (background/
//! bottom abaixo das janelas, top/overlay acima). A **zona exclusiva** entra
//! no `next_cascade_position` (área útil = output menos os layers exclusivos).

use std::sync::Mutex;

use smithay::{
    desktop::{layer_map_for_output, LayerSurface, WindowSurfaceType},
    output::Output,
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    wayland::{
        compositor::with_states,
        shell::wlr_layer::{
            Layer, LayerSurface as WlrLayerSurface, LayerSurfaceAttributes, WlrLayerShellHandler,
            WlrLayerShellState,
        },
    },
};
use tracing::{info, warn};

use crate::state::LumaState;

impl WlrLayerShellHandler for LumaState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        wl_output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        // Resolve o output destino: o que o cliente passou, senão o primário.
        // Cada layer surface PERTENCE a 1 output (LayerMap por output).
        let output = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .unwrap_or_else(|| self.primary_output().clone());
        info!(
            "nova layer surface (namespace={namespace:?}, output={:?}) — mapeando",
            output.name()
        );
        let mut map = layer_map_for_output(&output);
        if let Err(err) = map.map_layer(&LayerSurface::new(surface, namespace)) {
            warn!("falha ao mapear layer surface: {err:?}");
        }
        // O configure inicial e o arrange saem no commit (`handle_layer_commit`).
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        // Desmapeia do output dono e re-arranja (libera a zona exclusiva).
        let output = self.space.outputs().find(|o| {
            let map = layer_map_for_output(o);
            map.layer_for_surface(surface.wl_surface(), WindowSurfaceType::TOPLEVEL)
                .is_some()
        });
        if let Some(output) = output.cloned() {
            let mut map = layer_map_for_output(&output);
            if let Some(layer) = map
                .layer_for_surface(surface.wl_surface(), WindowSurfaceType::TOPLEVEL)
                .cloned()
            {
                map.unmap_layer(&layer);
                map.arrange();
            }
        }
        info!("layer surface destruída");
    }
}

/// Trata o commit de uma layer surface (chamado do `CompositorHandler::commit`):
/// arranja o `LayerMap` do output dono e manda o **configure inicial** na
/// primeira vez. Retorna `true` se a surface era mesmo uma layer surface (pra
/// o commit comum poder pular o resto). Sem dono = não é layer surface.
pub(crate) fn handle_layer_commit(state: &mut LumaState, surface: &WlSurface) -> bool {
    let output = state
        .space
        .outputs()
        .find(|o| {
            layer_map_for_output(o)
                .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                .is_some()
        })
        .cloned();
    let Some(output) = output else {
        return false;
    };

    let mut map = layer_map_for_output(&output);
    // Re-arranja sempre (a surface pode ter mudado âncora/tamanho/zona).
    map.arrange();

    let initial_configure_sent = with_states(surface, |states| {
        states
            .data_map
            .get::<Mutex<LayerSurfaceAttributes>>()
            .map(|attrs| attrs.lock().unwrap().initial_configure_sent)
            .unwrap_or(false)
    });
    if !initial_configure_sent {
        if let Some(layer) = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL) {
            layer.layer_surface().send_configure();
        }
    }
    true
}

// `delegate_dispatch2!(LumaState)` em handlers/mod.rs cobre o dispatch
// das mensagens deste protocolo (smithay master usa macro unificado em
// vez de uma macro `delegate_layer_shell!` específica).
