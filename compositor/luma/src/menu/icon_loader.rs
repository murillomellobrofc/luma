//! Carrega SVGs do disco e rasteriza pra MemoryRenderBuffer. H32.10.14.
//!
//! Usa `usvg` pra parse + `resvg` pra rasterizar + `tiny-skia` pra
//! pixmap. Saída em Argb8888 premultiplicada (formato esperado pelo
//! Smithay).
//!
//! O Lock vai inline (sem arquivo SVG separado, brief). Os outros 4
//! (Sair, Sleep, Restart, Power) vêm de assets/branding/icons/*.svg.

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::element::memory::MemoryRenderBuffer,
    },
    utils::Transform,
};

/// Parse + rasteriza um arquivo SVG no tamanho dado. Retorna `None`
/// se o path não existir ou se houver erro de parse — fallback é o
/// caller (geralmente quadrado colorido).
pub fn load_svg_file(path: &str, size: u32) -> Option<MemoryRenderBuffer> {
    let bytes = std::fs::read(path).ok()?;
    rasterize_svg_bytes(&bytes, size)
}

/// Mesmo que load_svg_file mas a partir de bytes embedded (inline).
pub fn rasterize_svg_bytes(bytes: &[u8], size: u32) -> Option<MemoryRenderBuffer> {
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;

    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let view_size = tree.size();
    let scale_x = size as f32 / view_size.width();
    let scale_y = size as f32 / view_size.height();
    let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia pixmap é RGBA premultiplicada em row-major little-endian
    // (= bytes [R, G, B, A]). Smithay/GLES esperam Argb8888 em
    // little-endian => bytes [B, G, R, A]. Swap R<->B canal a canal.
    let mut bgra = pixmap.data().to_vec();
    for chunk in bgra.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }

    Some(MemoryRenderBuffer::from_slice(
        &bgra,
        Fourcc::Argb8888,
        (size as i32, size as i32),
        1,
        Transform::Normal,
        None,
    ))
}

/// SVGs inline dos 4 mini-places à direita da session bar. H32.10.15.
/// Folder, download, image, drive — pixel art simples com `stroke_hex`.
pub fn folder_svg_inline(stroke_hex: &str, size: u32) -> Option<MemoryRenderBuffer> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="{s}" height="{s}">
            <path d="M 3 6 L 9 6 L 11 8 L 21 8 L 21 19 L 3 19 Z"
                  fill="none" stroke="{c}" stroke-width="1.5" stroke-linejoin="round"/>
        </svg>"#,
        s = size, c = stroke_hex,
    );
    rasterize_svg_bytes(svg.as_bytes(), size)
}

pub fn download_svg_inline(stroke_hex: &str, size: u32) -> Option<MemoryRenderBuffer> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="{s}" height="{s}">
            <g fill="none" stroke="{c}" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M 12 3 V 14"/>
                <path d="M 7 10 L 12 15 L 17 10"/>
                <path d="M 4 20 H 20"/>
            </g>
        </svg>"#,
        s = size, c = stroke_hex,
    );
    rasterize_svg_bytes(svg.as_bytes(), size)
}

pub fn image_svg_inline(stroke_hex: &str, size: u32) -> Option<MemoryRenderBuffer> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="{s}" height="{s}">
            <g fill="none" stroke="{c}" stroke-width="1.5" stroke-linejoin="round">
                <rect x="3" y="4" width="18" height="16" rx="2"/>
                <circle cx="8" cy="9" r="1.5"/>
                <path d="M 4 17 L 9 13 L 14 17 L 17 14 L 20 17"/>
            </g>
        </svg>"#,
        s = size, c = stroke_hex,
    );
    rasterize_svg_bytes(svg.as_bytes(), size)
}

pub fn drive_svg_inline(stroke_hex: &str, size: u32) -> Option<MemoryRenderBuffer> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="{s}" height="{s}">
            <g fill="none" stroke="{c}" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <rect x="3" y="8" width="18" height="9" rx="1.5"/>
                <circle cx="7" cy="12.5" r="0.8" fill="{c}" stroke="none"/>
                <circle cx="10" cy="12.5" r="0.8" fill="{c}" stroke="none"/>
            </g>
        </svg>"#,
        s = size, c = stroke_hex,
    );
    rasterize_svg_bytes(svg.as_bytes(), size)
}

/// SVG inline do Lock — padlock simples cream stroke 1.5, fill none.
/// Path adaptado do spec luma-session-icons. Cor passada via stroke
/// inline (variável `{COLOR}` substituída no template).
pub fn lock_svg_inline(stroke_hex: &str, size: u32) -> Option<MemoryRenderBuffer> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="{s}" height="{s}">
            <g fill="none" stroke="{c}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <rect x="5" y="11" width="14" height="10" rx="1.5"/>
                <path d="M 8 11 V 7.5 a 4 4 0 0 1 8 0 V 11"/>
                <circle cx="12" cy="16" r="1.2" fill="{c}" stroke="none"/>
            </g>
        </svg>"#,
        s = size,
        c = stroke_hex,
    );
    rasterize_svg_bytes(svg.as_bytes(), size)
}
