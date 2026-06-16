//! Pager de workspaces na taskbar.
//!
//! H33.taskbar.7: quadrados 32×32 numerados (re-skin do pager H36).
//! - Ativo: pill âmbar 32×32 r=9 + número COFFEE bold centralizado.
//! - Inativo: sem bg (transparente) + número CREAM alpha 45%.
//! - Hover (inativo): bg sutil cream alpha 0.1 pra feedback.
//!
//! Esconde quando há apenas 1 workspace (`tokens.pager_hide_if_single`).

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::element::memory::MemoryRenderBuffer,
    },
    utils::{Logical, Point, Size, Transform},
};

/// Label rasterizado de um número do pager (buffer + size).
pub type NumberLabel = (MemoryRenderBuffer, Size<i32, Logical>);

/// Buffers pré-renderizados pra cada estado do item do pager.
/// H33.taskbar.7: dots saíram. Apenas pill ativo (32×32 r=9) é
/// pré-renderizado; itens inativos não têm bg (transparente).
#[derive(Debug)]
pub struct PagerState {
    /// Bg do pill ativo (32×32 r=9 cor âmbar). MemoryRenderBuffer pra
    /// ter alpha=0 nos cantos. H33.taskbar.7.
    pub pill_active: MemoryRenderBuffer,
    /// Bg sutil do hover de item inativo (32×32 r=9 cream alpha 0.1).
    /// Cobre cliques durante hover sem alterar o número.
    pub hover_bg: MemoryRenderBuffer,
    /// Índice do item em hover (0-based no Vec de workspaces).
    pub hovered: Option<usize>,
    /// Labels "1".."9" rasterizados em coffee escuro pra mostrar dentro
    /// do pill ATIVO. Lazy: ensure_labels rasteriza on demand.
    pub number_labels_active: Vec<Option<NumberLabel>>,
    /// Labels "1".."9" rasterizados em cream pra mostrar nos quadrados
    /// INATIVOS (renderizados com alpha 0.45 no draw).
    pub number_labels_inactive: Vec<Option<NumberLabel>>,
}

impl PagerState {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let t = &theme.workspaces;
        Self {
            pill_active: crate::gfx::make_rounded_panel(
                t.pager_item_size,
                t.pager_item_size,
                t.pager_corner_radius,
                t.pager_active_color,
            ),
            hover_bg: crate::gfx::make_rounded_panel(
                t.pager_item_size,
                t.pager_item_size,
                t.pager_corner_radius,
                smithay::backend::renderer::Color32F::new(
                    t.pager_text_color_inactive.r(),
                    t.pager_text_color_inactive.g(),
                    t.pager_text_color_inactive.b(),
                    0.1,
                ),
            ),
            hovered: None,
            number_labels_active: Vec::new(),
            number_labels_inactive: Vec::new(),
        }
    }

    /// Rasteriza lazy os labels "1".."9" em ambas as cores (coffee pro
    /// ativo, cream pro inativo). H33.taskbar.7.
    pub fn ensure_labels(&mut self, text: &crate::text::TextRenderer, max: usize) {
        let theme = crate::theme::current();
        let t = &theme.workspaces;
        let target = max.min(9);

        while self.number_labels_active.len() < target {
            let n = self.number_labels_active.len() + 1;
            self.number_labels_active
                .push(build_label(text, &n.to_string(), t.pager_text_size, t.pager_text_color_active));
        }
        while self.number_labels_inactive.len() < target {
            let n = self.number_labels_inactive.len() + 1;
            self.number_labels_inactive
                .push(build_label(text, &n.to_string(), t.pager_text_size, t.pager_text_color_inactive));
        }
    }
}

fn build_label(
    renderer: &crate::text::TextRenderer,
    text: &str,
    size: f32,
    color: smithay::backend::renderer::Color32F,
) -> Option<NumberLabel> {
    let (pixels, w, h) = renderer.rasterize_colored(text, size, color);
    if w == 0 || h == 0 {
        return None;
    }
    let buffer = MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (w as i32, h as i32),
        1,
        Transform::Normal,
        None,
    );
    Some((buffer, Size::from((w as i32, h as i32))))
}

impl Default for PagerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Visibilidade do pager — esconde se só há 1 workspace e
/// `pager_hide_if_single` está ativo. H33.taskbar.8.
pub fn is_visible(workspace_count: usize) -> bool {
    let t = &crate::theme::current().workspaces;
    if t.pager_hide_if_single && workspace_count < 2 {
        return false;
    }
    workspace_count > 0
}

/// X inicial do pager dentro da taskbar (depois do launcher).
/// Cliente decide o `launcher_end_x`. H36.7.
pub fn pager_start_x(launcher_end_x: i32) -> i32 {
    let t = &crate::theme::current().workspaces;
    launcher_end_x + t.pager_padding_left
}

/// Largura total que o pager ocupa pra `count` workspaces. Usado pra
/// shiftar a window list à direita. H33.taskbar.7: agora todos os items
/// têm o mesmo tamanho (`pager_item_size`).
pub fn pager_total_width(count: usize) -> i32 {
    if count == 0 {
        return 0;
    }
    let t = &crate::theme::current().workspaces;
    let n = count as i32;
    t.pager_item_size * n + t.pager_gap * (n - 1).max(0)
}

/// Layout: pra cada workspace idx, devolve o ponto (x, y_top) do item.
/// `taskbar_y` é o topo da taskbar; centraliza verticalmente. H33.taskbar.7:
/// items uniformes 32×32.
pub fn item_layout(
    start_x: i32,
    taskbar_y: i32,
    taskbar_h: i32,
    count: usize,
    current: usize,
) -> Vec<(i32, i32, bool)> {
    let t = &crate::theme::current().workspaces;
    let size = t.pager_item_size;
    let y = taskbar_y + (taskbar_h - size) / 2;
    let mut out = Vec::with_capacity(count);
    let mut cursor = start_x;
    for idx in 0..count {
        out.push((cursor, y, idx == current));
        cursor += size + t.pager_gap;
    }
    out
}

/// Hit-test: dado um ponto, retorna o índice do item embaixo (ou None).
/// Items ≥ 24×24 (mínimo a11y) ainda quando size cai abaixo. H33.taskbar.7.
pub fn hit_test(
    point: Point<f64, Logical>,
    layout: &[(i32, i32, bool)],
    taskbar_y: i32,
    taskbar_h: i32,
) -> Option<usize> {
    let t = &crate::theme::current().workspaces;
    let size = t.pager_item_size.max(24);
    for (idx, (x, _y, _active)) in layout.iter().enumerate() {
        if point.x as i32 >= *x
            && (point.x as i32) < x + size
            && point.y as i32 >= taskbar_y
            && (point.y as i32) < taskbar_y + taskbar_h
        {
            return Some(idx);
        }
    }
    None
}

