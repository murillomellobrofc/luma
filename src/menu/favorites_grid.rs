//! Grid de favoritos curados 5×2 do menu fullscreen. H32.10.8.
//!
//! Renderiza os 10 slots de `state::CuratedFavorite` como quadrados
//! coloridos 80×80 (canto 18) com label embaixo. A cor vem do TOML,
//! NÃO do `Icon=` do .desktop — diferença intencional: queremos o
//! pixel art harmônico Golden Hour sempre, sem importar qual app
//! resolve o slot.
//!
//! Slot sem app instalado: render alternativo "+" tracejado (vem em
//! commit posterior). Por enquanto todos os 10 renderizam.

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Size},
};

type Label = (MemoryRenderBuffer, Size<i32, Logical>);

/// Buffers do grid de favoritos. Construído com o vec de
/// `CuratedFavorite` (já vem dos defaults H32.10.3 ou do TOML).
#[derive(Debug)]
pub struct FavoritesGrid {
    /// 10 ícones quadrados pre-renderizados (1 por slot).
    pub icons: Vec<MemoryRenderBuffer>,
    /// 10 labels pre-rasterizados Cream (1 por slot).
    pub labels: Vec<Option<Label>>,
    /// 10 labels pre-rasterizados Âmbar (1 por slot) — usado no item
    /// selecionado pra dar mais peso visual (H32.10.9).
    pub labels_selected: Vec<Option<Label>>,
    /// Halo âmbar pré-renderizado atrás do ícone selecionado. Tamanho
    /// = icon_size + 2*padding, alpha = selected_glow_alpha.
    pub glow: MemoryRenderBuffer,
}

impl FavoritesGrid {
    pub fn new(items: &[crate::menu::state::CuratedFavorite]) -> Self {
        let theme = crate::theme::current();
        let f = &theme.menu_favorites;

        let icons = items
            .iter()
            .map(|fav| {
                // Quadrado externo + (opcional) interno menor.
                // make_rounded_panel já produz Memory Argb8888 premul.
                let outer = crate::gfx::make_rounded_panel(
                    f.icon_size,
                    f.icon_size,
                    f.icon_corner_radius,
                    fav.icon_color,
                );
                if let Some(inner) = fav.icon_inner_color {
                    let _ = inner; // inner sobreposto via render time (TODO)
                }
                outer
            })
            .collect();

        // Halo: quadrado maior amber alpha baixo (proxy de gaussian
        // blur que não temos via shader).
        let glow_size = f.icon_size + 2 * f.selected_glow_padding;
        let glow_color = smithay::backend::renderer::Color32F::new(
            f.selected_glow_color.r(),
            f.selected_glow_color.g(),
            f.selected_glow_color.b(),
            f.selected_glow_alpha,
        );
        let glow = crate::gfx::make_rounded_panel(
            glow_size,
            glow_size,
            f.icon_corner_radius + f.selected_glow_padding,
            glow_color,
        );

        Self {
            icons,
            labels: vec![None; items.len()],
            labels_selected: vec![None; items.len()],
            glow,
        }
    }

    /// Rasteriza labels 1x (lazy).
    pub fn ensure_labels(
        &mut self,
        text: &crate::text::TextRenderer,
        items: &[crate::menu::state::CuratedFavorite],
    ) {
        let theme = crate::theme::current();
        let f = &theme.menu_favorites;
        for (i, fav) in items.iter().enumerate() {
            if i >= self.labels.len() {
                break;
            }
            let truncated = if fav.label.chars().count() > f.label_max_chars {
                let mut s: String = fav.label.chars().take(f.label_max_chars - 1).collect();
                s.push('…');
                s
            } else {
                fav.label.clone()
            };
            if self.labels[i].is_none() {
                self.labels[i] = rasterize_colored(
                    text, &truncated, f.label_size, f.label_color,
                );
            }
            if self.labels_selected[i].is_none() {
                self.labels_selected[i] = rasterize_colored(
                    text, &truncated, f.label_size, f.selected_label_color,
                );
            }
        }
    }
}

/// Layout do grid centralizado no zone[2] (fav_grid wireframe).
///
/// Retorna 10 (col, row, top-left logical point) — caller posiciona
/// cada ícone e label.
pub fn slot_origins(
    zone_origin: Point<i32, Logical>,
) -> Vec<Point<i32, Logical>> {
    let theme = crate::theme::current();
    let f = &theme.menu_favorites;
    let total = (f.columns * f.rows) as usize;
    let mut out = Vec::with_capacity(total);
    for row in 0..f.rows {
        for col in 0..f.columns {
            let x = zone_origin.x + col * (f.icon_size + f.gap_horizontal);
            let y = zone_origin.y + row * (f.icon_size + f.gap_vertical
                + f.label_margin_top + f.label_size as i32);
            out.push(Point::from((x, y)));
        }
    }
    out
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
