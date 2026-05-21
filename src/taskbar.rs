use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                memory::MemoryRenderBuffer,
                solid::{SolidColorBuffer, SolidColorRenderElement},
                Kind,
            },
        },
    },
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Size, Transform},
};

// Layout da taskbar agora vem 100% do `theme.taskbar` (Golden Hour H32.5.2).
// Helpers `*()` abaixo lêem do tema e são usados pelo render.

/// Altura total da barra inferior. Default 48px (Golden Hour).
pub fn taskbar_height() -> i32 {
    crate::theme::current().taskbar.height
}

/// Tamanho do container do launcher (quadrado). Default 44 (H33.taskbar.3).
pub fn launcher_width() -> i32 {
    crate::theme::current().taskbar.launcher.size
}

/// Altura do launcher = mesma do width (quadrado).
pub fn launcher_height() -> i32 {
    launcher_width()
}

/// Corner radius do launcher (H33.taskbar.3).
pub fn launcher_corner_radius() -> i32 {
    crate::theme::current().taskbar.launcher.corner_radius
}

/// Padding lateral esquerdo da barra (antes do launcher). Default 12.
pub fn padding_left() -> i32 {
    crate::theme::current().taskbar.padding_left
}

/// Padding lateral direito da barra. Default 0 (show-desktop encosta).
/// H32.5.2 só preenche o token; usado de fato a partir da H32.5.11.
#[allow(dead_code)]
pub fn padding_right() -> i32 {
    crate::theme::current().taskbar.padding_right
}

/// Espaço entre cada applet (launcher↔primeiro bloco, bloco↔bloco).
/// Default 8.
pub fn gap_between_applets() -> i32 {
    crate::theme::current().taskbar.gap_between_applets
}

/// Espaço vertical livre de cada lado do bloco dentro da barra.
pub const BLOCK_PADDING: i32 = 4;

/// Largura máxima do botão de janela na taskbar (pílula). Default 220.
pub fn block_width() -> i32 {
    crate::theme::current().window_button.max_width
}

/// Altura do botão de janela. Default 32.
pub fn block_height() -> i32 {
    crate::theme::current().window_button.height
}

/// Border-radius (px) dos applets — pílula do window button, container do
/// launcher, etc. Default 6.
pub fn applet_corner_radius() -> i32 {
    crate::theme::current().taskbar.applet_corner_radius
}

/// Espaçamento horizontal entre blocos = mesmo gap dos applets.
pub fn block_spacing() -> i32 {
    gap_between_applets()
}

/// Tamanho da fonte do relógio (HH:MM no canto direito).
const CLOCK_LABEL_SIZE: f32 = 16.0;

/// Margem direita do relógio dentro da taskbar (espaço entre fim do
/// relógio e show-desktop).
pub fn clock_padding_right() -> i32 {
    crate::theme::current().clock.padding_right
}

/// Largura da faixinha "Mostrar área de trabalho" no canto direito.
/// Default 10 (Win7-style fininho).
pub fn show_desktop_width() -> i32 {
    crate::theme::current().show_desktop.width
}

// Cores agora vêm de `crate::theme::current()`.

/// 6 zonas da taskbar v5 (H33.taskbar.2). Layout:
///
/// ```text
/// [A]│[B B B]│[C C C C ...]          [D][E][F]
///  ^---- grupo ESQUERDO ----^         ^-- DIREITO --^
/// ```
///
/// - Grupo esquerdo ancora em `padding_left`, cresce pra direita.
/// - Grupo direito (D/E/F) ancora na borda direita, cresce pra esquerda,
///   3 zonas grudadas (`right_group.gap_between_zones`).
///
/// As funções `zone_bounds_*` em `taskbar.rs` (commits A.3+) calculam
/// posição e tamanho de cada zona dinamicamente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarZone {
    /// A — Launcher (logo Luma). 44×44.
    Launcher,
    /// B — Workspaces pager. Quadrados 32×32 numerados (escondido se 1 ws).
    Workspaces,
    /// C — Apps abertos. Quadrados 44×44 com ícone 32px.
    Apps,
    /// D — System tray (battery, wifi, volume, notifications).
    SystemTray,
    /// E — Clock (hora Fraunces + data Manrope).
    Clock,
    /// F — Show desktop (botão 44×44).
    ShowDesktop,
}

/// Resultado do hit-test na taskbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarHit {
    Launcher,
    Window(usize),
    ShowDesktop,
}

/// Cache do título de uma janela já rasterizado: o buffer pronto pra
/// render + tamanho lógico + o texto que gerou (pra invalidar quando muda).
#[derive(Debug, Clone)]
pub struct TitleCache {
    pub text: String,
    pub buffer: MemoryRenderBuffer,
    pub size: Size<i32, Logical>,
}

pub struct TaskbarState {
    /// Background da taskbar — textura 1×taskbar_height com gradient
    /// vertical Coffee→Black (Golden Hour H32.5.3). Pré-renderizada uma
    /// vez no init, esticada horizontalmente pra largura do output.
    pub background: MemoryRenderBuffer,
    /// Borda âmbar 1px no topo da taskbar (Golden Hour H32.5.4). Detalhe
    /// de marca, sem função semântica. Cor `accent_amber` com alpha
    /// `accent_amber_alpha_top` (default 0.25).
    pub border_top: SolidColorBuffer,
    /// Container quadrado do launcher (44×44 default em H33) com cantos
    /// arredondados + mini logo Luma desenhado dentro. Memory texture
    /// pré-renderizada.
    pub launcher: MemoryRenderBuffer,
    /// Divider 1px à direita do launcher, altura = taskbar. Sutil
    /// (alpha ~0.1). H33.taskbar.3.
    pub launcher_divider: SolidColorBuffer,
    /// Pílulas pré-renderizadas pros 4 estados visuais do bloco da
    /// janela na taskbar (H32.5.5/.8). Cada uma é w×h com cantos
    /// arredondados (raio = applet_corner_radius). Compartilhadas por
    /// TODOS os blocos — só a cor muda por estado.
    pub block_inactive: MemoryRenderBuffer,
    pub block_hover: MemoryRenderBuffer,
    pub block_active: MemoryRenderBuffer,
    /// Pílula da janela minimizada — cor `taskbar_bg_top` (Coffee escuro)
    /// com alpha 0.6, ícone/título atrás recebem alpha menor pra dar
    /// feel "dim". Pré-renderizada também (H32.5.8).
    pub block_minimized: MemoryRenderBuffer,
    /// Indicador âmbar fininho que aparece no fundo do bloco ATIVO
    /// (H32.5.6). Cor `accent_amber`, altura `window_button.active_indicator_height`
    /// (default 3px), largura = block_width.
    pub block_active_indicator: SolidColorBuffer,
    /// Cache de títulos rasterizados por janela (paralelo a `order`).
    /// Re-rasterizados quando o título da janela muda. Ainda só
    /// preparado pra ser usado pela render do bloco no winit.rs (H32.5.5
    /// vai consumir).
    pub title_labels: Vec<Option<TitleCache>>,
    /// Texto formatado do relógio. Hora (`clock_time_text`) e data
    /// (`clock_date_text`) cacheados separadamente — ensure_clock só
    /// re-rasteriza o buffer correspondente quando muda. (H32.5.10:
    /// clock virou 2 linhas — hora 13pt cor primary + data 9pt cor
    /// secondary, ambas alinhadas à direita.)
    pub clock_time_text: String,
    pub clock_time_label: Option<(MemoryRenderBuffer, Size<i32, Logical>)>,
    pub clock_date_text: String,
    pub clock_date_label: Option<(MemoryRenderBuffer, Size<i32, Logical>)>,
    /// Botão "Mostrar área de trabalho" no canto extremo direito da
    /// taskbar (faixinha fina estilo Win7). Default: transparente
    /// (gradient da taskbar aparece atrás). Hover: accent_amber com
    /// `show_desktop.hover_bg_alpha` (default 0.3). H32.5.11.
    pub show_desktop: SolidColorBuffer,
    /// Linha 1px de divisor à esquerda do show-desktop (cor Cream com
    /// alpha `show_desktop.border_left_alpha`, default 0.15). Sempre
    /// renderizada (sem hover). H32.5.11.
    pub show_desktop_border_left: SolidColorBuffer,
    pub show_desktop_hovered: bool,
    /// Índice do bloco da janela sob o cursor (`None` se nenhum).
    pub hovered_block: Option<usize>,
    /// Ordem ESTÁVEL das janelas na taskbar — independente do z-order
    /// do Space (que muda em cada raise). Surface entra no final quando
    /// uma janela nova aparece; sai quando some do espaço. Mantém o
    /// bloco visual no mesmo lugar durante click-to-focus.
    pub order: Vec<WlSurface>,
    /// Nome do output ao qual esta taskbar pertence. Setado em
    /// `attach_to_output`. Hoje é só metadata — o render continua
    /// recebendo `output_size` como param. Em multi-monitor futuro,
    /// vai ter 1 TaskbarState por output e o name vira handle. H34.3.
    pub output_name: Option<String>,
}

impl TaskbarState {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let background = crate::gfx::make_vertical_gradient(
            taskbar_height(),
            theme.colors.taskbar_bg_top,
            theme.colors.taskbar_bg_bottom,
        );
        // Borda âmbar 1px (cor accent_amber com alpha de top border).
        let amber = theme.colors.accent_amber;
        let border_top_color = smithay::backend::renderer::Color32F::new(
            amber.r(),
            amber.g(),
            amber.b(),
            theme.colors.accent_amber_alpha_top,
        );
        // Pílulas pré-renderizadas pros 3 estados visuais (H32.5.5).
        let bw = block_width();
        let bh = block_height();
        let br = applet_corner_radius();
        let block_inactive = crate::gfx::make_rounded_panel(bw, bh, br, theme.colors.applet_bg);
        let block_hover =
            crate::gfx::make_rounded_panel(bw, bh, br, theme.colors.applet_bg_hover);
        let block_active =
            crate::gfx::make_rounded_panel(bw, bh, br, theme.colors.applet_bg_active);
        // Minimizado: cor Coffee escuro (taskbar_bg_top) com alpha 0.6.
        let mini_color = smithay::backend::renderer::Color32F::new(
            theme.colors.taskbar_bg_top.r(),
            theme.colors.taskbar_bg_top.g(),
            theme.colors.taskbar_bg_top.b(),
            0.6,
        );
        let block_minimized = crate::gfx::make_rounded_panel(bw, bh, br, mini_color);
        // Indicador âmbar do bloco ativo (3px na cor accent_amber).
        let block_active_indicator = SolidColorBuffer::new(
            (bw, theme.window_button.active_indicator_height),
            theme.colors.accent_amber,
        );
        Self {
            background,
            border_top: SolidColorBuffer::new(
                (0, theme.taskbar.border_top_height),
                border_top_color,
            ),
            // Launcher: container 44×44 (H33) com cantos r9 + mini logo
            // Luma dentro. Cor bg vem de `taskbar.launcher.bg` (#3D2520).
            launcher: crate::gfx::make_luma_launcher(
                launcher_width(),
                theme.taskbar.launcher.bg,
                launcher_corner_radius(),
                &crate::gfx::LumaLogoColors {
                    // Beam: gradient bottom (Coffee escuro) → top (Honey).
                    beam_bottom: smithay::backend::renderer::Color32F::new(
                        0.624, 0.290, 0.208, 1.0, // #9F4A35
                    ),
                    beam_top: theme.colors.hover_minimize, // Honey #F5D5A8
                    foot: theme.colors.hover_minimize,     // Honey
                    dot: theme.colors.text_primary,        // Cream #FBF6EE
                },
            ),
            // Divider 1px à direita do launcher. Cor = divider Cream com
            // alpha do token. H33.taskbar.3.
            launcher_divider: SolidColorBuffer::new(
                (1, taskbar_height()),
                smithay::backend::renderer::Color32F::new(
                    theme.colors.divider.r(),
                    theme.colors.divider.g(),
                    theme.colors.divider.b(),
                    theme.taskbar.launcher.divider_alpha,
                ),
            ),
            block_inactive,
            block_hover,
            block_active,
            block_minimized,
            block_active_indicator,
            title_labels: Vec::new(),
            clock_time_text: String::new(),
            clock_time_label: None,
            clock_date_text: String::new(),
            clock_date_label: None,
            show_desktop: SolidColorBuffer::new(
                (show_desktop_width(), taskbar_height()),
                // Default transparente — só aparece no hover.
                smithay::backend::renderer::Color32F::new(0.0, 0.0, 0.0, 0.0),
            ),
            show_desktop_border_left: SolidColorBuffer::new(
                (1, taskbar_height()),
                smithay::backend::renderer::Color32F::new(
                    theme.colors.divider.r(),
                    theme.colors.divider.g(),
                    theme.colors.divider.b(),
                    theme.show_desktop.border_left_alpha,
                ),
            ),
            show_desktop_hovered: false,
            hovered_block: None,
            order: Vec::new(),
            output_name: None,
        }
    }

    /// Vincula esta taskbar a um output. Hoje só armazena o nome pra
    /// referência futura. Em multi-monitor, 1 TaskbarState por output
    /// e os call sites usam o name como handle.
    pub fn attach_to_output(&mut self, output: &Output) {
        self.output_name = Some(output.name());
    }

    /// Sincroniza `order` com a lista atual de surfaces — mantém a ordem
    /// das que sobreviveram, append novas no fim, remove as que sumiram.
    pub fn sync_order(&mut self, current: &[WlSurface]) {
        self.order.retain(|s| current.iter().any(|cs| cs == s));
        for surface in current {
            if !self.order.iter().any(|s| s == surface) {
                self.order.push(surface.clone());
            }
        }
        // H36.fix.titles: trunca caches paralelos pra evitar render de
        // títulos órfãos (que continuariam aparecendo depois de filter
        // reduzir order). title_labels (e qualquer Vec paralelo a order)
        // tem que respeitar o tamanho.
        self.title_labels.truncate(self.order.len());
    }

    /// Sincroniza o cache de títulos das janelas com a `order` atual:
    /// pra cada surface, extrai o `xdg_toplevel.title` (ou app_id como
    /// fallback) e re-rasteriza só se mudou desde a última render.
    ///
    /// Chamada antes do render no winit, com o text renderer disponível.
    /// Cresce o Vec conforme novas janelas aparecem; entradas órfãs ficam
    /// até a próxima sync_order limpar.
    pub fn ensure_title_labels(&mut self, renderer: &crate::text::TextRenderer) {
        const TITLE_SIZE: f32 = 13.0;
        const MAX_CHARS: usize = 22;
        while self.title_labels.len() < self.order.len() {
            self.title_labels.push(None);
        }
        for (i, surface) in self.order.iter().enumerate() {
            let raw = smithay::wayland::compositor::with_states(surface, |states| {
                states
                    .data_map
                    .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                    .and_then(|d| {
                        let g = d.lock().unwrap();
                        g.title.clone().or_else(|| g.app_id.clone())
                    })
            })
            .unwrap_or_default();
            let text = truncate_with_ellipsis(&raw, MAX_CHARS);
            let needs_rebuild = match &self.title_labels[i] {
                Some(cache) => cache.text != text,
                None => !text.is_empty(),
            };
            if !needs_rebuild {
                continue;
            }
            if text.is_empty() {
                self.title_labels[i] = None;
                continue;
            }
            let (pixels, w, h) = renderer.rasterize(&text, TITLE_SIZE);
            if w == 0 || h == 0 {
                self.title_labels[i] = None;
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
            self.title_labels[i] = Some(TitleCache {
                text,
                buffer,
                size: Size::from((w as i32, h as i32)),
            });
        }
    }

    /// Atualiza os 2 buffers do relógio (hora + data) só se as strings
    /// mudaram desde a última rasterização. Caller passa as 2 strings
    /// já formatadas — lógica de timezone/locale fica fora do taskbar.
    pub fn ensure_clock(
        &mut self,
        renderer: &crate::text::TextRenderer,
        time_str: &str,
        date_str: &str,
    ) {
        let theme = crate::theme::current();
        if self.clock_time_text != time_str {
            self.clock_time_text.clear();
            self.clock_time_text.push_str(time_str);
            self.clock_time_label =
                rasterize_text(renderer, time_str, theme.clock.time_size);
        }
        if self.clock_date_text != date_str {
            self.clock_date_text.clear();
            self.clock_date_text.push_str(date_str);
            self.clock_date_label =
                rasterize_text(renderer, date_str, theme.clock.date_size);
        }
    }

    // Compat — placeholder pra preservar compilação se ficou algum call site
    // antigo. Pode sumir em refactor futuro.
    #[allow(dead_code)]
    fn _legacy_ensure_clock_one_line(
        &mut self,
        renderer: &crate::text::TextRenderer,
        now_str: &str,
    ) {
        if self.clock_time_text == now_str {
            return;
        }
        self.clock_time_text.clear();
        self.clock_time_text.push_str(now_str);
        let (pixels, w, h) = renderer.rasterize(now_str, CLOCK_LABEL_SIZE);
        if w == 0 || h == 0 {
            self.clock_time_label = None;
            return;
        }
        let buffer = MemoryRenderBuffer::from_slice(
            &pixels,
            Fourcc::Argb8888,
            (w as i32, h as i32),
            1,
            Transform::Normal,
            None,
        );
        self.clock_time_label = Some((buffer, Size::from((w as i32, h as i32))));
    }
}

/// Helper genérico pra rasterizar texto e empacotar em (buffer, size).
/// `None` se o renderer devolveu buffer vazio (texto vazio ou erro).
fn rasterize_text(
    renderer: &crate::text::TextRenderer,
    text: &str,
    size: f32,
) -> Option<(MemoryRenderBuffer, Size<i32, Logical>)> {
    let (pixels, w, h) = renderer.rasterize(text, size);
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

/// Trunca string em N chars adicionando "…" se passou. Usado pra título
/// de janela na pílula da taskbar.
fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

// Blocos viraram pílulas Memory; consts antigas (BLOCK_SIZE etc) saíram.

pub fn blocks_start_x() -> i32 {
    padding_left() + launcher_width() + gap_between_applets()
}

/// Mesmo que `blocks_start_x` mas incluindo a largura do pager quando
/// visível (≥ pager_show_if_count workspaces). Usado pelos call sites
/// que precisam shiftar pra direita pra não colidir com o pager. H36.fix.
pub fn blocks_start_x_with_pager(workspace_count: usize) -> i32 {
    let base = blocks_start_x();
    if !crate::pager::is_visible(workspace_count) {
        return base;
    }
    let t = &crate::theme::current().workspaces;
    base + t.pager_padding_left + crate::pager::pager_total_width(workspace_count)
        + gap_between_applets()
}

/// Estado visual de um bloco da taskbar (H32.5.5–.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockState {
    Inactive,
    Hover,
    Active,
    Minimized,
}

/// Decide o estado visual do bloco `i` baseado em foco/hover/minimized.
/// Precedência: Minimized > Active > Hover > Inactive — uma janela
/// minimizada nunca aparece como Active mesmo se for a focada (que
/// não acontece no fluxo normal, mas o caso degenerado é tratado).
pub fn block_state_at(
    state: &TaskbarState,
    i: usize,
    surfaces: &[Option<WlSurface>],
    focused_surface: Option<&WlSurface>,
    minimized: &[bool],
) -> BlockState {
    if minimized.get(i).copied().unwrap_or(false) {
        return BlockState::Minimized;
    }
    let focused = matches!(
        (surfaces.get(i).and_then(Option::as_ref), focused_surface),
        (Some(a), Some(b)) if a == b
    );
    if focused {
        BlockState::Active
    } else if state.hovered_block == Some(i) {
        BlockState::Hover
    } else {
        BlockState::Inactive
    }
}

/// Constrói os elementos solid restantes (launcher, show-desktop, borda
/// âmbar, indicador âmbar do bloco ativo). Os blocos viraram Memory
/// elements e o background gradient também — esses são renderizados em
/// winit.rs.
pub fn render_solid_elements(
    state: &mut TaskbarState,
    output_size: Size<i32, Logical>,
    n_windows: usize,
    surfaces: &[Option<WlSurface>],
    focused_surface: Option<&WlSurface>,
    minimized: &[bool],
    workspace_count: usize,
) -> Vec<SolidColorRenderElement> {
    let bar_y = output_size.h - taskbar_height();
    // Background gradient e blocos (pílulas) são Memory elements —
    // renderizados em winit.rs depois deste Vec. Aqui só vão os Solids:
    // launcher, show-desktop, borda âmbar.

    let theme = crate::theme::current();
    let mut elements: Vec<SolidColorRenderElement> = Vec::with_capacity(n_windows + 4);

    // H33.taskbar.1: indicador âmbar 3px do bloco ATIVO legado desativado.
    // Substituído pela linha âmbar 24×3 sob app ativo em B.5.
    let _ = (n_windows, surfaces, focused_surface, minimized, workspace_count);

    // H33.taskbar.3: divider 1px vertical à direita do launcher, com
    // respiro `divider_gap_left` (default 6px) entre a borda do launcher
    // e a linha. Pushed antes do border top pra ficar atrás dele.
    if theme.taskbar.launcher.divider_after {
        state
            .launcher_divider
            .resize((1, taskbar_height()));
        let dx = padding_left() + launcher_width() + theme.taskbar.launcher.divider_gap_left;
        let dloc = Point::<i32, Logical>::from((dx, bar_y))
            .to_physical_precise_round(1.0);
        elements.push(SolidColorRenderElement::from_buffer(
            &state.launcher_divider,
            dloc,
            1.0,
            1.0,
            Kind::Unspecified,
        ));
    }

    // Show-desktop (faixinha no canto direito). Default transparente:
    // só renderiza no hover, com cor accent_amber alpha 0.3 (Golden Hour
    // H32.5.11). Borda 1px Cream sempre fica à esquerda como divisor.
    let sdw = show_desktop_width();
    let sd_x = output_size.w - sdw;
    if state.show_desktop_hovered {
        let amber_alpha = smithay::backend::renderer::Color32F::new(
            theme.colors.accent_amber.r(),
            theme.colors.accent_amber.g(),
            theme.colors.accent_amber.b(),
            theme.show_desktop.hover_bg_alpha,
        );
        state.show_desktop.set_color(amber_alpha);
        let loc =
            Point::<i32, Logical>::from((sd_x, bar_y)).to_physical_precise_round(1.0);
        elements.push(SolidColorRenderElement::from_buffer(
            &state.show_desktop,
            loc,
            1.0,
            1.0,
            Kind::Unspecified,
        ));
    }
    // Borda esquerda do show-desktop (sempre visível, divisor sutil).
    let border_left_loc =
        Point::<i32, Logical>::from((sd_x - 1, bar_y)).to_physical_precise_round(1.0);
    elements.push(SolidColorRenderElement::from_buffer(
        &state.show_desktop_border_left,
        border_left_loc,
        1.0,
        1.0,
        Kind::Unspecified,
    ));

    // Launcher agora é Memory texture (H32.5.9) — renderizado em winit.rs.

    // Borda âmbar 1px no topo da taskbar (H32.5.4). Pushed POR ÚLTIMO
    // dos Solids pra ficar atrás dos blocos/launcher (mais ao fundo do
    // Vec = mais ao fundo no z-stack do Smithay), mas ainda em CIMA do
    // gradient Memory (que é renderizado depois deste Vec todo).
    state.border_top.resize((output_size.w, theme.taskbar.border_top_height));
    let border_loc =
        Point::<i32, Logical>::from((0, bar_y)).to_physical_precise_round(1.0);
    elements.push(SolidColorRenderElement::from_buffer(
        &state.border_top,
        border_loc,
        1.0,
        1.0,
        Kind::Unspecified,
    ));

    // O background gradient é Memory element — renderizado pelo backend
    // após este Vec de Solid elements (vide H32.5.3 em winit.rs).

    elements
}

/// Posição lógica (topo-esquerdo) do container do launcher, centralizado
/// vertical na taskbar (já não tem mais o "L" — o logo é desenhado dentro
/// da textura do launcher). H32.5.9.
pub fn launcher_location(output_size: Size<i32, Logical>) -> Point<i32, Logical> {
    let bar_y = output_size.h - taskbar_height();
    let y = bar_y + (taskbar_height() - launcher_height()) / 2;
    Point::from((padding_left(), y))
}

/// Posições lógicas dos 2 labels do relógio (hora em cima, data embaixo).
/// Ambos alinhados à direita do mesmo `right_edge` (= borda esquerda do
/// show-desktop − padding_right do clock). Vertical: empilhados centralizados
/// na altura da taskbar com 1px de respiro entre as linhas.
pub fn clock_labels_location(
    output_size: Size<i32, Logical>,
    time_size: Size<i32, Logical>,
    date_size: Size<i32, Logical>,
) -> (Point<i32, Logical>, Point<i32, Logical>) {
    let bar_y = output_size.h - taskbar_height();
    let right_edge = output_size.w - show_desktop_width() - clock_padding_right();
    let total_h = time_size.h + date_size.h + 1;
    let top_y = bar_y + (taskbar_height() - total_h) / 2;
    let time_pt = Point::from((right_edge - time_size.w, top_y));
    let date_pt = Point::from((right_edge - date_size.w, top_y + time_size.h + 1));
    (time_pt, date_pt)
}

/// Hit-test: ponto está sobre launcher, sobre um bloco, ou em nenhum
/// alvo da taskbar.
pub fn hit_test(
    output_size: Size<i32, Logical>,
    point: Point<f64, Logical>,
    n_windows: usize,
    workspace_count: usize,
) -> Option<TaskbarHit> {
    let bar_y = (output_size.h - taskbar_height()) as f64;

    // Resto da taskbar.
    if point.y < bar_y || point.y > output_size.h as f64 {
        return None;
    }

    // Launcher dentro do padding_left.
    let launcher_min_x = padding_left() as f64;
    let launcher_max_x = (padding_left() + launcher_width()) as f64;
    if point.x >= launcher_min_x && point.x <= launcher_max_x {
        return Some(TaskbarHit::Launcher);
    }

    // Faixinha "Mostrar área de trabalho" no canto extremo direito.
    if point.x >= (output_size.w - show_desktop_width()) as f64 {
        return Some(TaskbarHit::ShowDesktop);
    }

    if n_windows == 0 {
        return None;
    }
    let bw = block_width();
    let bsp = block_spacing();
    let start_x = blocks_start_x_with_pager(workspace_count) as f64;
    let total_width = bw * n_windows as i32 + bsp * (n_windows as i32 - 1).max(0);
    let end_x = start_x + total_width as f64;
    if point.x < start_x || point.x > end_x {
        return None;
    }
    let relative_x = point.x - start_x;
    let slot = bw + bsp;
    let idx = (relative_x / slot as f64) as usize;
    let local = relative_x % slot as f64;
    if local <= bw as f64 && idx < n_windows {
        Some(TaskbarHit::Window(idx))
    } else {
        None
    }
}
