//! Search results (lista vertical) do menu fullscreen. H32.10.16.
//!
//! Quando `search_query != ""`, o grid de favoritos/all_apps some e
//! aparece uma lista vertical centralizada com até 6 apps que casam
//! com a query (fuzzy match). Setas navegam, Enter abre.

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Size},
};

type Label = (MemoryRenderBuffer, Size<i32, Logical>);

/// Máximo de resultados mostrados de uma vez.
pub const MAX_VISIBLE: usize = 6;

/// Filtra `apps` pela query via fuzzy match (skim algorithm), retorna
/// índices em `apps` ordenados por score decrescente. Top MAX_VISIBLE.
pub fn match_query(apps: &[crate::apps::DesktopApp], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, usize)> = apps
        .iter()
        .enumerate()
        .filter_map(|(i, app)| matcher.fuzzy_match(&app.name, query).map(|s| (s, i)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(MAX_VISIBLE).map(|(_, i)| i).collect()
}

/// Buffers pré-renderizados pra renderizar a lista de resultados:
/// bg do item selecionado, barra âmbar 4px à esquerda, ícone fallback.
/// Labels (nome do app) são rasterizadas em cache no app_labels do
/// AllAppsGrid (mesmo cache, mesmo Cream 13px) — não duplicamos aqui.
#[derive(Debug)]
pub struct SearchResultsLayer {
    /// Background #3D2520 do item selecionado, corner 8.
    pub item_bg: MemoryRenderBuffer,
    /// Barra âmbar 4×altura_item à esquerda do selecionado.
    pub left_bar: MemoryRenderBuffer,
    /// "↵" rasterizado em âmbar pra indicar Enter no selecionado.
    pub enter_glyph: Option<Label>,
    /// Ícone fallback 40×40 (mesmo do AllAppsGrid mas em outro tamanho).
    pub fallback_icon: MemoryRenderBuffer,
}

impl SearchResultsLayer {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let item_h = item_height();
        let item_w = item_width();
        let item_bg = crate::gfx::make_rounded_panel(
            item_w, item_h, 8, theme.colors.row_bg_hover,
        );
        let left_bar = crate::gfx::make_rounded_panel(
            4, item_h - 12, 0, theme.colors.accent_amber,
        );
        let fallback_icon = crate::gfx::make_rounded_panel(
            ICON_SIZE, ICON_SIZE, 8,
            theme.menu_all_apps.fallback_color,
        );
        Self {
            item_bg,
            left_bar,
            enter_glyph: None,
            fallback_icon,
        }
    }

    pub fn ensure_enter_glyph(&mut self, text: &crate::text::TextRenderer) {
        if self.enter_glyph.is_none() {
            let theme = crate::theme::current();
            let (pixels, w, h) = text.rasterize_colored(
                "Enter", 11.0, theme.colors.accent_amber,
            );
            if w > 0 && h > 0 {
                let buffer = MemoryRenderBuffer::from_slice(
                    &pixels,
                    smithay::backend::allocator::Fourcc::Argb8888,
                    (w as i32, h as i32),
                    1,
                    smithay::utils::Transform::Normal,
                    None,
                );
                self.enter_glyph = Some((buffer, Size::from((w as i32, h as i32))));
            }
        }
    }
}

impl Default for SearchResultsLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Tamanhos (constantes, não vai pro theme ainda).
pub const ICON_SIZE: i32 = 40;
pub const ITEM_PADDING: i32 = 12;
pub const ITEM_GAP: i32 = 6;
pub const NAME_FONT_SIZE: f32 = 14.0;

pub fn item_height() -> i32 {
    ICON_SIZE + 2 * ITEM_PADDING
}
pub fn item_width() -> i32 {
    // Largura igual à área central, dada por FAV_GRID_W.
    crate::menu::render::layout::FAV_GRID_W
}

/// Posição top-left de cada item na lista (vertical, ancorado em zone[2]).
pub fn item_origins(zone_origin: Point<i32, Logical>, n: usize) -> Vec<Point<i32, Logical>> {
    let step = item_height() + ITEM_GAP;
    (0..n)
        .map(|i| Point::from((zone_origin.x, zone_origin.y + i as i32 * step)))
        .collect()
}
