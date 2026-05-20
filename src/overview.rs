//! Overview screen — Activities-style. Aberto via Ctrl+Alt+Esc.
//!
//! Layout: backdrop fullscreen Coffee alpha 0.5 + linha de thumbs
//! horizontais centralizada + botão "+" no fim. Cada thumb representa
//! um workspace; o ativo tem border âmbar.
//!
//! H36 Bloco D.

use smithay::{
    backend::renderer::element::solid::SolidColorBuffer,
    utils::{Logical, Point, Size},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewHit {
    /// Click num workspace existente (idx).
    Workspace(usize),
    /// Click no botão "+".
    NewWorkspace,
}

/// Buffers pré-renderizados pro overview. H36.11.
#[derive(Debug)]
pub struct OverviewState {
    /// Backdrop fullscreen Coffee alpha (legacy solid, mantido pro
    /// fallback). H36.spec.
    pub backdrop: SolidColorBuffer,
    /// Backdrop gradient vertical Coffee→Amber (1×1200 pre-renderizado,
    /// expandido horizontal no render). H36.spec.fix.
    pub backdrop_gradient: smithay::backend::renderer::element::memory::MemoryRenderBuffer,
    /// Bg do thumb inativo (190×120 ou tema).
    pub thumb_bg: SolidColorBuffer,
    /// Bg do thumb do workspace ativo (border amber via outra solid).
    pub thumb_active_bg: SolidColorBuffer,
    /// Border âmbar pro workspace ativo (4 SolidColorBuffer pros lados).
    pub border_top: SolidColorBuffer,
    pub border_bottom: SolidColorBuffer,
    pub border_left: SolidColorBuffer,
    pub border_right: SolidColorBuffer,
    /// Bg do botão "+". Cor mais sutil que thumb.
    pub plus_bg: SolidColorBuffer,
    /// Cruz "+" 28×28 desenhada em cream alpha 0.5, pra centralizar
    /// no plus_bg. H36.spec.
    pub plus_icon: smithay::backend::renderer::element::memory::MemoryRenderBuffer,
    pub plus_icon_size: smithay::utils::Size<i32, smithay::utils::Logical>,
    /// Title "Workspaces" rasterizado em Cream 24px. H36.spec.
    pub title_label: Option<(
        smithay::backend::renderer::element::memory::MemoryRenderBuffer,
        smithay::utils::Size<i32, smithay::utils::Logical>,
    )>,
    /// Subtitle "Clique pra alternar..." em Secondary 12px.
    pub subtitle_label: Option<(
        smithay::backend::renderer::element::memory::MemoryRenderBuffer,
        smithay::utils::Size<i32, smithay::utils::Logical>,
    )>,
}

impl OverviewState {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let t = &theme.workspaces;
        let amber = theme.colors.accent_amber;
        Self {
            backdrop: SolidColorBuffer::new((1, 1), t.overview_backdrop),
            backdrop_gradient: crate::gfx::make_vertical_gradient(
                1200,
                // Topo: Coffee BEM escuro (puxa pro preto).
                smithay::backend::renderer::Color32F::new(
                    theme.colors.taskbar_bg_top.r() * 0.35,
                    theme.colors.taskbar_bg_top.g() * 0.35,
                    theme.colors.taskbar_bg_top.b() * 0.35,
                    0.95,
                ),
                // Base: Coffee médio com tinge amber (efeito warm).
                smithay::backend::renderer::Color32F::new(
                    theme.colors.accent_amber.r() * 0.55,
                    theme.colors.accent_amber.g() * 0.35,
                    theme.colors.accent_amber.b() * 0.20,
                    0.95,
                ),
            ),
            // Thumbs mais luminosos: amber suave + alpha baixo pra dar
            // sensação de "janela transparente Coffee". H36.spec.fix.
            thumb_bg: SolidColorBuffer::new(
                (t.overview_thumb_width, t.overview_thumb_height),
                smithay::backend::renderer::Color32F::new(
                    theme.colors.accent_amber.r() * 0.4,
                    theme.colors.accent_amber.g() * 0.3,
                    theme.colors.accent_amber.b() * 0.2,
                    0.45,
                ),
            ),
            thumb_active_bg: SolidColorBuffer::new(
                (t.overview_thumb_width, t.overview_thumb_height),
                smithay::backend::renderer::Color32F::new(
                    theme.colors.accent_amber.r() * 0.55,
                    theme.colors.accent_amber.g() * 0.42,
                    theme.colors.accent_amber.b() * 0.25,
                    0.6,
                ),
            ),
            border_top: SolidColorBuffer::new(
                (t.overview_thumb_width, 2),
                amber,
            ),
            border_bottom: SolidColorBuffer::new(
                (t.overview_thumb_width, 2),
                amber,
            ),
            border_left: SolidColorBuffer::new(
                (2, t.overview_thumb_height),
                amber,
            ),
            border_right: SolidColorBuffer::new(
                (2, t.overview_thumb_height),
                amber,
            ),
            plus_bg: SolidColorBuffer::new(
                (t.overview_thumb_width, t.overview_thumb_height),
                with_alpha(theme.colors.applet_bg, 0.4),
            ),
            plus_icon: make_plus_icon(28, theme.colors.divider),
            plus_icon_size: smithay::utils::Size::from((28, 28)),
            title_label: None,
            subtitle_label: None,
        }
    }

    /// Rasteriza title + subtitle do overview (lazy, 1x). H36.spec.
    pub fn ensure_labels(&mut self, text: &crate::text::TextRenderer) {
        let theme = crate::theme::current();
        if self.title_label.is_none() {
            let (pixels, w, h) = text.rasterize_colored(
                "Workspaces",
                24.0,
                theme.colors.divider,
            );
            if w > 0 && h > 0 {
                let buffer = smithay::backend::renderer::element::memory::MemoryRenderBuffer::from_slice(
                    &pixels,
                    smithay::backend::allocator::Fourcc::Argb8888,
                    (w as i32, h as i32),
                    1,
                    smithay::utils::Transform::Normal,
                    None,
                );
                self.title_label = Some((buffer, smithay::utils::Size::from((w as i32, h as i32))));
            }
        }
        if self.subtitle_label.is_none() {
            let (pixels, w, h) = text.rasterize_colored(
                "Clique pra alternar · arraste janelas · + cria novo",
                12.0,
                theme.colors.column_header_color,
            );
            if w > 0 && h > 0 {
                let buffer = smithay::backend::renderer::element::memory::MemoryRenderBuffer::from_slice(
                    &pixels,
                    smithay::backend::allocator::Fourcc::Argb8888,
                    (w as i32, h as i32),
                    1,
                    smithay::utils::Transform::Normal,
                    None,
                );
                self.subtitle_label = Some((buffer, smithay::utils::Size::from((w as i32, h as i32))));
            }
        }
    }
}

/// Cria um buffer 28×28 com a cruz "+" desenhada em pixel art, usando
/// `color` premultiplied alpha 0.5. H36.spec.
fn make_plus_icon(
    size: i32,
    color: smithay::backend::renderer::Color32F,
) -> smithay::backend::renderer::element::memory::MemoryRenderBuffer {
    use smithay::backend::allocator::Fourcc;
    use smithay::utils::Transform;
    let stroke = 2;
    let pad = size / 4;
    let alpha = 0.5;
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    // Linha horizontal: y = size/2 ± stroke/2
    let mid = size / 2;
    for x in pad..(size - pad) {
        for dy in -(stroke / 2)..=(stroke / 2) {
            let y = mid + dy;
            if y >= 0 && y < size {
                put_pixel(&mut pixels, x, y, size, color, alpha);
            }
        }
    }
    // Linha vertical: x = size/2 ± stroke/2
    for y in pad..(size - pad) {
        for dx in -(stroke / 2)..=(stroke / 2) {
            let x = mid + dx;
            if x >= 0 && x < size {
                put_pixel(&mut pixels, x, y, size, color, alpha);
            }
        }
    }
    smithay::backend::renderer::element::memory::MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (size, size),
        1,
        Transform::Normal,
        None,
    )
}

fn put_pixel(
    pixels: &mut [u8],
    x: i32,
    y: i32,
    w: i32,
    color: smithay::backend::renderer::Color32F,
    alpha: f32,
) {
    let idx = ((y * w + x) * 4) as usize;
    let a = (color.a() * alpha * 255.0).round() as u8;
    pixels[idx] = (color.b() * alpha * 255.0).round() as u8;
    pixels[idx + 1] = (color.g() * alpha * 255.0).round() as u8;
    pixels[idx + 2] = (color.r() * alpha * 255.0).round() as u8;
    pixels[idx + 3] = a;
}

impl Default for OverviewState {
    fn default() -> Self {
        Self::new()
    }
}

fn with_alpha(c: smithay::backend::renderer::Color32F, alpha: f32) -> smithay::backend::renderer::Color32F {
    smithay::backend::renderer::Color32F::new(c.r(), c.g(), c.b(), alpha)
}

/// Layout dos thumbs: cada workspace + botão "+". Centralizado horizontal,
/// um pouco acima do meio vertical pro título caber em cima.
/// Retorna Vec<(idx_or_plus, x, y, w, h)>. `idx_or_plus` é Some(idx)
/// pro workspace ou None pro botão "+". H36.12.
pub fn thumb_layout(
    output_size: Size<i32, Logical>,
    workspace_count: usize,
) -> Vec<(Option<usize>, i32, i32, i32, i32)> {
    let t = &crate::theme::current().workspaces;
    let w = t.overview_thumb_width;
    let h = t.overview_thumb_height;
    let gap = t.overview_thumb_gap;
    let total_count = workspace_count + 1; // +1 pro botão "+"
    let total_w = total_count as i32 * w + (total_count as i32 - 1) * gap;
    let start_x = (output_size.w - total_w) / 2;
    let center_y = output_size.h / 2 - h / 2 + 20; // +20 pra deixar título acima
    let mut out = Vec::with_capacity(total_count);
    for i in 0..workspace_count {
        let x = start_x + i as i32 * (w + gap);
        out.push((Some(i), x, center_y, w, h));
    }
    let plus_x = start_x + workspace_count as i32 * (w + gap);
    out.push((None, plus_x, center_y, w, h));
    out
}

/// Hit-test do overview. H36.14.
pub fn hit_test(
    point: Point<f64, Logical>,
    layout: &[(Option<usize>, i32, i32, i32, i32)],
) -> Option<OverviewHit> {
    for (id, x, y, w, h) in layout {
        if (point.x as i32) >= *x
            && (point.x as i32) < x + w
            && (point.y as i32) >= *y
            && (point.y as i32) < y + h
        {
            return match id {
                Some(idx) => Some(OverviewHit::Workspace(*idx)),
                None => Some(OverviewHit::NewWorkspace),
            };
        }
    }
    None
}
