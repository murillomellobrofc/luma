//! Pager de workspaces na taskbar. H36 Bloco C.
//!
//! Renderiza N items horizontais entre o launcher e a window list:
//! - Workspace inativo: dot pequeno alpha 0.25 (ou 0.55 em hover)
//! - Workspace ativo: pill âmbar 28×16
//!
//! Esconde quando há apenas 1 workspace (tokens.pager_show_if_count).

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::element::{memory::MemoryRenderBuffer, solid::SolidColorBuffer},
    },
    utils::{Logical, Point, Size, Transform},
};

/// Label rasterizado de um número do pager (buffer + size).
pub type NumberLabel = (MemoryRenderBuffer, Size<i32, Logical>);

/// Buffers pré-renderizados pra cada estado do item do pager.
/// Reutilizados entre frames; redimensionados em runtime quando contagem
/// de workspaces muda.
#[derive(Debug)]
pub struct PagerState {
    /// Bg do dot inativo (8×8 = 2*radius).
    pub dot_inactive: SolidColorBuffer,
    /// Bg do dot em hover (mesmo tamanho).
    pub dot_hover: SolidColorBuffer,
    /// Bg do pill ativo arredondado em capsule (r=h/2). MemoryRenderBuffer
    /// pra ter alpha=0 nos cantos. H36.spec.fix.
    pub pill_active: MemoryRenderBuffer,
    /// Halo externo do glow arredondado.
    pub pill_glow_outer: MemoryRenderBuffer,
    /// Halo interno do glow arredondado.
    pub pill_glow_inner: MemoryRenderBuffer,
    /// Índice do item em hover (0-based no Vec de workspaces).
    pub hovered: Option<usize>,
    /// Labels "1".."9" rasterizados em coffee escuro pra mostrar dentro
    /// dos dots/pills. Lazy: ensure_labels rasteriza on demand.
    /// H36.spec: indica número do workspace.
    pub number_labels: Vec<Option<NumberLabel>>,
}

impl PagerState {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let t = &theme.workspaces;
        let dot_size = (t.pager_dot_radius * 2, t.pager_dot_radius * 2);
        Self {
            dot_inactive: SolidColorBuffer::new(dot_size, t.pager_inactive_color),
            dot_hover: SolidColorBuffer::new(dot_size, t.pager_hover_color),
            // Pill capsule: corner radius = altura/2 pra cantos totalmente
            // arredondados. H36.spec.fix.
            pill_active: crate::gfx::make_rounded_panel(
                t.pager_active_width,
                t.pager_active_height,
                t.pager_active_height / 2,
                t.pager_active_color,
            ),
            // Glow: 2 halos amber alpha baixo, MAIORES que o pill, atrás dele.
            pill_glow_outer: crate::gfx::make_rounded_panel(
                t.pager_active_width + 8,
                t.pager_active_height + 8,
                (t.pager_active_height + 8) / 2,
                {
                    let c = t.pager_active_color;
                    smithay::backend::renderer::Color32F::new(c.r(), c.g(), c.b(), 0.15)
                },
            ),
            pill_glow_inner: crate::gfx::make_rounded_panel(
                t.pager_active_width + 4,
                t.pager_active_height + 4,
                (t.pager_active_height + 4) / 2,
                {
                    let c = t.pager_active_color;
                    smithay::backend::renderer::Color32F::new(c.r(), c.g(), c.b(), 0.35)
                },
            ),
            hovered: None,
            number_labels: Vec::new(),
        }
    }

    /// Rasteriza lazy os labels "1".."9" pra mostrar nos dots/pills.
    /// H36.spec.
    pub fn ensure_labels(&mut self, text: &crate::text::TextRenderer, max: usize) {
        let theme = crate::theme::current();
        let coffee = theme.colors.taskbar_bg_top;
        while self.number_labels.len() < max.min(9) {
            let n = self.number_labels.len() + 1;
            let (pixels, w, h) = text.rasterize_colored(
                &n.to_string(),
                10.0,
                smithay::backend::renderer::Color32F::new(
                    coffee.r(),
                    coffee.g(),
                    coffee.b(),
                    1.0,
                ),
            );
            if w == 0 || h == 0 {
                self.number_labels.push(None);
                continue;
            }
            let buffer = MemoryRenderBuffer::from_slice(
                &pixels,
                Fourcc::Argb8888,
                (w as i32, h as i32),
                1,
                Transform::Normal,
                None,
            );
            self.number_labels
                .push(Some((buffer, Size::from((w as i32, h as i32)))));
        }
    }
}

impl Default for PagerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Visibilidade do pager — esconde se só há 1 workspace. H36.7.
pub fn is_visible(workspace_count: usize) -> bool {
    let t = &crate::theme::current().workspaces;
    workspace_count >= t.pager_show_if_count
}

/// X inicial do pager dentro da taskbar (depois do launcher).
/// Cliente decide o `launcher_end_x`. H36.7.
pub fn pager_start_x(launcher_end_x: i32) -> i32 {
    let t = &crate::theme::current().workspaces;
    launcher_end_x + t.pager_padding_left
}

/// Largura total que o pager ocupa pra `count` workspaces. Usado pra
/// shiftar a window list à direita. H36.7.
pub fn pager_total_width(count: usize) -> i32 {
    if count == 0 {
        return 0;
    }
    let t = &crate::theme::current().workspaces;
    let active_w = t.pager_active_width;
    let dot_w = t.pager_dot_radius * 2;
    let n = count as i32;
    // 1 ativo + (n-1) inativos + (n-1) gaps
    active_w + (n - 1).max(0) * dot_w + (n - 1).max(0) * t.pager_gap
}

/// Layout: pra cada workspace idx, devolve o ponto (x, y_top) do item.
/// `taskbar_y` é o topo da taskbar; centraliza verticalmente. H36.7.
pub fn item_layout(
    start_x: i32,
    taskbar_y: i32,
    taskbar_h: i32,
    count: usize,
    current: usize,
) -> Vec<(i32, i32, bool)> {
    let t = &crate::theme::current().workspaces;
    let mut out = Vec::with_capacity(count);
    let mut cursor = start_x;
    let dot_w = t.pager_dot_radius * 2;
    for idx in 0..count {
        let active = idx == current;
        let (w, h) = if active {
            (t.pager_active_width, t.pager_active_height)
        } else {
            (dot_w, dot_w)
        };
        let y = taskbar_y + (taskbar_h - h) / 2;
        out.push((cursor, y, active));
        cursor += w + t.pager_gap;
    }
    out
}

/// Hit-test: dado um ponto, retorna o índice do item embaixo (ou None).
/// Hit area é generosa (mínimo 24×24 pra acessibilidade).
pub fn hit_test(
    point: Point<f64, Logical>,
    layout: &[(i32, i32, bool)],
    taskbar_y: i32,
    taskbar_h: i32,
) -> Option<usize> {
    let t = &crate::theme::current().workspaces;
    let dot_w = t.pager_dot_radius * 2;
    for (idx, (x, _y, active)) in layout.iter().enumerate() {
        let item_w = if *active {
            t.pager_active_width
        } else {
            dot_w
        };
        // Hit area mínima 24px de largura mesmo se visual menor.
        let hit_w = item_w.max(24);
        let hit_x = x - (hit_w - item_w) / 2;
        if point.x as i32 >= hit_x
            && (point.x as i32) < hit_x + hit_w
            && point.y as i32 >= taskbar_y
            && (point.y as i32) < taskbar_y + taskbar_h
        {
            return Some(idx);
        }
    }
    None
}

/// Tamanho do dot inativo pra resize dinâmico futuro.
pub fn dot_size() -> Size<i32, Logical> {
    let t = &crate::theme::current().workspaces;
    Size::from((t.pager_dot_radius * 2, t.pager_dot_radius * 2))
}
