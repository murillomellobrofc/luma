use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{element::memory::MemoryRenderBuffer, Color32F},
    },
    utils::Transform,
};

/// Estilo dos window controls do Luma — ícones puros sem background
/// (H32.5.12). Hit area é o tamanho do buffer (transparente), ícone
/// `icon_size` centralizado dentro com cor `color` (premultiplicado
/// com o alpha embutido na Color32F).
#[derive(Debug, Clone, Copy)]
pub enum WindowControlIcon {
    /// Linha horizontal na parte inferior do hit area.
    Minimize,
    /// Quadrado outline (não preenchido) centralizado.
    Maximize,
    /// Dois traços diagonais formando ×.
    Close,
}

/// Pré-renderiza um window control "Luma original": buffer transparente
/// `hit_area×hit_area` com o ícone vetorial desenhado em `color`. Sem
/// background — H32.5.12. Stroke ~2px (aproximação de 1.5px com
/// antialiasing limitado).
pub fn make_window_control(
    hit_area: i32,
    icon_size: i32,
    icon: WindowControlIcon,
    color: Color32F,
) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (hit_area * hit_area * 4) as usize];
    let pad = (hit_area - icon_size) / 2;
    let stroke = 2i32;

    match icon {
        WindowControlIcon::Minimize => {
            // Linha horizontal na PARTE INFERIOR do hit area (não do
            // icon area) — fica próximo da base, estilo "barrinha".
            let y_line = hit_area - pad - stroke; // (hit_area - 6) - 2
            for y in y_line..(y_line + stroke) {
                for x in pad..(pad + icon_size) {
                    put_pixel_premul(&mut pixels, x, y, hit_area, color);
                }
            }
        }
        WindowControlIcon::Maximize => {
            // Quadrado outline 12x12 centralizado, bordas ~stroke px.
            let x0 = pad;
            let y0 = pad;
            let x1 = pad + icon_size - 1;
            let y1 = pad + icon_size - 1;
            // Top/bottom
            for y in y0..(y0 + stroke) {
                for x in x0..=x1 {
                    put_pixel_premul(&mut pixels, x, y, hit_area, color);
                }
            }
            for y in (y1 - stroke + 1)..=y1 {
                for x in x0..=x1 {
                    put_pixel_premul(&mut pixels, x, y, hit_area, color);
                }
            }
            // Left/right
            for y in y0..=y1 {
                for x in x0..(x0 + stroke) {
                    put_pixel_premul(&mut pixels, x, y, hit_area, color);
                }
                for x in (x1 - stroke + 1)..=x1 {
                    put_pixel_premul(&mut pixels, x, y, hit_area, color);
                }
            }
        }
        WindowControlIcon::Close => {
            // Dois traços diagonais formando × (Bresenham simples,
            // espessura `stroke` via desenho de quadrados ao longo).
            let x0 = pad;
            let y0 = pad;
            let x1 = pad + icon_size - 1;
            let y1 = pad + icon_size - 1;
            draw_thick_line(&mut pixels, hit_area, x0, y0, x1, y1, stroke, color);
            draw_thick_line(&mut pixels, hit_area, x1, y0, x0, y1, stroke, color);
        }
    }

    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (hit_area, hit_area),
        1,
        Transform::Normal,
        None,
    )
}

/// Linha grossa via Bresenham + quadrado de `thickness×thickness` pixels
/// em cada ponto. Sem antialiasing (linecap quadrado em vez de round).
fn draw_thick_line(
    pixels: &mut [u8],
    width: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    color: Color32F,
) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    let half = thickness / 2;
    loop {
        for ty in -half..=half {
            for tx in -half..=half {
                let px = x + tx;
                let py = y + ty;
                if px >= 0 && px < width && py >= 0 && py < width {
                    put_pixel_premul(pixels, px, py, width, color);
                }
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Cores pra desenhar o mini logo Luma dentro do botão launcher.
/// Beam (haste vertical) usa gradient bottom→top; foot e dot são sólidos.
pub struct LumaLogoColors {
    /// Cor da base do beam (parte de baixo da haste).
    pub beam_bottom: Color32F,
    /// Cor do topo do beam (parte de cima da haste).
    pub beam_top: Color32F,
    /// Cor do foot (base horizontal do "L").
    pub foot: Color32F,
    /// Cor do ponto luminoso na intersecção beam↔foot.
    pub dot: Color32F,
}

/// Pré-renderiza o ícone do launcher do luma: container quadrado
/// `container_size`×`container_size` com cor `container_color` e cantos
/// arredondados `container_radius`, com o mini logo Luma desenhado no
/// centro (beam vertical com gradient + foot horizontal + dot luminoso).
///
/// Logo derivado do `assets/branding/luma-logo.svg` em pixel art adaptado.
/// Argb8888 premultiplicado.
pub fn make_luma_launcher(
    container_size: i32,
    container_color: Color32F,
    container_radius: i32,
    colors: &LumaLogoColors,
) -> MemoryRenderBuffer {
    let s = container_size;
    let mut pixels = vec![0u8; (s * s * 4) as usize];
    let r_sq = container_radius * container_radius;

    // Pinta container (mesmo padrão do make_rounded_panel).
    for y in 0..s {
        for x in 0..s {
            if !is_inside_rounded(x, y, s, s, container_radius, r_sq) {
                continue;
            }
            put_pixel_premul(&mut pixels, x, y, s, container_color);
        }
    }

    // Logo ocupa ~50% do container, centralizado.
    // Geometria (em coords relativas ao container 32x32, escala se outro):
    //   beam: vertical 4×16 começando em (10, 8)
    //   foot: horizontal 16×4 começando em (10, 20)
    //   dot:  círculo r=2 em (12, 10) (topo do beam)
    // Pra container_size != 32, escala proporcional.
    let scale = s as f32 / 32.0;
    let beam_w = ((4.0 * scale).round() as i32).max(2);
    let beam_h = ((16.0 * scale).round() as i32).max(4);
    let beam_x = ((10.0 * scale).round() as i32).max(2);
    let beam_y = ((8.0 * scale).round() as i32).max(2);
    let foot_w = ((16.0 * scale).round() as i32).max(4);
    let foot_h = ((4.0 * scale).round() as i32).max(2);
    let foot_x = beam_x;
    let foot_y = beam_y + beam_h - foot_h;
    let dot_cx = beam_x + beam_w / 2;
    let dot_cy = beam_y + (3.0 * scale).round() as i32;
    let dot_r = ((2.0 * scale).round() as i32).max(1);

    // Beam com gradient vertical (bottom→top).
    for y in beam_y..(beam_y + beam_h).min(s) {
        let t = if beam_h <= 1 {
            0.0
        } else {
            1.0 - (y - beam_y) as f32 / (beam_h - 1) as f32
        };
        let r = colors.beam_bottom.r() + (colors.beam_top.r() - colors.beam_bottom.r()) * t;
        let g = colors.beam_bottom.g() + (colors.beam_top.g() - colors.beam_bottom.g()) * t;
        let b = colors.beam_bottom.b() + (colors.beam_top.b() - colors.beam_bottom.b()) * t;
        let a = colors.beam_bottom.a() + (colors.beam_top.a() - colors.beam_bottom.a()) * t;
        let row_color = Color32F::new(r, g, b, a);
        for x in beam_x..(beam_x + beam_w).min(s) {
            put_pixel_premul(&mut pixels, x, y, s, row_color);
        }
    }
    // Foot horizontal (sólido).
    for y in foot_y..(foot_y + foot_h).min(s) {
        for x in foot_x..(foot_x + foot_w).min(s) {
            put_pixel_premul(&mut pixels, x, y, s, colors.foot);
        }
    }
    // Dot (círculo cheio centralizado).
    let dot_r_sq = dot_r * dot_r;
    for dy in -dot_r..=dot_r {
        for dx in -dot_r..=dot_r {
            if dx * dx + dy * dy > dot_r_sq {
                continue;
            }
            let x = dot_cx + dx;
            let y = dot_cy + dy;
            if x < 0 || x >= s || y < 0 || y >= s {
                continue;
            }
            put_pixel_premul(&mut pixels, x, y, s, colors.dot);
        }
    }

    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (s, s),
        1,
        Transform::Normal,
        None,
    )
}

/// Desenha um ícone lupa (search) `size×size` em `color`. Round outline
/// pro corpo + linha diagonal pro cabo. Stroke ~2px (aproximação 1.5px
/// da spec). Argb8888 premultiplicado, alpha=0 fora. H32.7.8.
pub fn make_search_icon(size: i32, color: Color32F) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    // Lupa ocupa ~60% do canto superior-esquerdo. Cabo desce pra
    // canto inferior direito.
    let cx = (size as f32 * 0.40) as i32;
    let cy = (size as f32 * 0.40) as i32;
    let r = (size as f32 * 0.30) as i32; // raio do círculo
    // Circle outline 2px de espessura.
    let r_out_sq = r * r;
    let r_in = (r - 2).max(1);
    let r_in_sq = r_in * r_in;
    for y in 0..size {
        for x in 0..size {
            let dx = x - cx;
            let dy = y - cy;
            let d_sq = dx * dx + dy * dy;
            if d_sq <= r_out_sq && d_sq >= r_in_sq {
                put_pixel_premul(&mut pixels, x, y, size, color);
            }
        }
    }
    // Cabo: linha do ponto (cx + r/√2, cy + r/√2) até quase o canto
    // inferior direito. ~45 graus.
    let start_x = cx + ((r as f32) * 0.7) as i32;
    let start_y = cy + ((r as f32) * 0.7) as i32;
    let end_x = size - 2;
    let end_y = size - 2;
    draw_thick_line(&mut pixels, size, start_x, start_y, end_x, end_y, 2, color);
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (size, size),
        1,
        Transform::Normal,
        None,
    )
}

/// Desenha um avatar circular `size×size` com gradient vertical entre
/// `top` (topo) e `bottom` (base). Fora do círculo: alpha=0. Argb8888
/// premultiplicado. Usado no profile bar do menu (H32.7.4).
pub fn make_avatar_circle(size: i32, top: Color32F, bottom: Color32F) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let cx = (size - 1) as f32 / 2.0;
    let cy = (size - 1) as f32 / 2.0;
    let r = (size / 2) as f32;
    let r_inner = (r - 1.0).max(0.0);
    let r_sq = r * r;
    let r_inner_sq = r_inner * r_inner;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d_sq = dx * dx + dy * dy;
            if d_sq > r_sq {
                continue; // fora do círculo
            }
            // Antialias 1px no perímetro: alpha vai de 1.0 no centro
            // pra 0.0 no raio externo.
            let edge_alpha = if d_sq > r_inner_sq {
                let d = d_sq.sqrt();
                ((r - d).max(0.0)).min(1.0)
            } else {
                1.0
            };
            // Gradient vertical: t=0 no topo, t=1 na base.
            let t = if size <= 1 {
                0.0
            } else {
                y as f32 / (size - 1) as f32
            };
            let cr = top.r() + (bottom.r() - top.r()) * t;
            let cg = top.g() + (bottom.g() - top.g()) * t;
            let cb = top.b() + (bottom.b() - top.b()) * t;
            let ca = (top.a() + (bottom.a() - top.a()) * t) * edge_alpha;
            // BGRA premultiplicado.
            let pr = (cb * ca * 255.0).round() as u8;
            let pg = (cg * ca * 255.0).round() as u8;
            let pb = (cr * ca * 255.0).round() as u8;
            let pa = (ca * 255.0).round() as u8;
            let idx = ((y * size + x) * 4) as usize;
            pixels[idx] = pr;
            pixels[idx + 1] = pg;
            pixels[idx + 2] = pb;
            pixels[idx + 3] = pa;
        }
    }
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (size, size),
        1,
        Transform::Normal,
        None,
    )
}

/// Cria um MemoryRenderBuffer 1×h com gradient vertical entre `top` e
/// `bottom` (interpolação linear RGB + alpha). Pré-renderizado uma vez,
/// renderizado com `size = (output_w, h)` esticando horizontalmente
/// (cada coluna é igual, então o stretch é lossless).
///
/// Argb8888 premultiplicado pra blend correto sobre o wallpaper.
pub fn make_vertical_gradient(h: i32, top: Color32F, bottom: Color32F) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (h * 4) as usize];
    for y in 0..h {
        let t = if h <= 1 { 0.0 } else { y as f32 / (h - 1) as f32 };
        let r = top.r() + (bottom.r() - top.r()) * t;
        let g = top.g() + (bottom.g() - top.g()) * t;
        let b = top.b() + (bottom.b() - top.b()) * t;
        let a = top.a() + (bottom.a() - top.a()) * t;
        // BGRA premultiplicado.
        let pr = (b * a * 255.0).round() as u8;
        let pg = (g * a * 255.0).round() as u8;
        let pb = (r * a * 255.0).round() as u8;
        let pa = (a * 255.0).round() as u8;
        let idx = (y * 4) as usize;
        pixels[idx] = pr;
        pixels[idx + 1] = pg;
        pixels[idx + 2] = pb;
        pixels[idx + 3] = pa;
    }
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (1, h),
        1,
        Transform::Normal,
        None,
    )
}

/// Cria um MemoryRenderBuffer (Argb8888 premultiplicado) representando um
/// painel retangular w×h preenchido com `color`, com cantos arredondados
/// de raio `radius`. Pixels fora do raio nos 4 cantos ficam totalmente
/// transparentes (alpha=0), deixando o que estiver atrás aparecer.
pub fn make_rounded_panel(w: i32, h: i32, radius: i32, color: Color32F) -> MemoryRenderBuffer {
    let r = (color.r() * 255.0).round() as u8;
    let g = (color.g() * 255.0).round() as u8;
    let b = (color.b() * 255.0).round() as u8;
    let a = (color.a() * 255.0).round() as u8;
    // BGRA premultiplicado pro Argb8888 (mesmo padrão dos ícones).
    let pb = ((r as u16 * a as u16) / 255) as u8;
    let pg = ((g as u16 * a as u16) / 255) as u8;
    let pr = ((b as u16 * a as u16) / 255) as u8;

    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let r_sq = radius * radius;
    for y in 0..h {
        for x in 0..w {
            let inside = is_inside_rounded(x, y, w, h, radius, r_sq);
            let idx = ((y * w + x) * 4) as usize;
            if inside {
                pixels[idx] = pr;
                pixels[idx + 1] = pg;
                pixels[idx + 2] = pb;
                pixels[idx + 3] = a;
            }
            // else fica 0,0,0,0 (transparente).
        }
    }
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (w, h),
        1,
        Transform::Normal,
        None,
    )
}

// H33.taskbar.SVG: pixel art legado (draw_line, make_bluetooth_icon,
// make_bell_icon, make_dot, make_wifi_icon, make_volume_icon,
// make_battery_icon) removido — agora todos os ícones do tray e do
// show-desktop vêm de assets/branding/icons/*.svg via menu::icon_loader.


/// Header da janela: gradient vertical w×h com os 2 cantos TOPO
/// arredondados (sup-esq + sup-dir) raio `radius`. Pixels fora dos
/// arcos ficam alpha 0. Base reta. H_deco.9.
pub fn make_titlebar_bg(
    w: i32,
    h: i32,
    radius: i32,
    top: Color32F,
    bottom: Color32F,
) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let r_sq = radius * radius;
    let h1 = (h - 1).max(1) as f32;
    for y in 0..h {
        let t = y as f32 / h1;
        let r = top.r() + (bottom.r() - top.r()) * t;
        let g = top.g() + (bottom.g() - top.g()) * t;
        let b = top.b() + (bottom.b() - top.b()) * t;
        let a = top.a() + (bottom.a() - top.a()) * t;
        let color = Color32F::new(r, g, b, a);
        for x in 0..w {
            let inside = if y >= radius {
                true
            } else if x < radius {
                let dx = radius - 1 - x;
                let dy = radius - 1 - y;
                dx * dx + dy * dy <= r_sq
            } else if x >= w - radius {
                let dx = x - (w - radius);
                let dy = radius - 1 - y;
                dx * dx + dy * dy <= r_sq
            } else {
                true
            };
            if inside {
                put_pixel_premul(&mut pixels, x, y, w, color);
            }
        }
    }
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (w, h),
        1,
        Transform::Normal,
        None,
    )
}

/// Painel arredondado com borda 1px na cor `border_color` + alpha
/// `border_alpha`, e interior preenchido com `bg`. Usado pelo tooltip
/// da taskbar (H33.taskbar.6). Pixels fora do round-rect ficam alpha 0.
pub fn make_tooltip_bg(
    w: i32,
    h: i32,
    radius: i32,
    bg: Color32F,
    border: Color32F,
    border_alpha: f32,
) -> MemoryRenderBuffer {
    let border_premul = Color32F::new(
        border.r() * border_alpha,
        border.g() * border_alpha,
        border.b() * border_alpha,
        border_alpha,
    );
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let r_sq = radius * radius;
    let inner_radius = (radius - 1).max(0);
    let inner_r_sq = inner_radius * inner_radius;
    for y in 0..h {
        for x in 0..w {
            if !is_inside_rounded(x, y, w, h, radius, r_sq) {
                continue;
            }
            // Inner: 1px menor em cada lado.
            let inside_inner = x >= 1
                && x < (w - 1)
                && y >= 1
                && y < (h - 1)
                && is_inside_rounded(x - 1, y - 1, w - 2, h - 2, inner_radius, inner_r_sq);
            if inside_inner {
                put_pixel_premul(&mut pixels, x, y, w, bg);
            } else {
                put_pixel_premul(&mut pixels, x, y, w, border_premul);
            }
        }
    }
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (w, h),
        1,
        Transform::Normal,
        None,
    )
}

/// Fallback de ícone pra apps sem `Icon=` no .desktop ou cujo nome de ícone
/// não foi encontrado no tema XDG. Quadrado `size×size` com cantos
/// arredondados, gradient vertical `top→bottom`. H32.7.fix.4.
pub fn make_app_fallback_icon(
    size: i32,
    top: Color32F,
    bottom: Color32F,
    radius: i32,
) -> MemoryRenderBuffer {
    let r_sq = radius * radius;
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        let t = if size <= 1 { 0.0 } else { y as f32 / (size - 1) as f32 };
        let cr = top.r() + (bottom.r() - top.r()) * t;
        let cg = top.g() + (bottom.g() - top.g()) * t;
        let cb = top.b() + (bottom.b() - top.b()) * t;
        let ca = top.a() + (bottom.a() - top.a()) * t;
        for x in 0..size {
            if !is_inside_rounded(x, y, size, size, radius, r_sq) {
                continue;
            }
            put_pixel_premul(
                &mut pixels,
                x,
                y,
                size,
                Color32F::new(cr, cg, cb, ca),
            );
        }
    }
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (size, size),
        1,
        Transform::Normal,
        None,
    )
}

fn put_pixel_premul(pixels: &mut [u8], x: i32, y: i32, w: i32, color: Color32F) {
    let r = (color.r() * 255.0).round() as u8;
    let g = (color.g() * 255.0).round() as u8;
    let b = (color.b() * 255.0).round() as u8;
    let a = (color.a() * 255.0).round() as u8;
    // BGRA premultiplicado pro Argb8888.
    let pb = ((r as u16 * a as u16) / 255) as u8;
    let pg = ((g as u16 * a as u16) / 255) as u8;
    let pr = ((b as u16 * a as u16) / 255) as u8;
    let idx = ((y * w + x) * 4) as usize;
    pixels[idx] = pr;
    pixels[idx + 1] = pg;
    pixels[idx + 2] = pb;
    pixels[idx + 3] = a;
}

/// Checa se (x, y) está dentro de um retângulo w×h com cantos arredondados
/// de raio `r`. Pra pixels nos quatro cantos, faz um teste de distância
/// até o centro do quarto-de-círculo correspondente; pra pixels não-canto,
/// está sempre dentro.
fn is_inside_rounded(x: i32, y: i32, w: i32, h: i32, r: i32, r_sq: i32) -> bool {
    if r <= 0 {
        return true;
    }
    let in_left = x < r;
    let in_right = x >= w - r;
    let in_top = y < r;
    let in_bottom = y >= h - r;
    let cx = if in_left {
        r
    } else if in_right {
        w - r - 1
    } else {
        return true;
    };
    let cy = if in_top {
        r
    } else if in_bottom {
        h - r - 1
    } else {
        return true;
    };
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= r_sq
}
