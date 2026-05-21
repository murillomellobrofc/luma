use smithay::{
    backend::renderer::{
        element::{memory::MemoryRenderBufferRenderElement, AsRenderElements, Kind},
        ImportAll, ImportMem, Renderer, Texture,
    },
    desktop::{space::SpaceElement, Window},
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale, Size},
};

use crate::{
    decoration::{
        header_elements_at, title_data, window_control_elements_at,
        BORDER_THICKNESS, HEADER_HEIGHT,
    },
    render::LumaRenderElement,
};

/// Wrapper sobre `smithay::desktop::Window` que inclui a barra SSD do
/// compositor como parte do conteúdo da janela. Faz com que o z-order
/// natural do `Space` se aplique também à decoração — sem isso, headers
/// renderizavam acima de todas as surfaces como custom elements,
/// causando o header de uma janela atrás aparecer sobre o conteúdo da
/// janela na frente.
#[derive(Debug, Clone, PartialEq)]
pub struct LumaWindow(pub Window);

impl LumaWindow {
    pub fn new(window: Window) -> Self {
        Self(window)
    }

    pub fn window(&self) -> &Window {
        &self.0
    }

    pub fn wl_surface(&self) -> Option<WlSurface> {
        self.0.toplevel().map(|t| t.wl_surface().clone())
    }
}

impl IsAlive for LumaWindow {
    fn alive(&self) -> bool {
        self.0.alive()
    }
}

impl SpaceElement for LumaWindow {
    fn bbox(&self) -> Rectangle<i32, Logical> {
        if crate::decoration::is_minimized(&self.0) {
            return Rectangle::default();
        }
        let mut bbox = SpaceElement::bbox(&self.0);
        bbox.size.w += 2 * BORDER_THICKNESS;
        bbox.size.h += HEADER_HEIGHT + BORDER_THICKNESS;
        bbox
    }

    fn geometry(&self) -> Rectangle<i32, Logical> {
        if crate::decoration::is_minimized(&self.0) {
            return Rectangle::default();
        }
        let mut geo = SpaceElement::geometry(&self.0);
        geo.size.w += 2 * BORDER_THICKNESS;
        geo.size.h += HEADER_HEIGHT + BORDER_THICKNESS;
        geo
    }

    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        if crate::decoration::is_minimized(&self.0) {
            return false;
        }
        // Header inteiro (incluindo as áreas das bordas laterais que se
        // alinham com o header em Y).
        if point.y < HEADER_HEIGHT as f64 {
            return true;
        }
        // Borda lateral / inferior: input do compositor, não do cliente.
        if point.x < BORDER_THICKNESS as f64
            || point.x >= (SpaceElement::geometry(&self.0).size.w + BORDER_THICKNESS) as f64
        {
            return true;
        }
        let shifted = *point
            - Point::from((BORDER_THICKNESS as f64, HEADER_HEIGHT as f64));
        SpaceElement::is_in_input_region(&self.0, &shifted)
    }

    fn set_activate(&self, activated: bool) {
        SpaceElement::set_activate(&self.0, activated);
    }

    fn output_enter(&self, output: &Output, overlap: Rectangle<i32, Logical>) {
        SpaceElement::output_enter(&self.0, output, overlap);
    }

    fn output_leave(&self, output: &Output) {
        SpaceElement::output_leave(&self.0, output);
    }

    fn refresh(&self) {
        SpaceElement::refresh(&self.0);
    }

    fn z_index(&self) -> u8 {
        SpaceElement::z_index(&self.0)
    }
}

impl<R> AsRenderElements<R> for LumaWindow
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
        if crate::decoration::is_minimized(&self.0) {
            return Vec::new();
        }
        let content_size = SpaceElement::geometry(&self.0).size;
        let full_width_logical = content_size.w + 2 * BORDER_THICKNESS;

        // Header cobre toda a largura (incluindo as 3px laterais que
        // continuam reservadas pra hit-test de resize mas SEM render).
        let header = header_elements_at(&self.0, location, full_width_logical);
        // H_deco.1: bordas L/R/Bottom NÃO renderizadas mais — visual
        // limpo, sem o contorno laranja. BORDER_THICKNESS=3 continua só
        // pra geometry/input/resize hit-test.

        // Título PRIMEIRO no vec — z-order do render_output é "primeiro no
        // array = topo da pilha". Se header for antes, ele tampa o título.
        // H32.6.5: padding-left vem de theme.window.title_padding_left;
        // alpha modula pela flag focused (multiplicador window_title_unfocused_alpha).
        let theme = crate::theme::current();
        let title_pad = theme.window.title_padding_left;
        let focused = crate::decoration::is_focused(&self.0);
        let title_alpha = if focused {
            None
        } else {
            Some(theme.colors.window_title_unfocused_alpha)
        };
        let mut out: Vec<C> = Vec::new();
        if let Some((title_buffer, title_size)) = title_data(&self.0) {
            let title_x = location.x
                + (scale.x * (BORDER_THICKNESS + title_pad) as f64) as i32;
            let header_h_phys = (scale.y * HEADER_HEIGHT as f64) as i32;
            let title_h_phys = (scale.y * title_size.h as f64) as i32;
            let title_y = location.y + ((header_h_phys - title_h_phys) / 2).max(0);
            let title_loc = Point::<i32, Physical>::from((title_x, title_y));
            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                title_loc.to_f64(),
                &title_buffer,
                title_alpha,
                None,
                None,
                Kind::Unspecified,
            ) {
                out.push(C::from(LumaRenderElement::Memory(elem)));
            }
        }
        // Window controls Luma original (H32.5.12/.13): ícones puros +
        // squircle colorido no hover. Layout vem do theme.window_controls.
        let _ = scale; // window_control_elements_at usa logical=physical do header
        let hover = crate::decoration::hovered_region(&self.0);
        for elem in window_control_elements_at(renderer, location, full_width_logical, hover) {
            out.push(C::from(LumaRenderElement::Memory(elem)));
        }

        out.extend(
            header
                .into_iter()
                .map(|e| C::from(LumaRenderElement::Solid(e))),
        );

        // H_deco.2-3: drop shadow 9-slice atrás da janela. Push DEPOIS
        // dos solids = atrás no z-stack (Smithay renderiza fim→começo).
        let full_size = Size::<i32, Logical>::from((
            full_width_logical,
            HEADER_HEIGHT + content_size.h + BORDER_THICKNESS,
        ));
        for elem in crate::window_shadow::shadow_elements_at(
            renderer,
            location,
            full_size,
            focused,
        ) {
            out.push(C::from(LumaRenderElement::Memory(elem)));
        }

        // Surface fica deslocada pela borda esquerda + header.
        let surface_loc = Point::<i32, Physical>::from((
            location.x + (scale.x * BORDER_THICKNESS as f64) as i32,
            location.y + (scale.y * HEADER_HEIGHT as f64) as i32,
        ));
        let surface_parts: Vec<LumaRenderElement<R>> =
            AsRenderElements::<R>::render_elements::<LumaRenderElement<R>>(
                &self.0,
                renderer,
                surface_loc,
                scale,
                alpha,
            );
        out.extend(surface_parts.into_iter().map(C::from));
        out
    }
}
