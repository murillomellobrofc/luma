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

/// Variantes dos ícones de sessão Luma Original (H32.7.9).
#[derive(Debug, Clone, Copy)]
pub enum SessionIcon {
    /// Cadeado fechado.
    Lock,
    /// Lua crescente.
    Sleep,
    /// Seta circular incompleta.
    Restart,
    /// Símbolo de power (linha vertical + arco aberto embaixo).
    Power,
}

/// Desenha um ícone de sessão `hit_area×hit_area` com a forma `icon` em
/// `color` (alpha embutida). Aproximações simples em pixel art com
/// stroke ~2px (a spec pede 1.5px round-cap; sem antialias completo).
/// H32.7.9.
pub fn make_session_icon(
    hit_area: i32,
    icon_size: i32,
    icon: SessionIcon,
    color: Color32F,
) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (hit_area * hit_area * 4) as usize];
    let pad = (hit_area - icon_size) / 2;
    let cx = hit_area / 2;
    let cy = hit_area / 2;
    let stroke = 2i32;

    match icon {
        SessionIcon::Lock => {
            // Corpo do cadeado: rect outline 60% × 50% do icon_size,
            // base alinhada ao bottom do icon.
            let body_w = (icon_size as f32 * 0.65) as i32;
            let body_h = (icon_size as f32 * 0.55) as i32;
            let body_x = cx - body_w / 2;
            let body_y = pad + icon_size - body_h;
            // 4 lados do rect outline.
            // top
            for x in body_x..(body_x + body_w) {
                for dy in 0..stroke {
                    put_pixel_premul(&mut pixels, x, body_y + dy, hit_area, color);
                }
            }
            // bottom
            for x in body_x..(body_x + body_w) {
                for dy in 0..stroke {
                    put_pixel_premul(
                        &mut pixels,
                        x,
                        body_y + body_h - 1 - dy,
                        hit_area,
                        color,
                    );
                }
            }
            // sides
            for y in body_y..(body_y + body_h) {
                for dx in 0..stroke {
                    put_pixel_premul(&mut pixels, body_x + dx, y, hit_area, color);
                    put_pixel_premul(
                        &mut pixels,
                        body_x + body_w - 1 - dx,
                        y,
                        hit_area,
                        color,
                    );
                }
            }
            // Arco (parte de cima do cadeado): semi-círculo acima do
            // corpo, raio = body_w/2 - 1.
            let arc_r = (body_w as f32 * 0.30) as i32;
            let arc_cy = body_y;
            for theta_deg in 180..=360 {
                let theta = (theta_deg as f32).to_radians();
                for t in 0..stroke {
                    let r = arc_r - t;
                    let x = cx + (r as f32 * theta.cos()).round() as i32;
                    let y = arc_cy + (r as f32 * theta.sin()).round() as i32;
                    if x >= 0 && x < hit_area && y >= 0 && y < hit_area {
                        put_pixel_premul(&mut pixels, x, y, hit_area, color);
                    }
                }
            }
        }
        SessionIcon::Sleep => {
            // Lua crescente PREENCHIDA: círculo cheio menos círculo
            // deslocado (cria o crescente). Diferencia VISUALMENTE do
            // Restart (que é arco aberto com flecha). H32.7.fix.polish.2.
            let r = (icon_size as f32 * 0.45) as i32;
            let r_sq = r * r;
            let cut_cx = cx + (r as f32 * 0.5) as i32;
            let cut_cy = cy - (r as f32 * 0.1) as i32;
            let cut_r = (r as f32 * 0.92) as i32;
            let cut_r_sq = cut_r * cut_r;
            for y in 0..hit_area {
                for x in 0..hit_area {
                    let dx = x - cx;
                    let dy = y - cy;
                    if dx * dx + dy * dy > r_sq {
                        continue;
                    }
                    let cdx = x - cut_cx;
                    let cdy = y - cut_cy;
                    if cdx * cdx + cdy * cdy <= cut_r_sq {
                        continue;
                    }
                    put_pixel_premul(&mut pixels, x, y, hit_area, color);
                }
            }
        }
        SessionIcon::Restart => {
            // Arco circular 270° + seta na ponta superior.
            let r = (icon_size / 2) - 1;
            // arc 0..270 graus (do leste, sentido horário)
            for theta_deg in -90..200 {
                let theta = (theta_deg as f32).to_radians();
                for t in 0..stroke {
                    let rr = r - t;
                    let x = cx + (rr as f32 * theta.cos()).round() as i32;
                    let y = cy + (rr as f32 * theta.sin()).round() as i32;
                    if x >= 0 && x < hit_area && y >= 0 && y < hit_area {
                        put_pixel_premul(&mut pixels, x, y, hit_area, color);
                    }
                }
            }
            // Seta no topo (pequeno triângulo apontando pra direita)
            let arrow_x = cx;
            let arrow_y = cy - r;
            for d in 0..4 {
                for s in 0..stroke {
                    let x = arrow_x + d - s;
                    let y = arrow_y - d;
                    if x >= 0 && x < hit_area && y >= 0 && y < hit_area {
                        put_pixel_premul(&mut pixels, x, y, hit_area, color);
                    }
                    let y2 = arrow_y + d;
                    if x >= 0 && x < hit_area && y2 >= 0 && y2 < hit_area {
                        put_pixel_premul(&mut pixels, x, y2, hit_area, color);
                    }
                }
            }
        }
        SessionIcon::Power => {
            // Linha vertical no topo do centro
            let line_top = pad + 1;
            let line_bottom = cy + 1;
            for y in line_top..=line_bottom {
                for s in 0..stroke {
                    let x = cx - stroke / 2 + s;
                    put_pixel_premul(&mut pixels, x, y, hit_area, color);
                }
            }
            // Arco aberto: parte do leste→sul→oeste passando pelo bottom.
            let r = (icon_size as f32 * 0.45) as i32;
            for theta_deg in -30..=210 {
                let theta = (theta_deg as f32).to_radians();
                for t in 0..stroke {
                    let rr = r - t;
                    let x = cx + (rr as f32 * theta.cos()).round() as i32;
                    let y = cy + (rr as f32 * theta.sin()).round() as i32;
                    if x >= 0 && x < hit_area && y >= 0 && y < hit_area {
                        put_pixel_premul(&mut pixels, x, y, hit_area, color);
                    }
                }
            }
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

/// Glyph ⌘ (Cmd/Apple) custom: quadrado outline + 4 pontos nos cantos
/// externos. Simula a forma clássica do glyph U+2318. H32.7.fix.27.
pub fn make_cmd_glyph(size: i32, color: Color32F) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let stroke = (size / 10).max(1);
    let inner_pad = size / 4;
    let sq_x = inner_pad;
    let sq_y = inner_pad;
    let sq_w = size - 2 * inner_pad;
    // Quadrado outline central.
    for x in sq_x..(sq_x + sq_w) {
        for s in 0..stroke {
            put_pixel_premul(&mut pixels, x, sq_y + s, size, color);
            put_pixel_premul(&mut pixels, x, sq_y + sq_w - 1 - s, size, color);
        }
    }
    for y in sq_y..(sq_y + sq_w) {
        for s in 0..stroke {
            put_pixel_premul(&mut pixels, sq_x + s, y, size, color);
            put_pixel_premul(&mut pixels, sq_x + sq_w - 1 - s, y, size, color);
        }
    }
    // 4 pontos circulares nos cantos externos (que vão "saindo" do quadrado).
    let dot_r = (size / 8).max(1);
    let dot_r_sq = dot_r * dot_r;
    let centers = [
        (inner_pad / 2, inner_pad / 2),
        (size - inner_pad / 2 - 1, inner_pad / 2),
        (inner_pad / 2, size - inner_pad / 2 - 1),
        (size - inner_pad / 2 - 1, size - inner_pad / 2 - 1),
    ];
    for &(cx, cy) in &centers {
        for y in (cy - dot_r).max(0)..=(cy + dot_r).min(size - 1) {
            for x in (cx - dot_r).max(0)..=(cx + dot_r).min(size - 1) {
                let dx = x - cx;
                let dy = y - cy;
                if dx * dx + dy * dy <= dot_r_sq {
                    put_pixel_premul(&mut pixels, x, y, size, color);
                }
            }
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

/// Ícone sol minimalista pra saudação "Bom dia" / "Boa tarde": círculo
/// central + 8 raios curtos ao redor. H32.7.fix.18.
pub fn make_sun_icon(size: i32, color: Color32F) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let cx = (size - 1) as f32 / 2.0;
    let cy = (size - 1) as f32 / 2.0;
    let inner_r = (size as f32) * 0.22;
    let inner_r_sq = inner_r * inner_r;
    // Círculo central preenchido.
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= inner_r_sq {
                put_pixel_premul(&mut pixels, x, y, size, color);
            }
        }
    }
    // 8 raios: linhas curtas saindo do meio em direções 0°, 45°, 90°, ...
    let ray_inner = (size as f32) * 0.35;
    let ray_outer = (size as f32) * 0.48;
    let stroke = (size / 10).max(1);
    for k in 0..8 {
        let angle = (k as f32) * std::f32::consts::PI / 4.0;
        let dx = angle.cos();
        let dy = angle.sin();
        let steps = ((ray_outer - ray_inner) * 2.0) as i32 + 1;
        for s in 0..=steps {
            let t = ray_inner + (s as f32) * 0.5;
            let px = (cx + dx * t).round() as i32;
            let py = (cy + dy * t).round() as i32;
            for sx in 0..stroke {
                for sy in 0..stroke {
                    let xx = px + sx;
                    let yy = py + sy;
                    if xx >= 0 && xx < size && yy >= 0 && yy < size {
                        put_pixel_premul(&mut pixels, xx, yy, size, color);
                    }
                }
            }
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

/// Ícone lua minimalista pra "Boa noite": círculo cheio menos um círculo
/// deslocado (cria crescente). H32.7.fix.18.
pub fn make_moon_icon(size: i32, color: Color32F) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let cx = (size - 1) as f32 / 2.0;
    let cy = (size - 1) as f32 / 2.0;
    let r = (size as f32) * 0.45;
    let r_sq = r * r;
    // Círculo "subtraído" desviado pra direita pra criar crescente.
    let cut_cx = cx + r * 0.5;
    let cut_cy = cy - r * 0.1;
    let cut_r = r * 0.92;
    let cut_r_sq = cut_r * cut_r;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy > r_sq {
                continue;
            }
            let cdx = x as f32 - cut_cx;
            let cdy = y as f32 - cut_cy;
            if cdx * cdx + cdy * cdy <= cut_r_sq {
                continue;
            }
            put_pixel_premul(&mut pixels, x, y, size, color);
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

/// Chevron `>` apontando pra direita: 2 traços diagonais formando uma
/// seta. Stroke proporcional a `size`. H32.7.fix.17.
pub fn make_chevron_right(size: i32, color: Color32F) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let stroke = (size / 8).max(2);
    let pad_x = size / 4;
    let pad_y = size / 4;
    let tip_x = size - pad_x - 1;
    let tip_y = size / 2;
    // Stroke superior: (pad_x, pad_y) → (tip_x, tip_y)
    draw_thick_line(&mut pixels, size, pad_x, pad_y, tip_x, tip_y, stroke, color);
    // Stroke inferior: (pad_x, size - pad_y - 1) → (tip_x, tip_y)
    draw_thick_line(
        &mut pixels,
        size,
        pad_x,
        size - pad_y - 1,
        tip_x,
        tip_y,
        stroke,
        color,
    );
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (size, size),
        1,
        Transform::Normal,
        None,
    )
}

/// Ícone de pasta minimalista pros lugares: retângulo principal + aba
/// tab pequena no canto superior esquerdo. Stroke proporcional a `size`.
/// H32.7.fix.polish.3.
pub fn make_folder_icon(size: i32, color: Color32F) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let stroke = (size / 12).max(1);
    let pad = size / 6;
    let body_x0 = pad;
    let body_y0 = pad + size / 6;
    let body_x1 = size - pad - 1;
    let body_y1 = size - pad - 1;
    // Tab (aba) no topo-esquerdo: linha curta acima do body.
    let tab_w = (size - 2 * pad) / 2 - 1;
    let tab_y = body_y0 - size / 8;
    // Top do body (com notch onde a tab encaixa).
    for x in body_x0..=body_x1 {
        for s in 0..stroke {
            put_pixel_premul(&mut pixels, x, body_y0 + s, size, color);
        }
    }
    // Bottom do body.
    for x in body_x0..=body_x1 {
        for s in 0..stroke {
            put_pixel_premul(&mut pixels, x, body_y1 - s, size, color);
        }
    }
    // Lados.
    for y in body_y0..=body_y1 {
        for s in 0..stroke {
            put_pixel_premul(&mut pixels, body_x0 + s, y, size, color);
            put_pixel_premul(&mut pixels, body_x1 - s, y, size, color);
        }
    }
    // Tab horizontal no topo + diagonais.
    for x in body_x0..(body_x0 + tab_w) {
        for s in 0..stroke {
            put_pixel_premul(&mut pixels, x, tab_y + s, size, color);
        }
    }
    // Vertical esquerda da tab.
    for y in tab_y..body_y0 {
        for s in 0..stroke {
            put_pixel_premul(&mut pixels, body_x0 + s, y, size, color);
        }
    }
    // Diagonal da tab descendo até o body.
    let dx_steps = body_y0 - tab_y;
    for d in 0..=dx_steps {
        let x = body_x0 + tab_w + d;
        let y = tab_y + d;
        for s in 0..stroke {
            if x + s < size && y < size {
                put_pixel_premul(&mut pixels, x + s, y, size, color);
            }
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

/// Quadradinho `size×size` só com borda (interior alpha=0). Stroke de
/// 1px no perímetro com cantos arredondados de raio `radius`. H32.7.fix.13.
pub fn make_rounded_border(size: i32, radius: i32, color: Color32F) -> MemoryRenderBuffer {
    let r_sq = radius * radius;
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            if !is_inside_rounded(x, y, size, size, radius, r_sq) {
                continue;
            }
            let on_straight = (x == 0 && y >= radius && y < size - radius)
                || (x == size - 1 && y >= radius && y < size - radius)
                || (y == 0 && x >= radius && x < size - radius)
                || (y == size - 1 && x >= radius && x < size - radius);
            let on_curve = is_curve_border(x, y, size, size, radius);
            if on_straight || on_curve {
                put_pixel_premul(&mut pixels, x, y, size, color);
            }
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

/// Ícone hamburger ≡ pro botão "Todos os apps": 3 linhas horizontais
/// igualmente espaçadas em um quadrado `size×size`. H32.7.fix.9.
pub fn make_hamburger_icon(size: i32, color: Color32F) -> MemoryRenderBuffer {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let stroke = (size / 8).max(2);
    let inner_pad = size / 6;
    let line_w = size - 2 * inner_pad;
    let total_h = 3 * stroke + 2 * stroke;
    let start_y = (size - total_h) / 2;
    for i in 0..3 {
        let y0 = start_y + i * 2 * stroke;
        for y in y0..(y0 + stroke) {
            for x in inner_pad..(inner_pad + line_w) {
                put_pixel_premul(&mut pixels, x, y, size, color);
            }
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

/// Especificação de layout pro menu inteiro pintado em uma textura única.
/// Coordenadas relativas ao topo-esquerdo do menu.
pub struct MenuPanelSpec {
    pub width: i32,
    pub height: i32,
    pub corner_radius: i32,
    /// Onde a coluna direita começa (x). Antes disso = coluna esquerda.
    pub left_col_width: i32,
    /// Altura do header user (y < user_header_height na coluna direita).
    pub user_header_height: i32,
    /// Faixa session no fundo (y >= height - session_bar_height).
    pub session_bar_height: i32,
    /// Faixa search acima da session (y entre search_y e session_y).
    pub search_bar_height: i32,
}

pub struct MenuPanelColors {
    pub bg: Color32F,
    pub bg_right: Color32F,
    pub header_bar: Color32F,
    pub session_bar: Color32F,
    pub search_bar: Color32F,
    pub border: Color32F,
}

/// Constrói uma textura RGBA do menu inteiro: cores das regiões (esquerda,
/// direita, header user, session bar, search bar) + bordas externas 1px +
/// cantos arredondados (alpha=0 fora do raio). Substitui vários
/// SolidColorBuffer separados por uma única textura. Pré-calculada uma vez
/// no init do MenuState.
pub fn make_rounded_menu_panel(
    spec: &MenuPanelSpec,
    colors: &MenuPanelColors,
) -> MemoryRenderBuffer {
    let w = spec.width;
    let h = spec.height;
    let r = spec.corner_radius;
    let r_sq = r * r;
    let session_y = h - spec.session_bar_height;
    let search_y = session_y - spec.search_bar_height;

    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            if !is_inside_rounded(x, y, w, h, r, r_sq) {
                continue;
            }
            // Borda 1px nas 4 laterais (apenas onde não tá no canto curvado).
            let on_straight_border = (x == 0 && y >= r && y < h - r)
                || (x == w - 1 && y >= r && y < h - r)
                || (y == 0 && x >= r && x < w - r)
                || (y == h - 1 && x >= r && x < w - r);
            // Anel da curva (1px no perímetro do canto arredondado).
            let on_curve_border = is_curve_border(x, y, w, h, r);

            let region_color = if y >= session_y {
                colors.session_bar
            } else if y >= search_y {
                colors.search_bar
            } else if x >= spec.left_col_width && y < spec.user_header_height {
                colors.header_bar
            } else if x >= spec.left_col_width {
                colors.bg_right
            } else {
                colors.bg
            };

            let color = if on_straight_border || on_curve_border {
                colors.border
            } else {
                region_color
            };

            put_pixel_premul(&mut pixels, x, y, w, color);
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

/// Detecta o anel de 1px no perímetro do canto arredondado: pixel está
/// dentro do raio mas o vizinho radial 1px pra fora estaria fora.
fn is_curve_border(x: i32, y: i32, w: i32, h: i32, r: i32) -> bool {
    if r <= 0 {
        return false;
    }
    let in_left = x < r;
    let in_right = x >= w - r;
    let in_top = y < r;
    let in_bottom = y >= h - r;
    if !((in_left || in_right) && (in_top || in_bottom)) {
        return false;
    }
    let cx = if in_left { r } else { w - r - 1 };
    let cy = if in_top { r } else { h - r - 1 };
    let dx = x - cx;
    let dy = y - cy;
    let d_sq = dx * dx + dy * dy;
    let r_sq = r * r;
    let r_inner = r - 1;
    let r_inner_sq = r_inner * r_inner;
    d_sq <= r_sq && d_sq > r_inner_sq
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
