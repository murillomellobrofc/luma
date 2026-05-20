//! Link "TODOS OS APPS →" + hint embaixo do grid de favoritos. H32.10.13.
//!
//! Único elemento da zone[3] do wireframe. Centralizado horizontal.
//! Quando mode=AllApps, esconde (já estamos no grid).

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Size},
};

type Label = (MemoryRenderBuffer, Size<i32, Logical>);

#[derive(Debug, Default)]
pub struct AllAppsLink {
    pub label: Option<Label>,
    pub hint: Option<Label>,
    /// Breadcrumb "← Favoritos" que aparece no mesmo lugar quando
    /// mode=AllApps. Click volta pra Favoritos.
    pub back_label: Option<Label>,
    pub back_hint: Option<Label>,
    /// Bg sutil pro estado hover do link (lazy init no render).
    pub hover_bg: Option<MemoryRenderBuffer>,
}

impl AllAppsLink {
    pub fn ensure_labels(&mut self, text: &crate::text::TextRenderer) {
        let t = &crate::theme::current().menu_all_apps_link;
        if self.label.is_none() {
            self.label = rasterize_colored(text, &t.label_text, t.label_size, t.label_color);
        }
        if self.hint.is_none() {
            self.hint = rasterize_colored(text, &t.hint_text, t.hint_size, t.hint_color);
        }
        if self.back_label.is_none() {
            // ASCII-safe: ← não rasteriza no font atual (vira □).
            self.back_label = rasterize_colored(
                text, "FAVORITOS", t.label_size, t.label_color,
            );
        }
        if self.back_hint.is_none() {
            self.back_hint = rasterize_colored(
                text, "clique ou pressione Tab", t.hint_size, t.hint_color,
            );
        }
        if self.hover_bg.is_none() {
            // Bg sutil amber alpha baixo, full width do link rect.
            let w = crate::menu::render::layout::ALL_APPS_LINK_W;
            let h = crate::menu::render::layout::ALL_APPS_LINK_H;
            let amber_subtle = smithay::backend::renderer::Color32F::new(
                0xE8 as f32 / 255.0,
                0x9B as f32 / 255.0,
                0x3D as f32 / 255.0,
                0.18,
            );
            self.hover_bg = Some(crate::gfx::make_rounded_panel(w, h, 8, amber_subtle));
        }
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
