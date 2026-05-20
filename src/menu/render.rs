//! Render do menu fullscreen híbrido. H32.10.
//!
//! Pinta o backdrop sobre o output inteiro (cobre wallpaper, janelas
//! e taskbar) + a linha âmbar 2px no topo pra continuidade visual.
//!
//! Bloco A (H32.10.2): só backdrop + accent line. Conteúdo das 5
//! zonas (greeting/search/grid/session) entra nos commits seguintes.

use smithay::{
    backend::renderer::{element::solid::SolidColorBuffer, Color32F},
    utils::{Logical, Point, Rectangle, Size},
};

// Wireframe placeholders cinza translúcido pras 5 zonas do H32.10.4.
// Serão substituídos por conteúdo real nos commits seguintes.
const PLACEHOLDER: Color32F = Color32F::new(0.5, 0.5, 0.5, 0.15);

/// Margens/tamanhos das 5 zonas (espelha o brief H32.10).
pub mod layout {
    use super::*;

    /// Greeting corner (sup dir): largura, altura, padding top/right.
    pub const GREETING_W: i32 = 280;
    pub const GREETING_H: i32 = 80;
    pub const GREETING_PAD_TOP: i32 = 40;
    pub const GREETING_PAD_RIGHT: i32 = 60;

    /// Search bar (centro): pill 64×500 com margin_top do topo.
    pub const SEARCH_W: i32 = 500;
    pub const SEARCH_H: i32 = 64;
    pub const SEARCH_MARGIN_TOP: i32 = 130;

    /// Grid de favoritos: 5 cols × 2 rows × 80px + gaps 60px + label 32.
    /// Total ≈ 5*80 + 4*60 = 640 wide, 2*80 + 60 + 2*32 = 284 alto.
    pub const FAV_GRID_W: i32 = 640;
    pub const FAV_GRID_H: i32 = 284;
    pub const FAV_GRID_MARGIN_TOP: i32 = 240;

    /// Link "TODOS OS APPS →": entre grid e session bar.
    pub const ALL_APPS_LINK_W: i32 = 280;
    pub const ALL_APPS_LINK_H: i32 = 40;
    pub const ALL_APPS_LINK_MARGIN_TOP: i32 = 30;

    /// Session bar: full width menos margem, embaixo.
    pub const SESSION_H: i32 = 60;
    pub const SESSION_MARGIN_BOTTOM: i32 = 40;
    pub const SESSION_MARGIN_HORIZONTAL: i32 = 40;

    /// Calcula bounding rects das 5 zonas dado o output_size.
    ///
    /// Layout horizontal: zonas centralizadas no output (greeting anchora
    /// no canto sup-dir, session bar full-width menos margens).
    ///
    /// Layout vertical (responsivo até 4K): greeting fica fixo no topo,
    /// session bar fixa no bottom, e o conjunto **search + grid + link**
    /// é centralizado verticalmente no espaço entre os dois. Em telas
    /// pequenas (~600px) o conjunto encosta nas margens; em 4K (2160px)
    /// fica centralizado com bastante respiro.
    pub fn zones(output_size: Size<i32, Logical>) -> [Rectangle<i32, Logical>; 5] {
        let cx = output_size.w / 2;

        // 1) Greeting canto sup-dir.
        let greeting = Rectangle::new(
            Point::from((
                output_size.w - GREETING_W - GREETING_PAD_RIGHT,
                GREETING_PAD_TOP,
            )),
            Size::from((GREETING_W, GREETING_H)),
        );

        // 5) Session bar embaixo (calculada antes pra delimitar o
        // centro vertical).
        let session_w = output_size.w - 2 * SESSION_MARGIN_HORIZONTAL;
        let session_y = output_size.h - SESSION_MARGIN_BOTTOM - SESSION_H;
        let session = Rectangle::new(
            Point::from((SESSION_MARGIN_HORIZONTAL, session_y)),
            Size::from((session_w, SESSION_H)),
        );

        // Conjunto search + grid + link centralizado vertical entre:
        // - topo livre = greeting bottom + padding (~140px)
        // - base livre = session top - padding (~40px)
        let stack_top_min = GREETING_PAD_TOP + GREETING_H + 20;
        let stack_bottom_max = session_y - 40;
        let stack_total_h = SEARCH_H + 60 + FAV_GRID_H
            + ALL_APPS_LINK_MARGIN_TOP + ALL_APPS_LINK_H;
        // Centro vertical disponível.
        let avail_center = (stack_top_min + stack_bottom_max) / 2;
        let stack_top = (avail_center - stack_total_h / 2).max(stack_top_min);

        // 2) Search bar centro horizontal, top do stack.
        let search = Rectangle::new(
            Point::from((cx - SEARCH_W / 2, stack_top)),
            Size::from((SEARCH_W, SEARCH_H)),
        );

        // 3) Favoritos grid abaixo da search (gap 60).
        let fav_y = search.loc.y + search.size.h + 60;
        let fav = Rectangle::new(
            Point::from((cx - FAV_GRID_W / 2, fav_y)),
            Size::from((FAV_GRID_W, FAV_GRID_H)),
        );

        // 4) Link "TODOS OS APPS →" abaixo do grid.
        let link_y = fav.loc.y + fav.size.h + ALL_APPS_LINK_MARGIN_TOP;
        let link = Rectangle::new(
            Point::from((cx - ALL_APPS_LINK_W / 2, link_y)),
            Size::from((ALL_APPS_LINK_W, ALL_APPS_LINK_H)),
        );

        [greeting, search, fav, link, session]
    }
}

/// Buffers pré-renderizados da camada fullscreen do menu novo.
/// Construído 1x no init do `MenuState`, reusa SolidColorBuffer.
#[derive(Debug)]
pub struct FullscreenLayer {
    /// Overlay Coffee escuro alpha 0.94 que cobre o output inteiro.
    /// `resize` no render pra acompanhar tamanho do output.
    pub backdrop: SolidColorBuffer,
    /// Linha âmbar 2px no topo. Largura redimensionada no render.
    pub top_accent: SolidColorBuffer,
    /// Wireframe (H32.10.4): 5 placeholders cinza translúcido pras
    /// zonas. Cada um vai ser substituído pelo conteúdo real:
    /// [greeting, search, fav_grid, all_apps_link, session_bar].
    /// Vão sumir conforme conteúdo real entra (~H32.10.9, H32.10.13).
    pub zone_placeholders: [SolidColorBuffer; 5],
}

impl FullscreenLayer {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let f = &theme.menu_fullscreen;
        let backdrop_color = smithay::backend::renderer::Color32F::new(
            f.backdrop_color.r(),
            f.backdrop_color.g(),
            f.backdrop_color.b(),
            f.backdrop_alpha,
        );
        let accent_color = smithay::backend::renderer::Color32F::new(
            f.top_accent_color.r(),
            f.top_accent_color.g(),
            f.top_accent_color.b(),
            f.top_accent_alpha,
        );
        Self {
            // Tamanho inicial 1×1 — `resize` chamado no winit
            // backend a cada frame com o output atual.
            backdrop: SolidColorBuffer::new((1, 1), backdrop_color),
            top_accent: SolidColorBuffer::new((1, 1), accent_color),
            zone_placeholders: std::array::from_fn(|_| {
                SolidColorBuffer::new((1, 1), PLACEHOLDER)
            }),
        }
    }

    /// Ajusta os buffers pro tamanho do output atual + redimensiona as
    /// 5 zonas do wireframe conforme `layout::zones()`.
    pub fn resize(&mut self, output_size: Size<i32, Logical>) {
        let h = crate::theme::current().menu_fullscreen.top_accent_height;
        self.backdrop.resize((output_size.w, output_size.h));
        self.top_accent.resize((output_size.w, h));
        let zones = layout::zones(output_size);
        for (buf, rect) in self.zone_placeholders.iter_mut().zip(zones.iter()) {
            buf.resize((rect.size.w, rect.size.h));
        }
    }

    /// Retorna o `Point` (top-left logical) da zona N pra o caller
    /// posicionar no render. Mesmo índice de `zone_placeholders`.
    pub fn zone_origin(
        &self,
        output_size: Size<i32, Logical>,
        zone: usize,
    ) -> Point<i32, Logical> {
        layout::zones(output_size)[zone].loc
    }
}

impl Default for FullscreenLayer {
    fn default() -> Self {
        Self::new()
    }
}
