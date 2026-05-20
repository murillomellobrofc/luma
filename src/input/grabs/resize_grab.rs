use std::cell::RefCell;

use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent,
        GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
        GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
        GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab, PointerInnerHandle,
        RelativeMotionEvent,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{Logical, Point, Rectangle, Size},
    wayland::{compositor::with_states, shell::xdg::SurfaceCachedState},
};

use crate::{state::LumaState, window::LumaWindow};

const BTN_LEFT: u32 = 0x110;

bitflags::bitflags! {
    /// Qual lado da janela está sendo arrastado. Combinações TOP|LEFT etc
    /// representam cantos. Quando TOP ou LEFT estão ativos, a janela
    /// precisa ser movida no commit pra ela "crescer" pra cima/esquerda
    /// visualmente sem sair do lugar.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ResizeEdge: u32 {
        const TOP    = 0b0001;
        const BOTTOM = 0b0010;
        const LEFT   = 0b0100;
        const RIGHT  = 0b1000;
    }
}

/// Espessura em pixels da borda sensível ao resize. Maior que a borda
/// visual (3px) pra ficar mais fácil de pegar com o mouse.
pub const RESIZE_EDGE_THICKNESS: i32 = 14;

/// Estado de resize armazenado no UserDataMap da WlSurface, usado pelo
/// handle_commit pra saber se a posição precisa ser ajustada quando o
/// cliente confirma um novo tamanho com TOP ou LEFT envolvido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeSurfaceState {
    #[default]
    Idle,
    Resizing {
        edges: ResizeEdge,
        initial_rect: Rectangle<i32, Logical>,
    },
    WaitingForLastCommit {
        edges: ResizeEdge,
        initial_rect: Rectangle<i32, Logical>,
    },
}

impl ResizeSurfaceState {
    pub fn with<F, T>(surface: &WlSurface, cb: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
    {
        with_states(surface, |states| {
            states.data_map.insert_if_missing(RefCell::<Self>::default);
            let state = states.data_map.get::<RefCell<Self>>().unwrap();
            cb(&mut state.borrow_mut())
        })
    }

    /// Consome o estado e devolve `(edges, initial_rect)` se há resize
    /// rolando ou esperando o último commit (que vira Idle automaticamente).
    pub fn commit(&mut self) -> Option<(ResizeEdge, Rectangle<i32, Logical>)> {
        match *self {
            Self::Resizing {
                edges,
                initial_rect,
            } => Some((edges, initial_rect)),
            Self::WaitingForLastCommit {
                edges,
                initial_rect,
            } => {
                *self = Self::Idle;
                Some((edges, initial_rect))
            }
            Self::Idle => None,
        }
    }
}

pub struct ResizeSurfaceGrab {
    pub start_data: PointerGrabStartData<LumaState>,
    pub window: LumaWindow,
    pub edges: ResizeEdge,
    pub initial_rect: Rectangle<i32, Logical>,
    pub last_window_size: Size<i32, Logical>,
}

impl ResizeSurfaceGrab {
    pub fn start(
        start_data: PointerGrabStartData<LumaState>,
        window: LumaWindow,
        edges: ResizeEdge,
        initial_rect: Rectangle<i32, Logical>,
    ) -> Self {
        if let Some(toplevel) = window.window().toplevel() {
            ResizeSurfaceState::with(toplevel.wl_surface(), |state| {
                *state = ResizeSurfaceState::Resizing {
                    edges,
                    initial_rect,
                };
            });
        }

        Self {
            start_data,
            window,
            edges,
            initial_rect,
            last_window_size: initial_rect.size,
        }
    }
}

impl PointerGrab<LumaState> for ResizeSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut LumaState,
        handle: &mut PointerInnerHandle<'_, LumaState>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);

        let mut delta = event.location - self.start_data.location;
        let mut new_w = self.initial_rect.size.w;
        let mut new_h = self.initial_rect.size.h;

        if self.edges.intersects(ResizeEdge::LEFT | ResizeEdge::RIGHT) {
            if self.edges.intersects(ResizeEdge::LEFT) {
                delta.x = -delta.x;
            }
            new_w = (self.initial_rect.size.w as f64 + delta.x).round() as i32;
        }
        if self.edges.intersects(ResizeEdge::TOP | ResizeEdge::BOTTOM) {
            if self.edges.intersects(ResizeEdge::TOP) {
                delta.y = -delta.y;
            }
            new_h = (self.initial_rect.size.h as f64 + delta.y).round() as i32;
        }

        let Some(toplevel) = self.window.window().toplevel().cloned() else {
            return;
        };

        let (min_size, max_size) = with_states(toplevel.wl_surface(), |states| {
            let mut guard = states.cached_state.get::<SurfaceCachedState>();
            let cached = guard.current();
            (cached.min_size, cached.max_size)
        });
        let min_w = min_size.w.max(1);
        let min_h = min_size.h.max(1);
        let max_w = if max_size.w == 0 { i32::MAX } else { max_size.w };
        let max_h = if max_size.h == 0 { i32::MAX } else { max_size.h };

        self.last_window_size = Size::from((
            new_w.max(min_w).min(max_w),
            new_h.max(min_h).min(max_h),
        ));

        toplevel.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
            state.size = Some(self.last_window_size);
        });
        toplevel.send_pending_configure();
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
            handle.unset_grab(self, data, event.serial, event.time, true);

            if let Some(toplevel) = self.window.window().toplevel().cloned() {
                toplevel.with_pending_state(|state| {
                    state.states.unset(xdg_toplevel::State::Resizing);
                    state.size = Some(self.last_window_size);
                });
                toplevel.send_pending_configure();

                ResizeSurfaceState::with(toplevel.wl_surface(), |state| {
                    *state = ResizeSurfaceState::WaitingForLastCommit {
                        edges: self.edges,
                        initial_rect: self.initial_rect,
                    };
                });
            }
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

/// Chamado pelo CompositorHandler.commit. Quando uma surface confirma
/// um novo tamanho durante (ou logo após) um resize com TOP ou LEFT
/// envolvido, a posição da janela no Space precisa ser ajustada pra
/// compensar a mudança de tamanho — só assim o cliente "cresce pra
/// cima" ou "pra esquerda" sem o canto oposto sair do lugar.
pub fn handle_commit(
    space: &mut smithay::desktop::Space<crate::space_item::SpaceItem>,
    surface: &WlSurface,
) -> Option<()> {
    let window = space
        .elements()
        .filter_map(|i| i.as_window())
        .find(|w| w.window().toplevel().map(|t| t.wl_surface() == surface).unwrap_or(false))
        .cloned()?;

    let item = crate::space_item::SpaceItem::Window(window.clone());
    let mut window_loc = space.element_location(&item)?;
    let geometry = window.window().geometry();

    let new_loc: Point<Option<i32>, Logical> = ResizeSurfaceState::with(surface, |state| {
        state
            .commit()
            .and_then(|(edges, initial_rect)| {
                edges
                    .intersects(ResizeEdge::TOP | ResizeEdge::LEFT)
                    .then(|| {
                        let new_x = edges.intersects(ResizeEdge::LEFT).then_some(
                            initial_rect.loc.x + (initial_rect.size.w - geometry.size.w),
                        );
                        let new_y = edges.intersects(ResizeEdge::TOP).then_some(
                            initial_rect.loc.y + (initial_rect.size.h - geometry.size.h),
                        );
                        (new_x, new_y).into()
                    })
            })
            .unwrap_or_default()
    });

    if let Some(new_x) = new_loc.x {
        window_loc.x = new_x;
    }
    if let Some(new_y) = new_loc.y {
        window_loc.y = new_y;
    }
    if new_loc.x.is_some() || new_loc.y.is_some() {
        space.map_element(crate::space_item::SpaceItem::Window(window), window_loc, false);
    }
    Some(())
}

/// Mesma altura do header em decoration.rs — passada como parâmetro
/// pra evitar dep cruzada.
const HEADER_HEIGHT_HINT: i32 = 24;

/// Hit-test pra resize. Cobre as 4 bordas e os 4 cantos. Retorna `None`
/// se está mais pra dentro (área do conteúdo) ou fora da janela.
pub fn hit_test_resize_edge(
    point_in_window: Point<f64, Logical>,
    window_size: Size<i32, Logical>,
) -> Option<ResizeEdge> {
    // Dentro da área do header (parte superior) NÃO entra a zona de
    // resize — senão os botões fechar/maximizar/minimizar ficam
    // sobrepostos com a borda de resize do canto superior-direito e
    // o click vai pro grab em vez do botão.
    if point_in_window.y < HEADER_HEIGHT_HINT as f64 {
        return None;
    }
    let near_left = point_in_window.x >= 0.0 && point_in_window.x < RESIZE_EDGE_THICKNESS as f64;
    let near_right = point_in_window.x >= (window_size.w - RESIZE_EDGE_THICKNESS) as f64
        && point_in_window.x <= window_size.w as f64;
    let near_top = point_in_window.y >= 0.0 && point_in_window.y < RESIZE_EDGE_THICKNESS as f64;
    let near_bottom = point_in_window.y >= (window_size.h - RESIZE_EDGE_THICKNESS) as f64
        && point_in_window.y <= window_size.h as f64;

    let mut edges = ResizeEdge::empty();
    if near_left {
        edges |= ResizeEdge::LEFT;
    }
    if near_right {
        edges |= ResizeEdge::RIGHT;
    }
    if near_top {
        edges |= ResizeEdge::TOP;
    }
    if near_bottom {
        edges |= ResizeEdge::BOTTOM;
    }

    if edges.is_empty() {
        None
    } else {
        Some(edges)
    }
}
