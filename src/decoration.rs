use std::{
    cell::{RefCell, RefMut},
    sync::OnceLock,
};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
                solid::{SolidColorBuffer, SolidColorRenderElement},
                Kind,
            },
            Color32F, ImportMem, Renderer,
        },
    },
    desktop::Window,
    utils::{Logical, Physical, Point, Size, Transform},
};

/// Altura da barra de título desenhada pelo compositor. H_deco.4: 40px
/// (era 28). Combina com os botões 24x24 + 8px padding cada lado.
pub const HEADER_HEIGHT: i32 = 40;
/// Espessura da borda visível (esquerda/direita/inferior) ao redor do
/// conteúdo da janela.
pub const BORDER_THICKNESS: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderRegion {
    Background,
    Minimize,
    Maximize,
    Close,
}

pub struct WindowDecoration {
    /// Background da titlebar — gradient vertical Coffee→Black do
    /// `theme.window.titlebar_bg_top/bottom`. Memory texture
    /// pré-renderizada uma vez (H_deco.4), esticada na largura via
    /// dst_size no render.
    pub background: MemoryRenderBuffer,
    /// Linha 1px no topo do header (Amber alpha 0.4 default). Continuidade
    /// visual com a taskbar e o menu (H32.6.4).
    pub header_top_accent: SolidColorBuffer,
    pub left_border: SolidColorBuffer,
    pub right_border: SolidColorBuffer,
    pub bottom_border: SolidColorBuffer,
    /// Região do header atualmente em hover (ou `None` se o cursor não tá
    /// sobre esta janela). Usado pra render condicional do squircle
    /// no H32.5.13.
    pub hovered: Option<HeaderRegion>,
    /// Flag de foco da janela. Atualizado pelo winit.rs antes do render via
    /// `set_focused()`. Determina cor da borda externa (focada=amber,
    /// fora-de-foco=cream quase invisível). H32.6.3.
    pub focused: bool,
    /// Posição (no Space) ANTES de maximizar. `Some` indica que a janela
    /// está maximizada agora e que esse é o destino pra `unmaximize`.
    pub restore_location: Option<Point<i32, Logical>>,
    /// Janela minimizada: continua viva e no Space pra aparecer na taskbar,
    /// mas o LumaWindow reporta bbox/geometry vazios e render_elements
    /// retorna nada — fica visualmente escondida sem input.
    pub minimized: bool,
    /// Buffer com o título da janela rasterizado, junto com o texto que
    /// gerou ele. Buffer é regenerado quando o texto muda.
    pub title: Option<TitleCache>,
}

pub struct TitleCache {
    pub text: String,
    pub buffer: MemoryRenderBuffer,
    pub size: Size<i32, Logical>,
}

impl WindowDecoration {
    fn new() -> Self {
        let theme = crate::theme::current();
        // H_deco.4: bg da titlebar vira gradient vertical 1xHEADER pre-renderizado,
        // esticado na largura no render.
        let background = crate::gfx::make_vertical_gradient(
            HEADER_HEIGHT,
            theme.window.titlebar_bg_top,
            theme.window.titlebar_bg_bottom,
        );
        // H32.6.3: bordas externas iniciam em "fora de foco" (cream alpha
        // 0.08, quase invisível). Janela ganha cor amber ao receber foco
        // via set_focused() chamado pelo winit.rs.
        //
        // BORDER_THICKNESS=3 fisicamente (resize edges precisam disso).
        // Visualmente 3px de cor com alpha baixo dá a moldura sutil que
        // a spec pede (a spec mostra "1px"; em 3px com alpha 0.7 ainda
        // fica elegante e mantém o hit-test do resize).
        let unfocused = with_alpha(
            theme.colors.window_border_unfocused,
            theme.colors.window_border_unfocused_alpha,
        );
        // Linha âmbar 1px no topo do header (H32.6.4).
        let top_accent = with_alpha(
            theme.colors.window_header_top_accent,
            theme.colors.window_header_top_alpha,
        );
        Self {
            background,
            header_top_accent: SolidColorBuffer::new((0, 1), top_accent),
            left_border: SolidColorBuffer::new((BORDER_THICKNESS, 0), unfocused),
            right_border: SolidColorBuffer::new((BORDER_THICKNESS, 0), unfocused),
            bottom_border: SolidColorBuffer::new((0, BORDER_THICKNESS), unfocused),
            hovered: None,
            focused: false,
            restore_location: None,
            minimized: false,
            title: None,
        }
    }
}

/// Cria uma cor nova com o RGB de `c` e o alpha trocado. Helper local
/// pra evitar repetição.
fn with_alpha(c: Color32F, alpha: f32) -> Color32F {
    Color32F::new(c.r(), c.g(), c.b(), alpha)
}

/// Atualiza o cache do título dessa janela se o texto mudou (ou se ainda
/// não tinha cache). Chamado antes do render. Resolve a string preferindo
/// `xdg_toplevel.title`, caindo no app_id como fallback.
///
/// H32.6.5: tamanho vem de `theme.window.title_size` (13), cor vem do
/// tema do header (Cream pra dark / Coffee pra light). Alpha do foco
/// é aplicado no render (não no buffer cacheado).
pub fn update_title(window: &Window, renderer: &crate::text::TextRenderer) {
    let title_text = window
        .toplevel()
        .and_then(|t| {
            smithay::wayland::compositor::with_states(t.wl_surface(), |states| {
                states
                    .data_map
                    .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                    .and_then(|d| {
                        let guard = d.lock().unwrap();
                        guard.title.clone().or_else(|| guard.app_id.clone())
                    })
            })
        })
        .unwrap_or_default();

    let mut deco = decoration_for(window);
    let needs_rebuild = match &deco.title {
        Some(cache) => cache.text != title_text,
        None => !title_text.is_empty(),
    };
    if !needs_rebuild {
        return;
    }

    if title_text.is_empty() {
        deco.title = None;
        return;
    }

    let theme = crate::theme::current();
    let (mut pixels, w, h) = renderer.rasterize(&title_text, theme.window.title_size);
    if w == 0 || h == 0 {
        deco.title = None;
        return;
    }
    // H32.6.5: text renderer produz branco premultiplicado. Repinta os
    // pixels pra cor do tema do header (Cream pra dark / Coffee pra light).
    // Header dark é o caso default; light entra quando o user customiza
    // window_header_bg_dark pra cor clara (a partir daí is_dark_header
    // muda também).
    let header_bg = theme.colors.window_header_bg_dark;
    let is_dark_header = theme.is_dark_header(header_bg);
    let title_color = if is_dark_header {
        theme.colors.window_title_color_dark
    } else {
        theme.colors.window_title_color_light
    };
    repaint_white_glyph_to(&mut pixels, title_color);

    let buffer = MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (w as i32, h as i32),
        1,
        Transform::Normal,
        None,
    );
    deco.title = Some(TitleCache {
        text: title_text,
        buffer,
        size: Size::from((w as i32, h as i32)),
    });
}

/// Repinta um buffer Argb8888 BGRA-em-little-endian premultiplicado:
/// trocar os pixels [α, α, α, α] (branco premul gerado pelo TextRenderer)
/// pela cor `target` com o mesmo alpha do glyph original. Preserva
/// anti-aliasing.
fn repaint_white_glyph_to(pixels: &mut [u8], target: Color32F) {
    let tr = (target.r() * 255.0).round() as u16;
    let tg = (target.g() * 255.0).round() as u16;
    let tb = (target.b() * 255.0).round() as u16;
    for chunk in pixels.chunks_exact_mut(4) {
        let a = chunk[3] as u16;
        if a == 0 {
            continue;
        }
        // Premultiplicação: cada canal de cor é (cor * alpha / 255).
        chunk[0] = ((tb * a) / 255) as u8; // B
        chunk[1] = ((tg * a) / 255) as u8; // G
        chunk[2] = ((tr * a) / 255) as u8; // R
        // alpha mantém igual (já era alpha do glyph)
    }
}

/// Consulta o flag de foco da janela (atualizado por `set_focused`).
/// Usado pelo render do título pra modular alpha (sem foco = mais
/// dim). H32.6.5.
pub fn is_focused(window: &Window) -> bool {
    window
        .user_data()
        .get::<RefCell<WindowDecoration>>()
        .map(|cell| cell.borrow().focused)
        .unwrap_or(false)
}

/// Consulta o flag minimized de uma janela. Acessor de leitura pra
/// quem não quer abrir o RefCell mutable.
pub fn is_minimized(window: &Window) -> bool {
    window
        .user_data()
        .get::<RefCell<WindowDecoration>>()
        .map(|cell| cell.borrow().minimized)
        .unwrap_or(false)
}

/// Acesso direto à decoração de uma janela pra leitura/escrita de campos
/// públicos (restore_location etc).
pub fn with_decoration<R>(window: &Window, f: impl FnOnce(&mut WindowDecoration) -> R) -> R {
    let mut deco = decoration_for(window);
    f(&mut *deco)
}

// luminance() e is_light_color() locais foram movidos pra theme.rs como
// wcag_luminance() + Theme::is_dark_header() (H32.6.1). Esta etapa
// (H32.6.6) trocou todas as decisões locais pelo helper centralizado.

fn decoration_for(window: &Window) -> RefMut<'_, WindowDecoration> {
    window
        .user_data()
        .insert_if_missing(|| RefCell::new(WindowDecoration::new()));
    window
        .user_data()
        .get::<RefCell<WindowDecoration>>()
        .unwrap()
        .borrow_mut()
}

pub fn ensure_decoration(window: &Window) {
    let _ = decoration_for(window);
}

/// Buffers dos 3 window controls "Luma original" (H32.5.12/.13)
/// pré-renderizados uma única vez. Cada um é `hit_area×hit_area`
/// transparente com o ícone vetorial em Cream alpha 0.85.
///
/// `*_hover_squircle`: pílula r=hover_radius preenchida na cor da ação
/// (Honey/Sage/Coral), pré-renderizada (H32.5.13).
/// `minimize_dark`: variante do minimize com ícone em Coffee — usado
/// quando minimize está em hover (squircle Honey é claro demais pra
/// Cream contrastar).
pub struct ButtonGlyphs {
    pub close: MemoryRenderBuffer,
    pub maximize: MemoryRenderBuffer,
    pub minimize: MemoryRenderBuffer,
    pub minimize_dark: MemoryRenderBuffer,
    /// Variantes cinza pra janela fora de foco. H_deco.8.
    pub close_inactive: MemoryRenderBuffer,
    pub maximize_inactive: MemoryRenderBuffer,
    pub minimize_inactive: MemoryRenderBuffer,
    pub hover_minimize_squircle: MemoryRenderBuffer,
    pub hover_maximize_squircle: MemoryRenderBuffer,
    pub hover_close_squircle: MemoryRenderBuffer,
    pub hit_area: i32,
}

static BUTTON_GLYPHS: OnceLock<ButtonGlyphs> = OnceLock::new();

/// Inicializa (ou devolve) os buffers globais dos window controls. O
/// `_renderer` é mantido por compat com call site existente, mas H32.5.12
/// não usa fontdue — os ícones são desenhados em código.
pub fn ensure_button_glyphs(_renderer: &crate::text::TextRenderer) -> &'static ButtonGlyphs {
    BUTTON_GLYPHS.get_or_init(build_button_glyphs)
}

pub fn button_glyphs() -> Option<&'static ButtonGlyphs> {
    BUTTON_GLYPHS.get()
}

fn build_button_glyphs() -> ButtonGlyphs {
    let theme = crate::theme::current();
    let hit = theme.window_controls.hit_area;
    let icon = theme.window_controls.icon_size;
    let radius = theme.window_controls.hover_radius;

    // H32.6.6: a cor + alpha do ícone seguem o TEMA DO HEADER (não mais
    // o legacy theme.border). Detecção via Theme::is_dark_header sobre a
    // cor REAL do header da janela (window_header_bg_dark default).
    // Quando o user customizar pra header light no luma.toml, os ícones
    // automaticamente trocam pra Coffee alpha 0.7.
    //
    // Limitação: tudo decidido no init (OnceLock). Janelas com headers
    // diferentes precisariam de 2 conjuntos de buffers — débito da
    // H32.6.7 documentado em visual-debt.md.
    let header_bg = theme.colors.window_header_bg_dark;
    let is_dark_header = theme.is_dark_header(header_bg);
    let (base_color, alpha) = if is_dark_header {
        (theme.colors.window_control_icon_light, theme.colors.window_control_light_alpha)
    } else {
        (theme.colors.window_control_icon_dark, theme.colors.window_control_dark_alpha)
    };
    let icon_color = Color32F::new(base_color.r(), base_color.g(), base_color.b(), alpha);
    // Variante "dark" usada APENAS sobre o squircle Honey (minimize hover) —
    // independe do header pq Honey é claro demais pra Cream contrastar.
    let dark = theme.colors.window_control_icon_dark;
    let icon_color_dark = Color32F::new(dark.r(), dark.g(), dark.b(), 1.0);

    ButtonGlyphs {
        close: crate::gfx::make_window_control(
            hit,
            icon,
            crate::gfx::WindowControlIcon::Close,
            icon_color,
        ),
        maximize: crate::gfx::make_window_control(
            hit,
            icon,
            crate::gfx::WindowControlIcon::Maximize,
            icon_color,
        ),
        minimize: crate::gfx::make_window_control(
            hit,
            icon,
            crate::gfx::WindowControlIcon::Minimize,
            icon_color,
        ),
        minimize_dark: crate::gfx::make_window_control(
            hit,
            icon,
            crate::gfx::WindowControlIcon::Minimize,
            icon_color_dark,
        ),
        // H_deco.8: variantes inactive cinza (#8B7D72) pra janela fora
        // de foco. Mesma forma dos ícones, só muda cor.
        close_inactive: crate::gfx::make_window_control(
            hit,
            icon,
            crate::gfx::WindowControlIcon::Close,
            theme.window_controls.inactive_color,
        ),
        maximize_inactive: crate::gfx::make_window_control(
            hit,
            icon,
            crate::gfx::WindowControlIcon::Maximize,
            theme.window_controls.inactive_color,
        ),
        minimize_inactive: crate::gfx::make_window_control(
            hit,
            icon,
            crate::gfx::WindowControlIcon::Minimize,
            theme.window_controls.inactive_color,
        ),
        hover_minimize_squircle: crate::gfx::make_rounded_panel(
            hit,
            hit,
            radius,
            theme.colors.hover_minimize,
        ),
        hover_maximize_squircle: crate::gfx::make_rounded_panel(
            hit,
            hit,
            radius,
            theme.colors.hover_maximize,
        ),
        hover_close_squircle: crate::gfx::make_rounded_panel(
            hit,
            hit,
            radius,
            theme.colors.hover_close,
        ),
        hit_area: hit,
    }
}

/// Pega clone do MemoryRenderBuffer do título e seu tamanho lógico, pra
/// montar um RenderElement fora do RefCell. `None` se a janela não tem
/// título rasterizado ainda (texto vazio ou TextRenderer indisponível).
pub fn title_data(window: &Window) -> Option<(MemoryRenderBuffer, Size<i32, Logical>)> {
    let deco = decoration_for(window);
    deco.title.as_ref().map(|c| (c.buffer.clone(), c.size))
}

/// Atualiza o estado de hover do header desta janela. Caller (input/pointer)
/// chama em cada motion. H32.5.12: o estado é só guardado — o squircle
/// colorido do hover entra na H32.5.13.
pub fn update_hover(window: &Window, region: Option<HeaderRegion>) {
    let mut deco = decoration_for(window);
    if deco.hovered == region {
        return;
    }
    deco.hovered = region;
}

/// Atualiza o foco da janela e recolore as bordas externas. Caller
/// (winit.rs) chama por janela no início de cada frame baseado em
/// `seat.get_keyboard().current_focus()`. Idempotente — se o estado
/// não mudou, vira no-op. H32.6.3.
pub fn set_focused(window: &Window, focused: bool) {
    let mut deco = decoration_for(window);
    if deco.focused == focused {
        return;
    }
    deco.focused = focused;
    let theme = crate::theme::current();
    let color = if focused {
        with_alpha(
            theme.colors.window_border_focused,
            theme.colors.window_border_focused_alpha,
        )
    } else {
        with_alpha(
            theme.colors.window_border_unfocused,
            theme.colors.window_border_unfocused_alpha,
        )
    };
    deco.left_border.set_color(color);
    deco.right_border.set_color(color);
    deco.bottom_border.set_color(color);
}

/// Devolve o estado de hover atual do header desta janela. Usado pelo
/// render do squircle (H32.5.13).
pub fn hovered_region(window: &Window) -> Option<HeaderRegion> {
    decoration_for(window).hovered
}

/// Constrói os elementos do header: linha âmbar 1px (Solid) + gradient
/// vertical bg (Memory). H_deco.4: bg agora é gradient esticado pra
/// largura via dst_size.
pub fn header_top_accent_at(
    window: &Window,
    origin: Point<i32, Physical>,
    full_width: i32,
) -> SolidColorRenderElement {
    let width = full_width.max(1);
    let mut deco = decoration_for(window);
    deco.header_top_accent.resize((width, 1));
    SolidColorRenderElement::from_buffer(
        &deco.header_top_accent,
        origin,
        1.0,
        1.0,
        Kind::Unspecified,
    )
}

/// Constrói o Memory element do bg gradient da titlebar, esticado
/// horizontalmente pra `full_width`. H_deco.4.
pub fn header_bg_at<R>(
    window: &Window,
    renderer: &mut R,
    origin: Point<i32, Physical>,
    full_width: i32,
) -> Option<MemoryRenderBufferRenderElement<R>>
where
    R: Renderer + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let width = full_width.max(1);
    let deco = decoration_for(window);
    let src = smithay::utils::Rectangle::<f64, Logical>::from_size(
        Size::<f64, Logical>::from((1.0, HEADER_HEIGHT as f64)),
    );
    let dst = Some(Size::<i32, Logical>::from((width, HEADER_HEIGHT)));
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        origin.to_f64(),
        &deco.background,
        None,
        Some(src),
        dst,
        Kind::Unspecified,
    )
    .ok()
}

/// Constrói os Memory elements dos 3 window controls posicionados à
/// direita do header. H32.5.13: quando `hovered` casa com a região do
/// controle, push primeiro o squircle colorido (Honey/Sage/Coral) e
/// depois o ícone (cor adapta pra contraste — Coffee em cima do Honey,
/// Cream nos outros 2).
///
/// Smithay renderiza fim→começo: ícone vai depois do squircle no Vec
/// pra ficar ATRÁS no z-stack? Não — invertido: o ícone vem PRIMEIRO
/// no Vec retornado (= topo do z-stack), squircle depois (= fundo).
pub fn window_control_elements_at<R>(
    renderer: &mut R,
    origin: Point<i32, Physical>,
    full_width: i32,
    hovered: Option<HeaderRegion>,
    focused: bool,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: Renderer + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let Some(glyphs) = button_glyphs() else {
        return Vec::new();
    };
    let theme = crate::theme::current();
    let hit = glyphs.hit_area;
    let gap = theme.window_controls.gap;
    let pad_right = theme.window_controls.header_padding_right;
    let y_offset = (HEADER_HEIGHT - hit) / 2;

    let close_x = full_width - pad_right - hit;
    let max_x = close_x - gap - hit;
    let min_x = max_x - gap - hit;

    let mut out = Vec::new();
    let src = smithay::utils::Rectangle::<f64, Logical>::from_size(
        Size::<f64, Logical>::from((hit as f64, hit as f64)),
    );
    let size_dest = Some(Size::<i32, Logical>::from((hit, hit)));

    // Pra cada controle, decide qual ícone usar + se tem squircle.
    // H_deco.8: janela fora de foco usa variante inactive (cinza).
    // Hover ainda mostra cor original (Honey/Sage/Coral) pra dar
    // feedback claro mesmo na janela inativa.
    for (region, x_off) in [
        (HeaderRegion::Minimize, min_x),
        (HeaderRegion::Maximize, max_x),
        (HeaderRegion::Close, close_x),
    ] {
        let is_hover = hovered == Some(region);
        let icon_buf = match (region, is_hover, focused) {
            // Min hover: minimize_dark (coffee sobre squircle Honey).
            (HeaderRegion::Minimize, true, _) => &glyphs.minimize_dark,
            // Inactive (sem hover): cinza.
            (HeaderRegion::Minimize, false, false) => &glyphs.minimize_inactive,
            (HeaderRegion::Maximize, false, false) => &glyphs.maximize_inactive,
            (HeaderRegion::Close, false, false) => &glyphs.close_inactive,
            // Foco normal: cream alpha 0.85.
            (HeaderRegion::Minimize, false, true) => &glyphs.minimize,
            (HeaderRegion::Maximize, _, _) => &glyphs.maximize,
            (HeaderRegion::Close, _, _) => &glyphs.close,
            _ => continue,
        };
        let pos = origin + Point::<i32, Physical>::from((x_off, y_offset));
        // Ícone primeiro (= topo do z-stack do header).
        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos.to_f64(),
            icon_buf,
            None,
            Some(src),
            size_dest,
            Kind::Unspecified,
        ) {
            out.push(elem);
        }
        // Squircle depois (= atrás do ícone no z-stack).
        if is_hover {
            let squircle = match region {
                HeaderRegion::Minimize => &glyphs.hover_minimize_squircle,
                HeaderRegion::Maximize => &glyphs.hover_maximize_squircle,
                HeaderRegion::Close => &glyphs.hover_close_squircle,
                _ => continue,
            };
            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                pos.to_f64(),
                squircle,
                None,
                Some(src),
                size_dest,
                Kind::Unspecified,
            ) {
                out.push(elem);
            }
        }
    }
    out
}

/// Constrói os 3 elementos das bordas laterais e inferior. A borda
/// superior já é coberta pelo header (mesma cor). `content_height` é a
/// altura da surface do cliente (sem header/bordas). `full_width` =
/// content + 2 * BORDER.
pub fn border_elements_at(
    window: &Window,
    origin: Point<i32, Physical>,
    full_width: i32,
    content_height: i32,
) -> Vec<SolidColorRenderElement> {
    let mut deco = decoration_for(window);
    deco.left_border
        .resize((BORDER_THICKNESS, content_height.max(1)));
    deco.right_border
        .resize((BORDER_THICKNESS, content_height.max(1)));
    deco.bottom_border
        .resize((full_width.max(1), BORDER_THICKNESS));

    let left_off = Point::<i32, Physical>::from((0, HEADER_HEIGHT));
    let right_off =
        Point::<i32, Physical>::from((full_width - BORDER_THICKNESS, HEADER_HEIGHT));
    let bottom_off = Point::<i32, Physical>::from((0, HEADER_HEIGHT + content_height));

    vec![
        SolidColorRenderElement::from_buffer(
            &deco.left_border,
            origin + left_off,
            1.0,
            1.0,
            Kind::Unspecified,
        ),
        SolidColorRenderElement::from_buffer(
            &deco.right_border,
            origin + right_off,
            1.0,
            1.0,
            Kind::Unspecified,
        ),
        SolidColorRenderElement::from_buffer(
            &deco.bottom_border,
            origin + bottom_off,
            1.0,
            1.0,
            Kind::Unspecified,
        ),
    ]
}

/// Hit-test no header. `point_in_window` é o ponto do mouse em coords
/// LÓGICAS relativas ao topo-esquerdo da janela (incluindo header).
/// H32.5.12: layout dos controles vem do theme.window_controls (hit_area
/// 24, gap 6, padding direito 12).
pub fn hit_test_header(width: i32, point_in_window: Point<f64, Logical>) -> Option<HeaderRegion> {
    if point_in_window.x < 0.0
        || point_in_window.x >= width as f64
        || point_in_window.y < 0.0
        || point_in_window.y >= HEADER_HEIGHT as f64
    {
        return None;
    }

    let theme = crate::theme::current();
    let hit = theme.window_controls.hit_area;
    let gap = theme.window_controls.gap;
    let pad_right = theme.window_controls.header_padding_right;

    let close_x_start = (width - pad_right - hit) as f64;
    let max_x_start = close_x_start - (gap + hit) as f64;
    let min_x_start = max_x_start - (gap + hit) as f64;
    let x = point_in_window.x;
    let close_end = close_x_start + hit as f64;

    if x >= close_x_start && x < close_end {
        Some(HeaderRegion::Close)
    } else if x >= max_x_start && x < close_x_start {
        // dentro do hit do max OU no gap close↔max — favorece max
        if x < max_x_start + hit as f64 {
            Some(HeaderRegion::Maximize)
        } else {
            Some(HeaderRegion::Background)
        }
    } else if x >= min_x_start && x < max_x_start {
        if x < min_x_start + hit as f64 {
            Some(HeaderRegion::Minimize)
        } else {
            Some(HeaderRegion::Background)
        }
    } else {
        Some(HeaderRegion::Background)
    }
}
