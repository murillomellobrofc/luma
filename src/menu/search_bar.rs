//! Search bar pill do menu fullscreen híbrido. H32.10.7.
//!
//! 500×64 corner-radius 32 (pill). Auto-focus na abertura do menu —
//! qualquer letra digitada filtra direto. Chip "⌘ K" à direita
//! reforça o atalho.
//!
//! Cores variam por estado (focused vs default): bg, border alpha e
//! cor da lupa. Por enquanto a versão focused é a única renderizada
//! (FocusedZone::Search é o default da abertura).

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::Size,
};

type Label = (MemoryRenderBuffer, Size<i32, smithay::utils::Logical>);

/// Buffers pré-renderizados da search bar.
#[derive(Debug)]
pub struct SearchBar {
    /// Background pill com cantos arredondados. Cor varia por estado;
    /// por enquanto rasterizamos só o estado focado (default ao abrir).
    pub bg_focused: MemoryRenderBuffer,
    /// Pill âmbar maior usado como borda quando hover do mouse na bar.
    /// width+border_width*2 × height+border_width*2.
    pub border_pill: MemoryRenderBuffer,
    /// Ícone lupa 20×20 cor âmbar (estado focado).
    pub magnifier: MemoryRenderBuffer,
    /// Placeholder "Buscar apps..." em cinza 18px alpha 0.6.
    pub placeholder: Option<Label>,
    /// Label do texto digitado (cream 18px), regenerado a cada mudança.
    pub query_label: Option<Label>,
    last_query: String,
    /// Chip "Super K" à direita: bg dark + label âmbar.
    pub chip_bg: MemoryRenderBuffer,
    pub chip_label: Option<Label>,
    /// Cursor "|" rasterizado em âmbar 18px. Aparece após o texto
    /// digitado quando focused_zone == Search. Piscar implementado
    /// via alpha modulation no render (Instant elapsed % 1000ms).
    pub cursor_label: Option<Label>,
}

impl SearchBar {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let s = &theme.menu_search_bar;

        let bg_focused = crate::gfx::make_rounded_panel(
            s.width, s.height, s.corner_radius, s.bg_focused,
        );
        // Borda âmbar = pill maior com cor âmbar alpha alto. Render
        // posiciona em (-border_width, -border_width) atrás do bg.
        let bw = s.border_width;
        let border_color = smithay::backend::renderer::Color32F::new(
            s.border_color.r(), s.border_color.g(), s.border_color.b(),
            s.border_alpha_focused,
        );
        let border_pill = crate::gfx::make_rounded_panel(
            s.width + 2 * bw,
            s.height + 2 * bw,
            s.corner_radius + bw,
            border_color,
        );
        let magnifier = crate::gfx::make_search_icon(s.icon_size, s.icon_color_focused);
        let chip_bg = crate::gfx::make_rounded_panel(
            s.chip_width, s.chip_height, 4, s.chip_bg,
        );

        Self {
            bg_focused,
            border_pill,
            magnifier,
            placeholder: None,
            query_label: None,
            last_query: String::new(),
            chip_bg,
            chip_label: None,
            cursor_label: None,
        }
    }

    /// Rasteriza o texto digitado se query mudou. Vazio = limpa cache.
    pub fn update_query(
        &mut self,
        text: &crate::text::TextRenderer,
        query: &str,
    ) {
        if self.last_query == query {
            return;
        }
        self.last_query = query.to_string();
        if query.is_empty() {
            self.query_label = None;
            return;
        }
        let theme = crate::theme::current();
        let s = &theme.menu_search_bar;
        self.query_label = rasterize_colored(text, query, s.text_size, s.text_color);
    }

    /// Rasteriza placeholder + chip label 1x (lazy).
    pub fn ensure_labels(&mut self, text: &crate::text::TextRenderer) {
        let theme = crate::theme::current();
        let s = &theme.menu_search_bar;

        if self.placeholder.is_none() {
            let mut color = s.placeholder_color;
            color = smithay::backend::renderer::Color32F::new(
                color.r(), color.g(), color.b(), s.placeholder_alpha,
            );
            self.placeholder = rasterize_colored(text, &s.placeholder_text, s.text_size, color);
        }
        if self.chip_label.is_none() {
            self.chip_label = rasterize_colored(
                text, &s.chip_label, s.chip_text_size, s.chip_color,
            );
        }
        if self.cursor_label.is_none() {
            self.cursor_label = rasterize_colored(
                text, "|", s.text_size, s.border_color, // âmbar
            );
        }
    }
}

impl Default for SearchBar {
    fn default() -> Self {
        Self::new()
    }
}

fn rasterize_colored(
    text: &crate::text::TextRenderer,
    s: &str,
    size: f32,
    color: smithay::backend::renderer::Color32F,
) -> Option<Label> {
    let (pixels, w, h) = text.rasterize_colored(s, size, color);
    if w == 0 || h == 0 {
        return None;
    }
    let buffer = MemoryRenderBuffer::from_slice(
        &pixels,
        smithay::backend::allocator::Fourcc::Argb8888,
        (w as i32, h as i32),
        1,
        smithay::utils::Transform::Normal,
        None,
    );
    Some((buffer, Size::from((w as i32, h as i32))))
}
