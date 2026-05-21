//! Drop shadow das janelas (9-slice + gaussian CPU). H_deco.2-3.
//!
//! Renderiza atrás de cada janela uma sombra suave (blur via gaussian
//! pré-calculado em CPU). Compõe via 9-slice: 4 cantos pré-renderizados
//! `blur×blur` + 4 edges (1×blur ou blur×1) esticados pelas laterais.
//! Caso a janela esteja fora de foco, usa um conjunto de tiles separado
//! com blur/opacity reduzidos.

use std::sync::OnceLock;

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
                Kind,
            },
            Color32F, ImportMem, Renderer,
        },
    },
    utils::{Logical, Physical, Point, Size, Transform},
};

/// Tiles de uma única intensidade de sombra (4 cantos + 4 edges).
pub struct ShadowTiles {
    pub blur: i32,
    pub corner_tl: MemoryRenderBuffer,
    pub corner_tr: MemoryRenderBuffer,
    pub corner_bl: MemoryRenderBuffer,
    pub corner_br: MemoryRenderBuffer,
    pub edge_top: MemoryRenderBuffer,
    pub edge_bot: MemoryRenderBuffer,
    pub edge_left: MemoryRenderBuffer,
    pub edge_right: MemoryRenderBuffer,
}

/// Cache global das sombras (ativa + inativa). Construído lazy. H_deco.2.
pub struct WindowShadows {
    pub focused: ShadowTiles,
    pub unfocused: ShadowTiles,
}

static SHADOWS: OnceLock<WindowShadows> = OnceLock::new();

pub fn shadows() -> &'static WindowShadows {
    SHADOWS.get_or_init(WindowShadows::build)
}

impl WindowShadows {
    fn build() -> Self {
        let theme = crate::theme::current();
        let w = &theme.window;
        WindowShadows {
            focused: ShadowTiles::build(w.shadow_blur, w.shadow_opacity, w.shadow_color),
            unfocused: ShadowTiles::build(
                w.shadow_blur_inactive,
                w.shadow_opacity_inactive,
                w.shadow_color,
            ),
        }
    }
}

impl ShadowTiles {
    fn build(blur: i32, opacity: f32, color: Color32F) -> Self {
        let blur = blur.max(1);
        let sigma = (blur as f32) / 2.5; // controla suavidade do decay
        let sigma_sq = (sigma * sigma).max(0.001);
        // Cor base do shadow em premul (BGRA little-endian).
        let r = (color.r() * 255.0) as u8;
        let g = (color.g() * 255.0) as u8;
        let b = (color.b() * 255.0) as u8;

        // Helper: alpha (0..1) baseado na distância à beira da janela.
        // dist=0 (junto à janela) → alpha=opacity. dist=blur → alpha~0.
        let alpha_at = |dist: f32| -> f32 {
            let d = dist.max(0.0);
            (-(d * d) / (2.0 * sigma_sq)).exp() * opacity
        };

        // Pinta 1 pixel premultiplicado em (x,y) do buffer w×h.
        let put = |buf: &mut [u8], w: i32, x: i32, y: i32, a: f32| {
            let a = (a.clamp(0.0, 1.0) * 255.0) as u8;
            let idx = ((y * w + x) * 4) as usize;
            let pr = ((b as u16 * a as u16) / 255) as u8;
            let pg = ((g as u16 * a as u16) / 255) as u8;
            let pb = ((r as u16 * a as u16) / 255) as u8;
            buf[idx] = pr;
            buf[idx + 1] = pg;
            buf[idx + 2] = pb;
            buf[idx + 3] = a;
        };

        // 4 cantos: tile blur×blur. (blur-1, blur-1) = pixel "junto à
        // janela"; (0,0) = pixel "longe". Pra cantos: dist = euclid.
        let mk_corner = |kind_x: i32, kind_y: i32| -> MemoryRenderBuffer {
            // kind_x, kind_y ∈ {-1, +1} indicam direção do canto:
            //   tl: (-1, -1)   tr: (+1, -1)
            //   bl: (-1, +1)   br: (+1, +1)
            // O pixel "junto à janela" é o canto interno do tile.
            let mut pixels = vec![0u8; (blur * blur * 4) as usize];
            for y in 0..blur {
                for x in 0..blur {
                    // distancia ao canto interno (0..blur-1)
                    let inner_x = if kind_x < 0 { blur - 1 - x } else { x };
                    let inner_y = if kind_y < 0 { blur - 1 - y } else { y };
                    let dx = inner_x as f32;
                    let dy = inner_y as f32;
                    let dist = (dx * dx + dy * dy).sqrt();
                    put(&mut pixels, blur, x, y, alpha_at(dist));
                }
            }
            MemoryRenderBuffer::from_slice(
                &pixels,
                Fourcc::Argb8888,
                (blur, blur),
                1,
                Transform::Normal,
                None,
            )
        };

        // Edge horizontal: blur×1 ou 1×blur. Esticado nas dimensões.
        // edge_top: tile vertical 1×blur, alpha decay do bottom (junto)
        // pro top (longe). Esticado horizontalmente pra blur×width.
        let mk_edge_vert = |bottom_strong: bool| -> MemoryRenderBuffer {
            let mut pixels = vec![0u8; (blur * 4) as usize];
            for y in 0..blur {
                let dist = if bottom_strong {
                    (blur - 1 - y) as f32
                } else {
                    y as f32
                };
                put(&mut pixels, 1, 0, y, alpha_at(dist));
            }
            MemoryRenderBuffer::from_slice(
                &pixels,
                Fourcc::Argb8888,
                (1, blur),
                1,
                Transform::Normal,
                None,
            )
        };
        let mk_edge_horiz = |right_strong: bool| -> MemoryRenderBuffer {
            let mut pixels = vec![0u8; (blur * 4) as usize];
            for x in 0..blur {
                let dist = if right_strong {
                    (blur - 1 - x) as f32
                } else {
                    x as f32
                };
                put(&mut pixels, blur, x, 0, alpha_at(dist));
            }
            MemoryRenderBuffer::from_slice(
                &pixels,
                Fourcc::Argb8888,
                (blur, 1),
                1,
                Transform::Normal,
                None,
            )
        };

        ShadowTiles {
            blur,
            corner_tl: mk_corner(-1, -1),
            corner_tr: mk_corner(1, -1),
            corner_bl: mk_corner(-1, 1),
            corner_br: mk_corner(1, 1),
            edge_top: mk_edge_vert(true),
            edge_bot: mk_edge_vert(false),
            edge_left: mk_edge_horiz(true),
            edge_right: mk_edge_horiz(false),
        }
    }
}

/// Constrói os 8 elementos do shadow 9-slice em volta de uma janela.
/// `window_origin` = canto superior-esquerdo da janela em coords físicas.
/// `window_size` = tamanho total da janela (header + body + bordas).
/// Retorna Vec na ordem: cantos primeiro (= topo do z-stack pra ficar
/// na frente), depois edges. Mesmo conjunto pra qualquer janela —
/// só o estado focused decide qual ShadowTiles usar. H_deco.2.
pub fn shadow_elements_at<R>(
    renderer: &mut R,
    window_origin: Point<i32, Physical>,
    window_size: Size<i32, Logical>,
    focused: bool,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: Renderer + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let tiles = if focused {
        &shadows().focused
    } else {
        &shadows().unfocused
    };
    let blur = tiles.blur;
    let theme = crate::theme::current();
    let offset_y = theme.window.shadow_offset_y;
    let w = window_size.w;
    let h = window_size.h;
    let mut out = Vec::with_capacity(8);

    // Pontos de referência (em coords físicas). offset_y desloca tudo
    // pra baixo (sombra mais forte na parte inferior).
    let x0 = window_origin.x;
    let y0 = window_origin.y + offset_y;
    let x1 = window_origin.x + w;
    let y1 = window_origin.y + h + offset_y;

    // 4 cantos (tile blur×blur).
    let corners = [
        (&tiles.corner_tl, Point::<i32, Physical>::from((x0 - blur, y0 - blur))),
        (&tiles.corner_tr, Point::<i32, Physical>::from((x1, y0 - blur))),
        (&tiles.corner_bl, Point::<i32, Physical>::from((x0 - blur, y1))),
        (&tiles.corner_br, Point::<i32, Physical>::from((x1, y1))),
    ];
    for (buf, pos) in corners {
        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos.to_f64(),
            buf,
            None,
            None,
            None,
            Kind::Unspecified,
        ) {
            out.push(elem);
        }
    }
    // 4 edges. dst_size estica.
    let edges = [
        // top: 1×blur tile esticado em w×blur
        (&tiles.edge_top, Point::<i32, Physical>::from((x0, y0 - blur)),
         Size::<i32, Logical>::from((w, blur))),
        // bottom: idem virado
        (&tiles.edge_bot, Point::<i32, Physical>::from((x0, y1)),
         Size::<i32, Logical>::from((w, blur))),
        // left: blur×1 tile esticado em blur×h
        (&tiles.edge_left, Point::<i32, Physical>::from((x0 - blur, y0)),
         Size::<i32, Logical>::from((blur, h))),
        // right: idem virado
        (&tiles.edge_right, Point::<i32, Physical>::from((x1, y0)),
         Size::<i32, Logical>::from((blur, h))),
    ];
    for (buf, pos, dst) in edges {
        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos.to_f64(),
            buf,
            None,
            None,
            Some(dst),
            Kind::Unspecified,
        ) {
            out.push(elem);
        }
    }
    out
}
