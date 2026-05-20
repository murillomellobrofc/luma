use std::sync::Arc;

use smithay::{
    backend::renderer::{element::AsRenderElements, ImportAll, ImportMem, Renderer, Texture},
    desktop::space::SpaceElement,
    output::Output,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale},
};

use crate::{render::LumaRenderElement, wallpaper::Wallpaper, window::LumaWindow};

/// Item do `Space` do Luma. Wallpaper e janelas convivem no mesmo Space
/// porque `Space<E>` exige um único tipo E — este enum faz dispatch
/// dinâmico pras impls de `SpaceElement` + `AsRenderElements` de cada
/// variante. Wallpaper tem `z_index = 0` (fundo); LumaWindow herda o
/// z_index padrão do smithay Window (10 = Shell), então fica acima do
/// wallpaper automaticamente.
pub enum SpaceItem {
    Window(LumaWindow),
    Wallpaper(Arc<Wallpaper>),
}

impl SpaceItem {
    pub fn as_window(&self) -> Option<&LumaWindow> {
        match self {
            Self::Window(w) => Some(w),
            _ => None,
        }
    }
}

impl std::fmt::Debug for SpaceItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Window(_) => f.debug_struct("SpaceItem::Window").finish(),
            Self::Wallpaper(_) => f.debug_struct("SpaceItem::Wallpaper").finish(),
        }
    }
}

impl Clone for SpaceItem {
    fn clone(&self) -> Self {
        match self {
            Self::Window(w) => Self::Window(w.clone()),
            Self::Wallpaper(w) => Self::Wallpaper(Arc::clone(w)),
        }
    }
}

impl PartialEq for SpaceItem {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Window(a), Self::Window(b)) => a == b,
            (Self::Wallpaper(a), Self::Wallpaper(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl IsAlive for SpaceItem {
    fn alive(&self) -> bool {
        match self {
            Self::Window(w) => IsAlive::alive(w),
            Self::Wallpaper(_) => true,
        }
    }
}

impl SpaceElement for SpaceItem {
    fn bbox(&self) -> Rectangle<i32, Logical> {
        match self {
            Self::Window(w) => SpaceElement::bbox(w),
            Self::Wallpaper(w) => SpaceElement::bbox(w.as_ref()),
        }
    }

    fn geometry(&self) -> Rectangle<i32, Logical> {
        match self {
            Self::Window(w) => SpaceElement::geometry(w),
            Self::Wallpaper(w) => SpaceElement::geometry(w.as_ref()),
        }
    }

    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        match self {
            Self::Window(w) => SpaceElement::is_in_input_region(w, point),
            Self::Wallpaper(w) => SpaceElement::is_in_input_region(w.as_ref(), point),
        }
    }

    fn set_activate(&self, activated: bool) {
        match self {
            Self::Window(w) => SpaceElement::set_activate(w, activated),
            Self::Wallpaper(w) => SpaceElement::set_activate(w.as_ref(), activated),
        }
    }

    fn output_enter(&self, output: &Output, overlap: Rectangle<i32, Logical>) {
        match self {
            Self::Window(w) => SpaceElement::output_enter(w, output, overlap),
            Self::Wallpaper(w) => SpaceElement::output_enter(w.as_ref(), output, overlap),
        }
    }

    fn output_leave(&self, output: &Output) {
        match self {
            Self::Window(w) => SpaceElement::output_leave(w, output),
            Self::Wallpaper(w) => SpaceElement::output_leave(w.as_ref(), output),
        }
    }

    fn refresh(&self) {
        match self {
            Self::Window(w) => SpaceElement::refresh(w),
            Self::Wallpaper(w) => SpaceElement::refresh(w.as_ref()),
        }
    }

    fn z_index(&self) -> u8 {
        match self {
            Self::Window(w) => SpaceElement::z_index(w),
            Self::Wallpaper(w) => SpaceElement::z_index(w.as_ref()),
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
        }
    }
}
