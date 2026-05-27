use std::sync::{Arc, Mutex};
use std::time::Instant;

use smithay::{
    backend::renderer::{
        element::{
            memory::MemoryRenderBufferRenderElement, AsRenderElements, Kind,
        },
        ImportAll, ImportMem, Renderer, Texture,
    },
    desktop::space::SpaceElement,
    output::Output,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale, Size},
};

use crate::{render::LumaRenderElement, wallpaper::Wallpaper, window::LumaWindow};

/// Item de widget mapeado no desktop. Z-index 5 (mesma camada dos
/// ícones, acima do wallpaper, atrás das janelas).
///
/// O widget é mutado pelo loop de tick do redraw handler — daí o
/// Mutex. last_tick segue paralelo ao Vec<Instant> do painel, só que
/// per-widget porque cada DesktopWidget vive sozinho.
///
/// Posição (x, y lógicos, origem canto superior-esquerdo do output
/// principal) NÃO é guardada aqui — fica no Space, lida via
/// `space.element_location(item)`. Multi-monitor fica pra depois.
///
/// `id` é o identificador estável da entrada [[widget]] no
/// layout.toml. O modo de edição usa esse id pra achar a entrada
/// certa na hora de salvar x/y ou reordenar, sem depender de
/// posição (que muda quando o arquivo é reescrito).
pub struct DesktopWidgetItem {
    pub widget: Mutex<Box<dyn luma_widget::Widget + Send>>,
    pub last_tick: Mutex<Instant>,
    pub id: String,
    /// Tamanho custom do modo de edição. None = natural. Set pelo
    /// cartão "Redimensionar" e persistido em placement.width/height.
    pub user_size: Mutex<Option<(u32, u32)>>,
}

impl DesktopWidgetItem {
    pub fn new(
        widget: Box<dyn luma_widget::Widget + Send>,
        id: String,
    ) -> Self {
        Self {
            widget: Mutex::new(widget),
            last_tick: Mutex::new(Instant::now()),
            id,
            user_size: Mutex::new(None),
        }
    }

    pub fn new_with_size(
        widget: Box<dyn luma_widget::Widget + Send>,
        id: String,
        size: Option<(u32, u32)>,
    ) -> Self {
        Self {
            widget: Mutex::new(widget),
            last_tick: Mutex::new(Instant::now()),
            id,
            user_size: Mutex::new(size),
        }
    }
}

/// Estado de drag de um widget de desktop. Vive em
/// `LumaState::desktop_widget_drag` enquanto o botão está pressionado
/// dentro do modo de edição. Threshold pequeno antes de ativar pra
/// não promover click sem intenção de mover.
pub struct DesktopWidgetDrag {
    /// Widget sendo arrastado. Arc é cheap clone — o real continua
    /// no Space.
    pub item: std::sync::Arc<DesktopWidgetItem>,
    /// Offset do cursor pra origem do widget no momento do press.
    /// Mantém o widget firme sob o cursor (não pula pra origem).
    pub offset: smithay::utils::Point<i32, smithay::utils::Logical>,
    /// Posição inicial do cursor (screen) — usada pra threshold.
    pub start_screen: smithay::utils::Point<f64, smithay::utils::Logical>,
    /// Posição atual do cursor (screen) — atualizada por motion.
    pub current_screen: smithay::utils::Point<f64, smithay::utils::Logical>,
    /// true quando o drag passou do threshold e está mexendo de fato.
    /// Click curto sem mover = ativa nunca, no-op.
    pub active: bool,
}

/// Item do `Space` do Luma. Wallpaper e janelas convivem no mesmo Space
/// porque `Space<E>` exige um único tipo E — este enum faz dispatch
/// dinâmico pras impls de `SpaceElement` + `AsRenderElements` de cada
/// variante. Wallpaper tem `z_index = 0` (fundo); LumaWindow herda o
/// z_index padrão do smithay Window (10 = Shell), então fica acima do
/// wallpaper automaticamente.
pub enum SpaceItem {
    Window(LumaWindow),
    Wallpaper(Arc<Wallpaper>),
    /// Camada de ícones do desktop (z_index=5, entre wallpaper e
    /// janelas). H50.
    DesktopIcons(Arc<crate::desktop_icons::DesktopIconsLayer>),
    /// Widget de desktop (fatia 4 widgets). Mesmo z=5 dos ícones —
    /// atrás das janelas, plasmoid-style. Position é por-item; o
    /// Space mapeia em (x, y).
    DesktopWidget(Arc<DesktopWidgetItem>),
}

impl SpaceItem {
    pub fn as_window(&self) -> Option<&LumaWindow> {
        match self {
            Self::Window(w) => Some(w),
            _ => None,
        }
    }
}

impl SpaceItem {
    fn icons_layer(&self) -> Option<&crate::desktop_icons::DesktopIconsLayer> {
        match self {
            Self::DesktopIcons(l) => Some(l.as_ref()),
            _ => None,
        }
    }
}

impl std::fmt::Debug for SpaceItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Window(_) => f.debug_struct("SpaceItem::Window").finish(),
            Self::Wallpaper(_) => f.debug_struct("SpaceItem::Wallpaper").finish(),
            Self::DesktopIcons(_) => f.debug_struct("SpaceItem::DesktopIcons").finish(),
            Self::DesktopWidget(_) => f.debug_struct("SpaceItem::DesktopWidget").finish(),
        }
    }
}

impl Clone for SpaceItem {
    fn clone(&self) -> Self {
        match self {
            Self::Window(w) => Self::Window(w.clone()),
            Self::Wallpaper(w) => Self::Wallpaper(Arc::clone(w)),
            Self::DesktopIcons(l) => Self::DesktopIcons(Arc::clone(l)),
            Self::DesktopWidget(w) => Self::DesktopWidget(Arc::clone(w)),
        }
    }
}

impl PartialEq for SpaceItem {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Window(a), Self::Window(b)) => a == b,
            (Self::Wallpaper(a), Self::Wallpaper(b)) => Arc::ptr_eq(a, b),
            (Self::DesktopIcons(a), Self::DesktopIcons(b)) => Arc::ptr_eq(a, b),
            (Self::DesktopWidget(a), Self::DesktopWidget(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl IsAlive for SpaceItem {
    fn alive(&self) -> bool {
        match self {
            Self::Window(w) => IsAlive::alive(w),
            Self::Wallpaper(_) => true,
            Self::DesktopIcons(l) => IsAlive::alive(l.as_ref()),
            Self::DesktopWidget(_) => true,
        }
    }
}

impl SpaceElement for SpaceItem {
    fn bbox(&self) -> Rectangle<i32, Logical> {
        match self {
            Self::Window(w) => SpaceElement::bbox(w),
            Self::Wallpaper(w) => SpaceElement::bbox(w.as_ref()),
            Self::DesktopIcons(l) => SpaceElement::bbox(l.as_ref()),
            // bbox de tamanho fixo razoável — o widget renderiza
            // num buffer próprio cujo tamanho varia, mas pra fins
            // de damage/z-order esse rect é suficiente. POC.
            Self::DesktopWidget(_) => Rectangle::from_size(Size::from((220, 50))),
        }
    }

    fn geometry(&self) -> Rectangle<i32, Logical> {
        match self {
            Self::Window(w) => SpaceElement::geometry(w),
            Self::Wallpaper(w) => SpaceElement::geometry(w.as_ref()),
            Self::DesktopIcons(l) => SpaceElement::geometry(l.as_ref()),
            Self::DesktopWidget(_) => Rectangle::from_size(Size::from((220, 50))),
        }
    }

    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        match self {
            Self::Window(w) => SpaceElement::is_in_input_region(w, point),
            Self::Wallpaper(w) => SpaceElement::is_in_input_region(w.as_ref(), point),
            Self::DesktopIcons(l) => SpaceElement::is_in_input_region(l.as_ref(), point),
            // H58.4 fatia 4: widget de desktop é read-only por enquanto.
            // Fora de input region → não captura clique.
            Self::DesktopWidget(_) => false,
        }
    }

    fn set_activate(&self, activated: bool) {
        match self {
            Self::Window(w) => SpaceElement::set_activate(w, activated),
            Self::Wallpaper(w) => SpaceElement::set_activate(w.as_ref(), activated),
            Self::DesktopIcons(l) => SpaceElement::set_activate(l.as_ref(), activated),
            Self::DesktopWidget(_) => {}
        }
    }

    fn output_enter(&self, output: &Output, overlap: Rectangle<i32, Logical>) {
        match self {
            Self::Window(w) => SpaceElement::output_enter(w, output, overlap),
            Self::Wallpaper(w) => SpaceElement::output_enter(w.as_ref(), output, overlap),
            Self::DesktopIcons(l) => SpaceElement::output_enter(l.as_ref(), output, overlap),
            Self::DesktopWidget(_) => {}
        }
    }

    fn output_leave(&self, output: &Output) {
        match self {
            Self::Window(w) => SpaceElement::output_leave(w, output),
            Self::Wallpaper(w) => SpaceElement::output_leave(w.as_ref(), output),
            Self::DesktopIcons(l) => SpaceElement::output_leave(l.as_ref(), output),
            Self::DesktopWidget(_) => {}
        }
    }

    fn refresh(&self) {
        match self {
            Self::Window(w) => SpaceElement::refresh(w),
            Self::Wallpaper(w) => SpaceElement::refresh(w.as_ref()),
            Self::DesktopIcons(l) => SpaceElement::refresh(l.as_ref()),
            Self::DesktopWidget(_) => {}
        }
    }

    fn z_index(&self) -> u8 {
        match self {
            Self::Window(w) => SpaceElement::z_index(w),
            Self::Wallpaper(w) => SpaceElement::z_index(w.as_ref()),
            Self::DesktopIcons(l) => SpaceElement::z_index(l.as_ref()),
            // Mesma camada dos ícones: acima do wallpaper, atrás das
            // janelas. Empate com ícones desempatado por ordem de
            // inserção (spec aceita).
            Self::DesktopWidget(_) => 5,
        }
    }
}

impl<R> AsRenderElements<R> for SpaceItem
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Texture + Clone + Send + 'static,
{
    type RenderElement = LumaRenderElement<R>;

    fn render_elements<C: From<LumaRenderElement<R>>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<C> {
        match self {
            Self::Window(w) => {
                AsRenderElements::<R>::render_elements::<C>(w, renderer, location, scale, alpha)
            }
            Self::Wallpaper(w) => AsRenderElements::<R>::render_elements::<C>(
                w.as_ref(),
                renderer,
                location,
                scale,
                alpha,
            ),
            Self::DesktopIcons(l) => AsRenderElements::<R>::render_elements::<C>(
                l.as_ref(),
                renderer,
                location,
                scale,
                alpha,
            ),
            Self::DesktopWidget(dw) => {
                // Render do widget é stateless do nosso lado — lock,
                // render(ctx), wrap no MemoryRenderBufferRenderElement
                // no location que o Space nos passou (já contém o
                // x/y mapeado, em coords Physical).
                let _ = alpha;
                let us = dw.user_size.lock().ok().and_then(|g| *g);
                let ctx = luma_widget::RenderCtx {
                    host: luma_widget::WidgetHost::Desktop,
                    theme: crate::theme::current(),
                    user_size: us,
                };
                let buffer = {
                    let w = dw.widget.lock().expect("widget poisoned");
                    w.render(&ctx)
                };
                match MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    location.to_f64(),
                    &buffer,
                    None, None, None,
                    Kind::Unspecified,
                ) {
                    Ok(elem) => vec![LumaRenderElement::Memory(elem).into()],
                    Err(_) => Vec::new(),
                }
            }
        }
    }
}
