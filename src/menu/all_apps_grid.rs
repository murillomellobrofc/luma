//! Grid 8×4 paginado de todos os apps. H32.10.11.
//!
//! Lê `state.apps` (já carregado em load() do app boot) — todos os
//! `.desktop` instalados ordenados alfabético, com NoDisplay/Hidden
//! filtrados. Renderiza 32 por página com label embaixo.
//!
//! Paginação (PageUp/Down, dots) vem em H32.10.12. Ícone real do
//! `Icon=` vem em H32.10.11.fix (este commit usa fallback colorido).

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Size},
};

type Label = (MemoryRenderBuffer, Size<i32, Logical>);

/// Buffers do grid 8×4. Diferente do FavoritesGrid, aqui os ícones são
/// gerados sob demanda (32 quadrados fallback por enquanto) e os labels
/// cacheados na ordem de `state.apps`.
#[derive(Debug)]
pub struct AllAppsGrid {
    /// Quadrado fallback usado em todos os slots por enquanto. Quando
    /// `Icon=` real for resolvido (H32.10.11.fix), vira `Vec<...>`.
    pub fallback_icon: MemoryRenderBuffer,
    /// Labels rasterizados na ordem de `state.apps`. Cresce sob demanda.
    pub labels: Vec<Option<Label>>,
    /// Page dot ativo (âmbar) pré-renderizado.
    pub dot_active: MemoryRenderBuffer,
    /// Page dot inativo (cream alpha 0.25) pré-renderizado.
    pub dot_inactive: MemoryRenderBuffer,
    /// "Página X de Y" label cache (key = (page, total)).
    pub page_label: Option<Label>,
    page_label_key: Option<(usize, usize)>,
    /// Glow âmbar atrás do app hovered (proxy de blur).
    pub hover_glow: MemoryRenderBuffer,
    pub hover_glow_pad: i32,
}

impl AllAppsGrid {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let a = &theme.menu_all_apps;
        let fallback_icon = crate::gfx::make_rounded_panel(
            a.icon_size, a.icon_size, a.icon_corner_radius, a.fallback_color,
        );
        // Dots = quadrado rounded (não tem make_circle) — radius=size/2
        // dá um round bonitinho dado size 5.
        let dot_active = crate::gfx::make_rounded_panel(
            a.page_dot_size, a.page_dot_size,
            a.page_dot_size / 2, a.page_dot_color_active,
        );
        let inactive_color = smithay::backend::renderer::Color32F::new(
            a.page_dot_color_inactive.r(),
            a.page_dot_color_inactive.g(),
            a.page_dot_color_inactive.b(),
            a.page_dot_alpha_inactive,
        );
        let dot_inactive = crate::gfx::make_rounded_panel(
            a.page_dot_size, a.page_dot_size,
            a.page_dot_size / 2, inactive_color,
        );
        let hover_glow_pad = 12;
        let glow_color = smithay::backend::renderer::Color32F::new(
            0xE8 as f32 / 255.0,
            0x9B as f32 / 255.0,
            0x3D as f32 / 255.0,
            0.85,
        );
        let hover_glow = crate::gfx::make_rounded_panel(
            a.icon_size + 2 * hover_glow_pad,
            a.icon_size + 2 * hover_glow_pad,
            a.icon_corner_radius + hover_glow_pad,
            glow_color,
        );

        Self {
            fallback_icon,
            labels: Vec::new(),
            dot_active,
            dot_inactive,
            page_label: None,
            page_label_key: None,
            hover_glow,
            hover_glow_pad,
        }
    }

    /// Rasteriza "Página X de Y" se mudou.
    pub fn update_page_label(
        &mut self,
        text: &crate::text::TextRenderer,
        page: usize,
        total_pages: usize,
    ) {
        let key = (page, total_pages);
        if self.page_label_key == Some(key) && self.page_label.is_some() {
            return;
        }
        let a = &crate::theme::current().menu_all_apps;
        let phrase = format!("Página {} de {}", page + 1, total_pages.max(1));
        self.page_label = rasterize_colored(
            text, &phrase, a.page_label_size, a.page_label_color,
        );
        self.page_label_key = Some(key);
    }

    pub fn ensure_labels(
        &mut self,
        text: &crate::text::TextRenderer,
        apps: &[crate::apps::DesktopApp],
    ) {
        let theme = crate::theme::current();
        let a = &theme.menu_all_apps;
        while self.labels.len() < apps.len() {
            self.labels.push(None);
        }
        for (i, app) in apps.iter().enumerate() {
            if self.labels[i].is_some() {
                continue;
            }
            let truncated = if app.name.chars().count() > a.label_max_chars {
                let mut s: String = app.name.chars().take(a.label_max_chars - 1).collect();
                s.push('…');
                s
            } else {
                app.name.clone()
            };
            self.labels[i] = rasterize_colored(
                text, &truncated, a.label_size, a.label_color,
            );
        }
    }
}

impl Default for AllAppsGrid {
    fn default() -> Self {
        Self::new()
    }
}

/// Posições top-left dos 32 slots da página. Ancorado em `zone_origin`.
pub fn slot_origins(
    zone_origin: Point<i32, Logical>,
) -> Vec<Point<i32, Logical>> {
    let theme = crate::theme::current();
    let a = &theme.menu_all_apps;
    let total = (a.columns * a.rows_per_page) as usize;
    let mut out = Vec::with_capacity(total);
    let row_height = a.icon_size + a.label_margin_top
        + a.label_size as i32 + a.gap_vertical;
    for row in 0..a.rows_per_page {
        for col in 0..a.columns {
            let x = zone_origin.x + col * (a.icon_size + a.gap_horizontal);
            let y = zone_origin.y + row * row_height;
            out.push(Point::from((x, y)));
        }
    }
    out
}

/// Largura total do grid (pra centralizar horizontal no zone).
pub fn grid_width() -> i32 {
    let a = &crate::theme::current().menu_all_apps;
    a.columns * a.icon_size + (a.columns - 1) * a.gap_horizontal
}

/// Altura total do grid (4 rows + 3 gaps + 4 labels).
pub fn grid_height() -> i32 {
    let a = &crate::theme::current().menu_all_apps;
    let row_h = a.icon_size + a.label_margin_top + a.label_size as i32;
    a.rows_per_page * row_h + (a.rows_per_page - 1) * a.gap_vertical
}

/// Quantidade total de páginas dado `total_apps`.
pub fn page_count(total_apps: usize) -> usize {
    let a = &crate::theme::current().menu_all_apps;
    let per_page = (a.columns * a.rows_per_page) as usize;
    if total_apps == 0 {
        1
    } else {
        total_apps.div_ceil(per_page)
    }
}

/// Posições top-left dos N page dots centralizados horiz no zone.
/// `grid_origin` é o top-left do grid (não do zone).
pub fn dot_origins(
    grid_origin: Point<i32, Logical>,
    total_pages: usize,
) -> Vec<Point<i32, Logical>> {
    let a = &crate::theme::current().menu_all_apps;
    let n = total_pages.max(1) as i32;
    let total_w = n * a.page_dot_size + (n - 1).max(0) * a.page_dot_gap;
    let gw = grid_width();
    let start_x = grid_origin.x + (gw - total_w) / 2;
    let y = grid_origin.y + grid_height() + a.page_dots_margin_top;
    (0..n)
        .map(|i| Point::from((start_x + i * (a.page_dot_size + a.page_dot_gap), y)))
        .collect()
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
