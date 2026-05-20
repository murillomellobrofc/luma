use std::{fs, path::Path};

use fontdue::{
    layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle},
    Font,
};
use smithay::backend::renderer::Color32F;
use tracing::warn;

/// Caminhos comuns onde uma fonte sans-serif decente pode estar instalada
/// no Fedora. Tenta na ordem; primeiro que existe vai.
const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/google-noto-sans-vf-fonts/NotoSans[wdth,wght].ttf",
    "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
    "/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf",
    "/usr/share/fonts/google-carlito-fonts/Carlito-Regular.ttf",
    "/usr/share/fonts/abattis-cantarell-fonts/Cantarell-Regular.otf",
];

pub struct TextRenderer {
    font: Font,
}

impl TextRenderer {
    pub fn load() -> Option<Self> {
        for candidate in FONT_CANDIDATES {
            if Path::new(candidate).exists() {
                if let Some(font) = try_load(candidate) {
                    return Some(Self { font });
                }
            }
        }
        warn!("nenhuma fonte sans-serif encontrada — título do header ficará vazio");
        None
    }

    /// Rasteriza `text` em um buffer Argb8888 (byte order BGRA em little
    /// endian). Cor: branca com alpha do glyph. Retorna (pixels, w, h).
    /// Pixels é vazio quando o texto cabe em 0×0 (string vazia).
    pub fn rasterize(&self, text: &str, size: f32) -> (Vec<u8>, u32, u32) {
        if text.is_empty() {
            return (Vec::new(), 0, 0);
        }
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings::default());
        layout.append(&[&self.font], &TextStyle::new(text, size, 0));

        let glyphs = layout.glyphs();
        let mut width = 0u32;
        let mut height = 0u32;
        for g in glyphs {
            let right = (g.x.ceil() as i32 + g.width as i32).max(0) as u32;
            let bottom = (g.y.ceil() as i32 + g.height as i32).max(0) as u32;
            width = width.max(right);
            height = height.max(bottom);
        }
        if width == 0 || height == 0 {
            return (Vec::new(), 0, 0);
        }

        let mut buffer = vec![0u8; (width * height * 4) as usize];
        for g in glyphs {
            let (_, bitmap) = self.font.rasterize(g.parent, size);
            for gy in 0..g.height {
                for gx in 0..g.width {
                    let alpha = bitmap[gy * g.width + gx];
                    let px = (g.x as i32 + gx as i32).max(0) as u32;
                    let py = (g.y as i32 + gy as i32).max(0) as u32;
                    if px >= width || py >= height {
                        continue;
                    }
                    let off = (py * width + px) as usize * 4;
                    // Argb8888 com PREMULTIPLIED alpha (smithay/gles esperam).
                    // Pra branco: R=G=B=alpha (255 * alpha / 255 = alpha).
                    // Byte order em little-endian fica [B, G, R, A].
                    buffer[off] = alpha; // B
                    buffer[off + 1] = alpha; // G
                    buffer[off + 2] = alpha; // R
                    buffer[off + 3] = alpha; // A
                }
            }
        }

        (buffer, width, height)
    }

    /// Rasteriza texto numa cor específica (em vez de branco). Aplica
    /// premultiplied alpha com `color.r/g/b`. H32.7.fix.20.
    pub fn rasterize_colored(
        &self,
        text: &str,
        size: f32,
        color: Color32F,
    ) -> (Vec<u8>, u32, u32) {
        let (mut pixels, w, h) = self.rasterize(text, size);
        if pixels.is_empty() {
            return (pixels, w, h);
        }
        for chunk in pixels.chunks_exact_mut(4) {
            let a = chunk[3] as f32 / 255.0;
            chunk[0] = (color.b() * a * 255.0).round() as u8;
            chunk[1] = (color.g() * a * 255.0).round() as u8;
            chunk[2] = (color.r() * a * 255.0).round() as u8;
        }
        (pixels, w, h)
    }

    /// Rasteriza com chars individuais coloridos: chars cujo índice está
    /// em `matched_indices` saem em `highlight_color`, o resto em
    /// `base_color`. Usado pelo H32.9.4 pra mostrar os chars que casaram
    /// no fuzzy search em amber sobre o resto cream.
    pub fn rasterize_highlighted(
        &self,
        text: &str,
        size: f32,
        base_color: Color32F,
        highlight_color: Color32F,
        matched_indices: &[usize],
    ) -> (Vec<u8>, u32, u32) {
        use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};
        if text.is_empty() {
            return (Vec::new(), 0, 0);
        }
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings::default());
        layout.append(&[&self.font], &TextStyle::new(text, size, 0));
        let glyphs = layout.glyphs();
        let mut width = 0u32;
        let mut height = 0u32;
        for g in glyphs {
            let right = (g.x.ceil() as i32 + g.width as i32).max(0) as u32;
            let bottom = (g.y.ceil() as i32 + g.height as i32).max(0) as u32;
            width = width.max(right);
            height = height.max(bottom);
        }
        if width == 0 || height == 0 {
            return (Vec::new(), 0, 0);
        }
        let matched: std::collections::HashSet<usize> =
            matched_indices.iter().copied().collect();
        let mut buffer = vec![0u8; (width * height * 4) as usize];
        for (i, g) in glyphs.iter().enumerate() {
            let color = if matched.contains(&i) {
                highlight_color
            } else {
                base_color
            };
            let (_, bitmap) = self.font.rasterize(g.parent, size);
            for gy in 0..g.height {
                for gx in 0..g.width {
                    let alpha = bitmap[gy * g.width + gx];
                    let px = (g.x as i32 + gx as i32).max(0) as u32;
                    let py = (g.y as i32 + gy as i32).max(0) as u32;
                    if px >= width || py >= height {
                        continue;
                    }
                    let off = (py * width + px) as usize * 4;
                    let a = alpha as f32 / 255.0;
                    buffer[off] = (color.b() * a * 255.0).round() as u8;
                    buffer[off + 1] = (color.g() * a * 255.0).round() as u8;
                    buffer[off + 2] = (color.r() * a * 255.0).round() as u8;
                    buffer[off + 3] = alpha;
                }
            }
        }
        (buffer, width, height)
    }
}

fn try_load(path: &str) -> Option<Font> {
    let bytes = fs::read(path).ok()?;
    Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
}
