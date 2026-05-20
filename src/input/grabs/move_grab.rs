use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent,
        GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
        GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
        GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab, PointerInnerHandle,
        RelativeMotionEvent,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
};

use crate::{state::LumaState, window::LumaWindow};

const BTN_LEFT: u32 = 0x110;
/// Distância da borda do output que dispara snap durante drag.
const SNAP_ZONE: f64 = 5.0;

#[derive(Debug, Clone, Copy)]
pub enum SnapKind {
    Maximize,
    Left,
    Right,
}

pub struct MoveSurfaceGrab {
    pub start_data: PointerGrabStartData<LumaState>,
    pub window: LumaWindow,
    pub initial_window_location: Point<i32, Logical>,
    /// Snap a ser aplicado quando o usuário soltar o botão; atualizado a
    /// cada motion baseado em onde o cursor está em relação às bordas do
    /// output.
    pub pending_snap: Option<SnapKind>,
}

impl PointerGrab<LumaState> for MoveSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // Enquanto o grab está ativo, nenhum cliente recebe pointer focus.
        handle.motion(data, None, event);

        let delta = event.location - self.start_data.location;
        let new_location = self.initial_window_location.to_f64() + delta;
        data.space.map_element(
            crate::space_item::SpaceItem::Window(self.window.clone()),
            new_location.to_i32_round(),
            true,
        );

        // Detecta zona de snap conforme a posição do cursor no output.
        self.pending_snap = detect_snap_zone(data, event.location);
    }

    fn relative_motion(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if !handle.current_pressed().contains(&BTN_LEFT) {
            // Aplica snap, se houver, ANTES de soltar o grab — assim o
            // map_element do snap tem efeito imediato.
            if let Some(kind) = self.pending_snap {
                match kind {
                    SnapKind::Maximize => {
                        // Só maximiza se ainda não estiver maximizada — evita
                        // toggle invertido quando arrasta uma janela já
                        // maximizada pro topo.
                        let already = crate::decoration::with_decoration(
                            self.window.window(),
                            |d| d.restore_location.is_some(),
                        );
                        if !already {
                            data.toggle_maximize(&self.window);
                        }
                    }
                    SnapKind::Left => data.snap_half(&self.window, true),
                    SnapKind::Right => data.snap_half(&self.window, false),
                }
            }
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut LumaState, handle: &mut PointerInnerHandle<'_, LumaState>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &PointerGrabStartData<LumaState> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut LumaState) {}
}

fn detect_snap_zone(
    data: &LumaState,
    pointer_pos: Point<f64, Logical>,
) -> Option<SnapKind> {
    let output = data.output_under_cursor(pointer_pos);
    let output_size = data.space.output_geometry(output)?.size;

    if pointer_pos.y <= SNAP_ZONE {
        Some(SnapKind::Maximize)
    } else if pointer_pos.x <= SNAP_ZONE {
        Some(SnapKind::Left)
    } else if pointer_pos.x >= output_size.w as f64 - SNAP_ZONE {
        Some(SnapKind::Right)
    } else {
        None
    }
}
