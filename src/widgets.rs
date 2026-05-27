//! Widgets nativos do Luma. POC da fatia 1: LabelWidget (texto fixo
//! numa pill arredondada). Cada widget implementa `luma_widget::Widget`
//! e se desenha num `MemoryRenderBuffer` único — o compositor compõe
//! o resultado direto, sem precisar entender layout do widget.
//!
//! As implementações ficam aqui no crate principal porque dependem
//! de helpers locais (gfx, text). O TRAIT mora em luma-widget pra
//! permitir widgets de terceiros sem ciclo de deps.

use std::sync::Arc;

use luma_widget::{RenderCtx, Widget};
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::element::memory::MemoryRenderBuffer,
    },
    utils::Transform,
};

use crate::{gfx::make_rounded_panel, text::TextRenderer};

/// Widget de texto fixo numa pill arredondada Golden Hour.
/// POC pra provar que a fundação funciona — fatia 1 do sistema de
/// widgets. Sem state, sem repaint, sem interação.
pub struct LabelWidget {
    text: String,
    text_renderer: Arc<TextRenderer>,
}

impl LabelWidget {
    pub fn new(text: impl Into<String>, text_renderer: Arc<TextRenderer>) -> Self {
        Self {
            text: text.into(),
            text_renderer,
        }
    }
}

impl Widget for LabelWidget {
    fn render(&self, ctx: &RenderCtx) -> MemoryRenderBuffer {
        // Cores e métricas do theme. Zero hex hardcoded aqui — tudo
        // sai dos tokens. Reusa lock.notif_* como ponto de partida do
        // Golden Hour (pill discreta sobre fundo escuro); a fatia 2
        // ganha sua própria seção de tokens quando o registro entrar.
        let lock = &ctx.theme.lock;
        let pill_color = lock.notif_row_bg;
        let text_color = lock.notif_app_color;
        let font_size = lock.notif_app_size;
        let corner = lock.notif_row_corner;
        let padding_h = lock.notif_row_padding_h;

        // 1) Rasteriza o texto pra saber a largura final da pill.
        let (text_px, text_w, text_h) =
            self.text_renderer.rasterize_colored(&self.text, font_size, text_color);

        let panel_h = ctx.panel_height as i32;
        let pill_w = padding_h + text_w as i32 + padding_h;
        let pill_h = panel_h;

        // 2) Pill arredondada (mesmo helper que o resto do compositor
        //    usa — alpha=0 fora dos cantos).
        let pill = make_rounded_panel(pill_w, pill_h, corner, pill_color);
        // `MemoryRenderBuffer` não expõe os pixels diretamente, então
        // recriamos o buffer manualmente compondo pill + texto.
        let mut pixels = vec![0u8; (pill_w * pill_h * 4) as usize];

        // Reconstrói os pixels da pill (mesma lógica do make_rounded_panel,
        // mas aqui a gente precisa do buffer mutável pra alpha-blend o
        // texto em cima).
        let r = (pill_color.r() * 255.0).round() as u8;
        let g = (pill_color.g() * 255.0).round() as u8;
        let b = (pill_color.b() * 255.0).round() as u8;
        let a = (pill_color.a() * 255.0).round() as u8;
        let pb = ((r as u16 * a as u16) / 255) as u8;
        let pg = ((g as u16 * a as u16) / 255) as u8;
        let pr = ((b as u16 * a as u16) / 255) as u8;
        let r_sq = corner * corner;
        for y in 0..pill_h {
            for x in 0..pill_w {
                if is_inside_rounded(x, y, pill_w, pill_h, corner, r_sq) {
                    let idx = ((y * pill_w + x) * 4) as usize;
                    pixels[idx] = pr;
                    pixels[idx + 1] = pg;
                    pixels[idx + 2] = pb;
                    pixels[idx + 3] = a;
                }
            }
        }

        // 3) Compõe o texto por cima da pill (alpha-blend BGRA premultiplicado).
        if text_w > 0 && text_h > 0 {
            let tx0 = (pill_w - text_w as i32) / 2;
            let ty0 = (pill_h - text_h as i32) / 2;
            for ty in 0..text_h as i32 {
                for tx in 0..text_w as i32 {
                    let dst_x = tx0 + tx;
                    let dst_y = ty0 + ty;
                    if dst_x < 0 || dst_y < 0 || dst_x >= pill_w || dst_y >= pill_h {
                        continue;
                    }
                    let src_idx = ((ty * text_w as i32 + tx) * 4) as usize;
                    let sa = text_px[src_idx + 3];
                    if sa == 0 {
                        continue;
                    }
                    let sb = text_px[src_idx];
                    let sg = text_px[src_idx + 1];
                    let sr = text_px[src_idx + 2];
                    let dst_idx = ((dst_y * pill_w + dst_x) * 4) as usize;
                    let inv = 255u16 - sa as u16;
                    pixels[dst_idx] =
                        ((sb as u16 * 255 + pixels[dst_idx] as u16 * inv) / 255) as u8;
                    pixels[dst_idx + 1] =
                        ((sg as u16 * 255 + pixels[dst_idx + 1] as u16 * inv) / 255) as u8;
                    pixels[dst_idx + 2] =
                        ((sr as u16 * 255 + pixels[dst_idx + 2] as u16 * inv) / 255) as u8;
                    pixels[dst_idx + 3] =
                        (sa as u16 + (pixels[dst_idx + 3] as u16 * inv) / 255) as u8;
                }
            }
        }

        let _ = pill; // helper só serviu pra documentar a forma esperada
        MemoryRenderBuffer::from_slice(
            &pixels,
            Fourcc::Argb8888,
            (pill_w, pill_h),
            1,
            Transform::Normal,
            None,
        )
    }
}

/// Mesma lógica do gfx::is_inside_rounded — duplicada localmente
/// porque a função original é pub(crate) no gfx e a widget compõe
/// a pill manualmente em vez de usar o helper (pra alpha-blend texto
/// em cima). Quando a fatia 2 entrar, ou expõe-se o helper ou move-se
/// pra um crate de gfx compartilhado.
fn is_inside_rounded(x: i32, y: i32, w: i32, h: i32, radius: i32, r_sq: i32) -> bool {
    if radius <= 0 {
        return true;
    }
    let in_left = x < radius;
    let in_right = x >= w - radius;
    let in_top = y < radius;
    let in_bottom = y >= h - radius;
    if !(in_left || in_right) || !(in_top || in_bottom) {
        return true;
    }
    let cx = if in_left { radius } else { w - radius - 1 };
    let cy = if in_top { radius } else { h - radius - 1 };
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= r_sq
}
