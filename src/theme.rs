use std::{path::PathBuf, sync::OnceLock};

use serde::Deserialize;
use smithay::backend::renderer::Color32F;
use tracing::{info, warn};

/// Tema global do compositor, carregado uma vez no boot do luma e
/// acessível de qualquer módulo via `theme::current()`. Imutável após
/// o init — mudança de cores requer reiniciar o compositor.
static GLOBAL_THEME: OnceLock<Theme> = OnceLock::new();

/// Inicializa o tema global a partir de `~/.config/luma/luma.toml` ou
/// defaults Golden Hour. Chamar no main, antes de qualquer subsistema usar cores.
pub fn init() {
    let _ = GLOBAL_THEME.set(Theme::load());
}

/// Referência ao tema atual. Se `init()` não foi chamado, inicializa
/// lazy com Theme::default() — usado em testes ou paths não previstos.
pub fn current() -> &'static Theme {
    GLOBAL_THEME.get_or_init(Theme::default)
}

/// Tema do Luma — paleta Golden Hour + tokens de layout pra taskbar,
/// window controls, menu, decoração. Carregado de `~/.config/luma/luma.toml`
/// se o arquivo existir, caso contrário usa `Theme::default()`.
///
/// Os campos de TOPO mantém compat com código existente; sub-structs
/// (`colors`, `taskbar`, etc) são os novos tokens H32.5 organizados por
/// seção. Defaults: paleta Golden Hour (Coffee/Coral/Amber/Cream/Sage/Honey).
#[derive(Debug, Clone)]
pub struct Theme {
    /// Cor de fundo do output (clear color). Default Coffee.
    pub clear: [f32; 4],

    /// Cor genérica de borda/divisor. Hoje usada em alguns lugares pré-H32.5.
    pub border: Color32F,

    /// Background da taskbar (cor sólida pra fallback; gradient real usa
    /// `colors.taskbar_bg_top` e `colors.taskbar_bg_bottom`).
    pub taskbar_bg: Color32F,
    /// Bloco da janela inativa na taskbar.
    pub taskbar_block_inactive: Color32F,
    /// Bloco da janela focada na taskbar.
    pub taskbar_block_active: Color32F,
    /// Botão launcher.
    pub taskbar_launcher: Color32F,

    /// Background do painel do menu (escuro). Usado na coluna esquerda.
    pub menu_bg: Color32F,
    /// Background da coluna direita (places).
    pub menu_bg_right: Color32F,
    /// Faixa superior do menu (área do user).
    pub menu_header_bar: Color32F,
    /// Cor de hover dos items do menu.
    pub menu_item_hover: Color32F,
    /// Cor da borda externa do menu (1px nas 4 laterais).
    pub menu_border: Color32F,

    /// Como o wallpaper é encaixado no output.
    pub wallpaper_mode: WallpaperMode,

    /// Tokens de cor Golden Hour (H32.5).
    pub colors: ColorTokens,
    /// Layout/cores da decoração de janela (H32.6).
    pub window: WindowTokens,
    /// Como decidir dark vs light theme da decoração (H32.6).
    pub window_theme_detection: WindowThemeDetectionTokens,
    /// Container do menu launcher (H32.7, legacy Win7 — desativado em H32.10.1).
    pub menu_launcher: MenuLauncherTokens,
    /// Backdrop + linha âmbar topo do menu fullscreen híbrido (H32.10).
    pub menu_fullscreen: MenuFullscreenTokens,
    /// Greeting corner do menu híbrido (H32.10.6).
    pub menu_greeting: MenuGreetingTokens,
    /// Search bar pill do menu híbrido (H32.10.7).
    pub menu_search_bar: MenuSearchBarTokens,
    /// Grid de favoritos curados 5×2 do menu híbrido (H32.10.8).
    pub menu_favorites: MenuFavoritesGridTokens,
    /// Grid 8×4 paginado de todos os apps (H32.10.11).
    pub menu_all_apps: MenuAllAppsGridTokens,
    /// Link "TODOS OS APPS →" entre grid e session bar (H32.10.13).
    pub menu_all_apps_link: MenuAllAppsLinkTokens,
    /// Session bar bottom com 5 botões + hint + 4 places (H32.10.13).
    pub menu_session_bar: MenuSessionBarTokens,
    /// Tokens de acessibilidade (a11y first-class, H32.10.17).
    pub accessibility: AccessibilityTokens,
    /// Profile bar do topo do menu.
    pub menu_profile_bar: MenuProfileBarTokens,
    /// Search bar do menu.
    pub menu_search: MenuSearchTokens,
    /// App row (item) do menu.
    pub menu_row: MenuRowTokens,
    /// Botões da sessão Luma Original.
    pub menu_session: MenuSessionTokens,
    /// Divider vertical entre colunas.
    pub menu_column_divider: MenuColumnDividerTokens,
    /// Tokens da search Spotlight-like. H32.9.
    pub search: SearchTokens,
    /// Tokens dos workspaces (pager + overview). H36.
    pub workspaces: WorkspacesTokens,
    /// Layout/medidas da taskbar.
    pub taskbar: TaskbarTokens,
    /// Layout dos botões de janela na taskbar.
    pub window_button: WindowButtonTokens,
    /// Layout/cores dos window controls (close/max/min) na decoração.
    pub window_controls: WindowControlsTokens,
    /// Layout do botão "show desktop" no canto direito.
    pub show_desktop: ShowDesktopTokens,
    /// Layout/cores do relógio.
    pub clock: ClockTokens,
}

/// Cores nomeadas da paleta Golden Hour + tokens de feedback (hover/glow).
#[derive(Debug, Clone)]
pub struct ColorTokens {
    /// Cor sólida do topo do gradient da taskbar (Coffee).
    pub taskbar_bg_top: Color32F,
    /// Cor sólida do fundo do gradient da taskbar (Black quase puro).
    pub taskbar_bg_bottom: Color32F,
    /// Background dos applets/blocos da taskbar (default).
    pub applet_bg: Color32F,
    /// Hover dos applets.
    pub applet_bg_hover: Color32F,
    /// Active/selected dos applets.
    pub applet_bg_active: Color32F,
    /// Acento âmbar (indicador, borda topo).
    pub accent_amber: Color32F,
    /// Alpha da borda âmbar 1px no topo da taskbar.
    pub accent_amber_alpha_top: f32,
    /// Raio do glow do indicador âmbar (px).
    pub accent_amber_glow: i32,
    /// Cor do squircle de hover do botão minimizar (Honey).
    pub hover_minimize: Color32F,
    /// Cor do squircle de hover do botão maximizar (Sage).
    pub hover_maximize: Color32F,
    /// Cor do squircle de hover do botão fechar (Coral).
    pub hover_close: Color32F,
    /// Raio do glow externo do squircle (px).
    pub hover_glow_radius: i32,
    /// Opacidade do glow externo do squircle.
    pub hover_glow_opacity: f32,
    /// Cor primária pra textos (Cream).
    pub text_primary: Color32F,
    /// Cor secundária pra textos (Cream desbotado).
    pub text_secondary: Color32F,
    /// Cor do ícone dos window controls em headers ESCUROS (Cream). H32.5.14.
    pub window_control_icon_light: Color32F,
    /// Opacidade do ícone em headers escuros.
    pub window_control_light_alpha: f32,
    /// Cor do ícone dos window controls em headers CLAROS (Coffee). H32.5.14.
    pub window_control_icon_dark: Color32F,
    /// Opacidade do ícone em headers claros.
    pub window_control_dark_alpha: f32,

    // ── H32.6 — decoração de janela ──
    /// Background do HEADER em tema escuro (Coffee +5%).
    pub window_header_bg_dark: Color32F,
    /// Background do HEADER em tema claro (Cream escurecido).
    pub window_header_bg_light: Color32F,
    /// Background do CORPO da janela em tema escuro (Coffee puro).
    pub window_bg_dark: Color32F,
    /// Background do CORPO em tema claro (Cream puro).
    pub window_bg_light: Color32F,
    /// Cor da borda da janela FOCADA (Amber).
    pub window_border_focused: Color32F,
    /// Opacidade da borda focada principal.
    pub window_border_focused_alpha: f32,
    /// Opacidade do halo externo da borda focada (sem efeito visual
    /// hoje — débito em visual-debt.md).
    pub window_border_focused_glow: f32,
    /// Cor da borda da janela FORA DE FOCO (Cream).
    pub window_border_unfocused: Color32F,
    /// Opacidade da borda fora-de-foco.
    pub window_border_unfocused_alpha: f32,
    /// Cor da linha 1px no topo do header.
    pub window_header_top_accent: Color32F,
    /// Opacidade da linha do topo do header.
    pub window_header_top_alpha: f32,
    /// Cor do título do header em tema escuro (Cream).
    pub window_title_color_dark: Color32F,
    /// Cor do título do header em tema claro (Coffee).
    pub window_title_color_light: Color32F,
    /// Multiplicador de alpha do título quando a janela está sem foco.
    pub window_title_unfocused_alpha: f32,
    /// Cor da sombra externa (sem efeito hoje — débito visual).
    pub window_shadow_color: Color32F,
    /// Opacidade da sombra externa em janelas sem foco.
    pub window_shadow_alpha: f32,
    /// Multiplicador de alpha da sombra na janela focada.
    pub window_shadow_focused_multiplier: f32,

    // ── H32.7 — menu launcher ──
    /// Cor do topo do gradient do menu.
    pub menu_bg_top: Color32F,
    pub menu_bg_top_alpha: f32,
    /// Cor do fundo do gradient do menu.
    pub menu_bg_bottom: Color32F,
    pub menu_bg_bottom_alpha: f32,
    /// Background da search bar.
    pub search_bg: Color32F,
    /// Background da search bar quando focada.
    pub search_bg_focused: Color32F,
    /// Cor da borda da search bar quando focada (Amber).
    pub search_border_focused: Color32F,
    /// Largura da borda da search bar focada (px).
    pub search_border_focused_width: f32,
    /// Cor do placeholder da search.
    pub search_placeholder: Color32F,
    /// Cor do texto digitado na search.
    pub search_text: Color32F,
    /// Cor do ícone lupa da search.
    pub search_icon: Color32F,
    /// Cor do ícone lupa quando search está focada.
    pub search_icon_focused: Color32F,
    /// Background do row em hover/keyboard-focus.
    pub row_bg_hover: Color32F,
    pub row_bg_keyboard_focus: Color32F,
    /// Linha 3px à esquerda do row em focus/hover (Amber).
    pub row_focus_indicator: Color32F,
    /// Cor do texto dos rows.
    pub row_text: Color32F,
    /// Peso do texto dos rows quando focado/hover.
    pub row_text_focused_weight: u16,
    /// Bolinha 3px à direita dos rows recém-usados.
    pub row_recently_used_dot: Color32F,
    /// Background do container da sessão.
    pub session_bg: Color32F,
    /// Cor default do ícone dos botões da sessão.
    pub session_icon_default: Color32F,
    pub session_icon_default_alpha: f32,
    /// Squircle hover do Lock (Honey).
    pub session_lock_hover: Color32F,
    /// Squircle hover do Sleep (Sage).
    pub session_sleep_hover: Color32F,
    /// Squircle hover do Restart (Amber).
    pub session_restart_hover: Color32F,
    /// Squircle hover do Power (Coral).
    pub session_power_hover: Color32F,
    /// Cor da saudação dinâmica ("Bom dia, X").
    pub greeting_color: Color32F,
    /// Cor dos sub-headers FAVORITOS / LUGARES.
    pub column_header_color: Color32F,
    pub column_header_size: f32,
    /// Letter-spacing dos sub-headers (px).
    pub column_header_letter_spacing: i32,
    /// Opacidade do texto em janela inativa.
    pub text_inactive_opacity: f32,
    /// Opacidade do texto em janela minimizada (italic + dim).
    pub text_minimized_opacity: f32,
    /// Cor do divisor (Cream).
    pub divider: Color32F,
    /// Alpha aplicado ao divisor pra fica fininho/sutil.
    pub divider_alpha: f32,
}

#[derive(Debug, Clone)]
pub struct TaskbarTokens {
    pub height: i32,
    pub padding_left: i32,
    pub padding_right: i32,
    pub gap_between_applets: i32,
    pub applet_corner_radius: i32,
    pub border_top_height: i32,
    /// Sub-tokens do launcher (zona A). H33.taskbar.3.
    pub launcher: TaskbarLauncherTokens,
    /// Sub-tokens da zona C (apps abertos). H33.taskbar.4.
    pub apps: TaskbarAppsTokens,
    /// Sub-tokens do tooltip do botão de app. H33.taskbar.6.
    pub tooltip: TaskbarTooltipTokens,
}

/// Sub-tokens do tooltip que aparece acima do botão de app no hover
/// após `apps.tooltip_delay_ms`. H33.taskbar.6.
#[derive(Debug, Clone)]
pub struct TaskbarTooltipTokens {
    pub bg: Color32F,
    pub border_color: Color32F,
    pub border_alpha: f32,
    pub text_color: Color32F,
    pub text_size: f32,
    pub padding: i32,
    pub corner_radius: i32,
    /// Distância vertical (px) entre o topo do botão e a base do tooltip.
    pub margin_above: i32,
}

/// Sub-tokens da zona C (apps abertos) — botões quadrados KDE-style.
/// H33.taskbar.4.
///
/// Cada app aberto vira um quadrado 44×44 com ícone 32px do `.desktop`
/// dentro (ou fallback colorido por hash do nome quando o `.desktop` Icon
/// não pôde ser resolvido). App ativo recebe bg `active_bg` + linha âmbar
/// 24×3 embaixo (commit B.5).
#[derive(Debug, Clone)]
pub struct TaskbarAppsTokens {
    pub button_size: i32,
    pub button_corner_radius: i32,
    pub icon_size: i32,
    pub icon_corner_radius: i32,
    pub gap: i32,
    pub active_bg: Color32F,
    pub hover_bg: Color32F,
    pub indicator_color: Color32F,
    pub indicator_width: i32,
    pub indicator_height: i32,
    /// Delay (ms) antes do tooltip aparecer no hover (commit B.6).
    pub tooltip_delay_ms: u32,
}

/// Sub-tokens da zona A (launcher) na taskbar v5. H33.taskbar.3.
///
/// Container quadrado 44×44 com logo Luma desenhado dentro + divider 1px
/// vertical à direita (separa launcher dos workspaces/apps).
#[derive(Debug, Clone)]
pub struct TaskbarLauncherTokens {
    pub size: i32,
    pub corner_radius: i32,
    pub bg: Color32F,
    pub active_bg: Color32F,
    /// Se true, renderiza divider 1px à direita do launcher.
    pub divider_after: bool,
    /// Alpha do divider (cor base = `colors.divider`).
    pub divider_alpha: f32,
    /// Distância (px) entre a borda direita do launcher e o divider.
    /// Default 6 — respiro visual antes da próxima zona.
    pub divider_gap_left: i32,
}

/// Layout da decoração de janela (header + bordas + sombra). H32.6.
#[derive(Debug, Clone)]
pub struct WindowTokens {
    pub corner_radius: i32,
    pub header_height: i32,
    pub title_size: f32,
    pub title_weight: u16,
    pub title_font_family: String,
    pub title_padding_left: i32,
    pub controls_padding_right: i32,
    pub controls_gap: i32,
    pub shadow_blur: i32,
    pub shadow_offset_y: i32,
}

/// Layout do menu launcher (container). H32.7.
#[derive(Debug, Clone)]
pub struct MenuLauncherTokens {
    pub width: i32,
    pub height: i32,
    pub corner_radius: i32,
    pub border_top_alpha: f32,
    pub shadow_blur: i32,
    pub shadow_offset_y: i32,
    pub shadow_alpha: f32,
    pub animation_duration_ms: u32,
}

/// Backdrop fullscreen + linha âmbar topo do menu híbrido. H32.10.
///
/// O menu novo cobre o output inteiro com um overlay Coffee escuro
/// alpha 0.94 sobre wallpaper/janelas, com uma linha âmbar 2px no
/// topo pra continuidade visual com a taskbar (mesmo amber).
///
/// blur do wallpaper foi planejado no brief (radius 30px) mas o backend
/// winit/GLES não suporta blur shader nativo — fallback é overlay
/// sólido. Ver docs/limitations.md.
#[derive(Debug, Clone)]
pub struct MenuFullscreenTokens {
    pub backdrop_color: Color32F,
    pub backdrop_alpha: f32,
    pub top_accent_color: Color32F,
    pub top_accent_alpha: f32,
    pub top_accent_height: i32,
    pub fade_in_ms: u32,
    pub fade_out_ms: u32,
}

/// Greeting corner (canto sup-dir) do menu fullscreen híbrido. H32.10.6.
///
/// Avatar 44×44 com gradient Honey→Rust (= logoBeam spec) + nome
/// "Murillo" Fraunces ao lado + saudação dinâmica "Bom dia/tarde/noite"
/// embaixo. Tokens emoji-free porque fontdue não tem fallback unicode
/// pros símbolos ☀ ☕ 🌙 — pode adicionar pixel-art ícone depois se
/// quiser igual sun/moon do legacy.
#[derive(Debug, Clone)]
pub struct MenuGreetingTokens {
    pub padding_top: i32,
    pub padding_right: i32,
    pub avatar_size: i32,
    pub avatar_gradient_top: Color32F,
    pub avatar_gradient_bottom: Color32F,
    pub avatar_letter_size: f32,
    pub username_size: f32,
    pub username_color: Color32F,
    pub greeting_size: f32,
    pub greeting_color: Color32F,
    /// Gap horizontal entre avatar e texto.
    pub avatar_text_gap: i32,
    /// Gap vertical entre nome e saudação.
    pub name_greeting_gap: i32,
    /// Textos por período (5-12h, 12-18h, 18-5h).
    pub morning_text: String,
    pub afternoon_text: String,
    pub evening_text: String,
}

/// Search bar pill do menu fullscreen híbrido. H32.10.7.
///
/// Pill 64×500 corner-radius 32, centro horizontal. Auto-focus na
/// abertura do menu — qualquer letra digitada filtra direto sem
/// precisar clicar. Chip "⌘ K" à direita pra reforçar atalho.
#[derive(Debug, Clone)]
pub struct MenuSearchBarTokens {
    pub width: i32,
    pub height: i32,
    pub corner_radius: i32,
    pub bg_default: Color32F,
    pub bg_focused: Color32F,
    pub border_color: Color32F,
    pub border_alpha_default: f32,
    pub border_alpha_focused: f32,
    pub border_width: i32,
    pub icon_size: i32,
    pub icon_color_default: Color32F,
    pub icon_color_focused: Color32F,
    pub text_size: f32,
    pub text_color: Color32F,
    pub placeholder_color: Color32F,
    pub placeholder_alpha: f32,
    pub placeholder_text: String,
    /// Padding interno (esquerda/direita).
    pub padding_horizontal: i32,
    /// Gap entre lupa e texto.
    pub icon_text_gap: i32,
    /// Chip "⌘ K" à direita.
    pub chip_label: String,
    pub chip_width: i32,
    pub chip_height: i32,
    pub chip_bg: Color32F,
    pub chip_color: Color32F,
    pub chip_text_size: f32,
}

/// Grid de favoritos curados (5×2) do menu fullscreen. H32.10.8.
///
/// Cada ícone é um quadrado colorido 80×80 (canto 18) com cor vinda
/// do TOML, não do `.desktop`. Se `icon_inner_color` for `Some`,
/// um quadrado menor 0.45× é desenhado centralizado dentro.
/// Label 12px Cream embaixo (truncated se > label_max_chars).
#[derive(Debug, Clone)]
pub struct MenuFavoritesGridTokens {
    pub columns: i32,
    pub rows: i32,
    pub icon_size: i32,
    pub icon_corner_radius: i32,
    pub icon_inner_scale: f32, // fração de icon_size pro inner square
    pub gap_horizontal: i32,
    pub gap_vertical: i32,
    pub label_size: f32,
    pub label_color: Color32F,
    pub label_margin_top: i32,
    pub label_max_chars: usize,
    /// Halo âmbar atrás do ícone selecionado. Não é gaussian blur (não
    /// temos shader pós-processo) — é um quadrado maior alpha baixo
    /// dando efeito visual de glow. Padding em px ao redor do ícone.
    pub selected_glow_padding: i32,
    pub selected_glow_color: Color32F,
    pub selected_glow_alpha: f32,
    /// Cor do label do item selecionado (âmbar) — substitui label_color.
    pub selected_label_color: Color32F,
}

/// Grid 8×4 paginado de todos os apps (H32.10.11). Ícones menores que
/// o grid de favoritos (60 vs 80) pra caber 32 por página. Usa o
/// `Icon=` real do .desktop (≠ favoritos que usam cor do TOML).
#[derive(Debug, Clone)]
pub struct MenuAllAppsGridTokens {
    pub columns: i32,
    pub rows_per_page: i32,
    pub icon_size: i32,
    pub icon_corner_radius: i32,
    pub gap_horizontal: i32,
    pub gap_vertical: i32,
    pub label_size: f32,
    pub label_color: Color32F,
    pub label_margin_top: i32,
    pub label_max_chars: usize,
    /// Cor de fallback do quadrado pros apps sem ícone resolvido.
    pub fallback_color: Color32F,
    /// Page dots (H32.10.12).
    pub page_dot_size: i32,
    pub page_dot_color_active: Color32F,
    pub page_dot_color_inactive: Color32F,
    pub page_dot_alpha_inactive: f32,
    pub page_dot_gap: i32,
    pub page_dots_margin_top: i32,
    pub page_label_color: Color32F,
    pub page_label_size: f32,
}

/// Link "TODOS OS APPS →" entre grid e session bar (H32.10.13).
/// Texto âmbar com hint cinza embaixo.
#[derive(Debug, Clone)]
pub struct MenuAllAppsLinkTokens {
    pub label_text: String,
    pub label_color: Color32F,
    pub label_size: f32,
    pub hint_text: String,
    pub hint_color: Color32F,
    pub hint_size: f32,
    pub label_hint_gap: i32,
}

/// Session bar bottom com 5 botões + hint + 4 places. H32.10.13.
#[derive(Debug, Clone)]
pub struct MenuSessionBarTokens {
    pub bg_color: Color32F,
    pub bg_alpha: f32,
    pub corner_radius: i32,
    /// Padding interno (esquerda/direita) dentro da bar.
    pub padding_horizontal: i32,
    /// Tamanho do hit area de cada um dos 5 botões.
    pub button_size: i32,
    pub button_corner_radius: i32,
    pub button_gap: i32,
    /// Cor default dos botões (transparent na maioria, Coral no Power).
    pub button_default_color: Color32F,
    pub button_default_alpha: f32,
    pub power_default_color: Color32F,
    /// Cor do hover bg por botão (ordem SessionButton: Lock, Logout,
    /// Sleep, Restart, Power). Honey/Coffee+/Sage/Amber/Amber.
    pub hover_colors: [Color32F; 5],
    pub hover_alpha: f32,
    /// Hint de atalhos no centro (texto cinza).
    pub hint_text: String,
    pub hint_color: Color32F,
    pub hint_size: f32,
    /// Mini-places à direita (4 ícones).
    pub place_icon_size: i32,
    pub place_icon_color: Color32F,
    pub place_gap: i32,
}

/// Acessibilidade PCD first-class. H32.10.17.
///
/// Os campos de `future` são stubs — ligados em commits posteriores
/// quando a infra de input/render comportar. Mantidos aqui pra
/// documentar o contrato com o usuário desde o início.
#[derive(Debug, Clone)]
pub struct AccessibilityTokens {
    /// Default true: setas/Tab/Enter cobrem toda navegação sem mouse.
    pub keyboard_first: bool,
    /// Default true: ao abrir menu, qualquer letra já vai pra search.
    pub auto_focus_search_on_open: bool,
    /// Tamanho mínimo de hit target (WCAG AAA = 44px).
    pub hit_target_min: i32,
    /// Default true: focus ring sempre visível com glow ≥ focus_ring_min_glow.
    pub focus_ring_visible: bool,
    pub focus_ring_min_glow: i32,
    /// Default true: respeita LUMA_REDUCE_MOTION=1 (ou setting do DE
    /// quando suportar) e pula animações.
    pub reduce_motion_respect: bool,

    // ── Stubs futuras (off por default) ──────────────────────────────
    /// Escala global da fonte (1.0 = padrão, 1.5 = large text mode).
    pub large_text_scale: f32,
    /// Modo alto contraste (preto+cream puro). Não implementado.
    pub high_contrast_mode: bool,
    /// Dwell click — segura o cursor parado por N ms = click. Off = 0.
    pub dwell_click_ms: u32,
    /// Sticky modifiers (Ctrl, Alt, Shift permanecem após release).
    pub sticky_modifiers: bool,
    /// Substitui fonte por OpenDyslexic ou similar.
    pub dyslexic_font: bool,
}

/// Profile bar no topo do menu (avatar + username). H32.7.
#[derive(Debug, Clone)]
pub struct MenuProfileBarTokens {
    pub avatar_size: i32,
    pub avatar_radius: i32,
    pub username_font: String,
    pub username_size: f32,
    pub username_weight: u16,
    pub subtitle_size: f32,
    pub padding: i32,
}

/// Search bar do menu. H32.7.
#[derive(Debug, Clone)]
pub struct MenuSearchTokens {
    pub height: i32,
    pub corner_radius: i32,
    pub padding: i32,
    pub placeholder_text: String,
    pub shortcut_keybind: String,
    pub shortcut_label_size: f32,
    pub shortcut_label_bg: Color32F,
}

/// App row (item da lista). H32.7.
#[derive(Debug, Clone)]
pub struct MenuRowTokens {
    pub height: i32,
    pub icon_size: i32,
    pub icon_corner_radius: i32,
    pub icon_text_gap: i32,
    pub focus_indicator_width: i32,
    pub focus_indicator_height: i32,
    pub text_size: f32,
    pub transition_ms: u32,
    pub recently_used_dot_size: i32,
    pub recently_used_max_age_hours: u32,
}

/// Botões da sessão (Lock/Sleep/Restart/Power). H32.7.
#[derive(Debug, Clone)]
pub struct MenuSessionTokens {
    pub hit_area: i32,
    pub gap: i32,
    pub icon_size: i32,
    pub hover_radius: i32,
    pub hover_glow_blur: i32,
    pub hover_glow_alpha: f32,
    pub transition_ms: u32,
}

/// Divider vertical entre as colunas do menu. H32.7.
#[derive(Debug, Clone)]
pub struct MenuColumnDividerTokens {
    pub width: i32,
    pub height_ratio: f32,
    pub gradient: bool,
}

/// Tokens dos workspaces (pager na taskbar + overview screen). H36.
#[derive(Debug, Clone)]
pub struct WorkspacesTokens {
    pub pager_show_if_count: usize,
    pub pager_padding_left: i32,
    pub pager_gap: i32,
    pub pager_dot_radius: i32,
    pub pager_inactive_color: Color32F,
    pub pager_hover_color: Color32F,
    pub pager_active_width: i32,
    pub pager_active_height: i32,
    pub pager_active_color: Color32F,
    pub overview_backdrop: Color32F,
    pub overview_thumb_width: i32,
    pub overview_thumb_height: i32,
    pub overview_thumb_gap: i32,
}

impl Default for WorkspacesTokens {
    fn default() -> Self {
        Self {
            pager_show_if_count: 2,
            pager_padding_left: 12,
            pager_gap: 6,
            pager_dot_radius: 4,
            pager_inactive_color: hex_color("#FBF6EE", 0.25),
            pager_hover_color: hex_color("#FBF6EE", 0.55),
            pager_active_width: 28,
            pager_active_height: 16,
            pager_active_color: hex_color("#E89B3D", 1.0),
            overview_backdrop: hex_color("#1F1611", 0.92),
            overview_thumb_width: 190,
            overview_thumb_height: 120,
            overview_thumb_gap: 20,
        }
    }
}

/// Tokens da search Spotlight-like. H32.9.
#[derive(Debug, Clone)]
pub struct SearchTokens {
    pub debounce_ms: u32,
    pub max_results_per_category: usize,
    /// Score mínimo do fuzzy match (0-1, escalado pra i64 do skim).
    pub fuzzy_threshold: f32,
    pub home_index_depth: usize,
    pub home_index_cache_seconds: u64,
    /// `"duckduckgo"` | `"google"` | `"bing"`.
    pub web_search_engine: String,
    pub apps_accent: Color32F,
    pub folders_accent: Color32F,
    pub calc_accent: Color32F,
    pub web_accent: Color32F,
    pub empty_state_logo_alpha: f32,
    pub placeholder_empty: String,
    pub placeholder_default: String,
    pub show_tips: bool,
    pub empty_title: String,
    pub empty_subtitle: String,
    pub suggest_web_search: bool,
}

/// Modo de detecção dark/light pra escolher cor do header e dos
/// window controls em cada janela. H32.6.
#[derive(Debug, Clone)]
pub struct WindowThemeDetectionTokens {
    /// Estratégia: `"luminance"` (calcula da cor do header), `"client_hint"`
    /// (lê xdg_decoration ou similar — pendente), `"manual_override"` (força
    /// dark/light no theme). Hoje só `luminance` está implementado.
    pub mode: WindowThemeDetectionMode,
    /// Threshold de luminância WCAG: ≥ threshold = light, abaixo = dark.
    /// Default 0.5.
    pub luminance_threshold: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowThemeDetectionMode {
    Luminance,
    ClientHint,
    ManualOverride,
}

#[derive(Debug, Clone)]
pub struct WindowButtonTokens {
    pub max_width: i32,
    pub height: i32,
    pub active_indicator_height: i32,
    pub icon_size: i32,
    pub icon_text_gap: i32,
}

#[derive(Debug, Clone)]
pub struct WindowControlsTokens {
    pub hit_area: i32,
    pub icon_size: i32,
    pub gap: i32,
    pub header_padding_right: i32,
    pub icon_stroke_width: f32,
    pub hover_radius: i32,
}

#[derive(Debug, Clone)]
pub struct ShowDesktopTokens {
    pub width: i32,
    pub border_left_alpha: f32,
    pub hover_bg_alpha: f32,
}

#[derive(Debug, Clone)]
pub struct ClockTokens {
    pub time_size: f32,
    pub date_size: f32,
    pub padding_right: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperMode {
    /// Estica a imagem pra preencher o output (pode distorcer).
    Stretch,
    /// Mantém o tamanho original, centralizada no output.
    Center,
    /// Mantém proporção, escala pra cobrir todo o output.
    Cover,
}

impl Default for Theme {
    fn default() -> Self {
        let colors = ColorTokens::golden_hour();
        Self {
            // Coffee como clear pra combinar com tema.
            clear: [0.122, 0.086, 0.067, 1.0],
            // Border = applet_bg (Coffee mais claro), pra dividers/headers.
            border: colors.applet_bg,
            taskbar_bg: with_alpha(colors.taskbar_bg_top, 0.94),
            taskbar_block_inactive: colors.applet_bg,
            taskbar_block_active: colors.applet_bg_active,
            taskbar_launcher: colors.applet_bg,
            menu_bg: colors.taskbar_bg_top,
            menu_bg_right: darken(colors.taskbar_bg_top, 0.7),
            menu_header_bar: colors.taskbar_bg_bottom,
            menu_item_hover: colors.applet_bg_hover,
            menu_border: with_alpha(colors.divider, colors.divider_alpha),
            wallpaper_mode: WallpaperMode::Stretch,
            colors,
            window: WindowTokens::default(),
            window_theme_detection: WindowThemeDetectionTokens::default(),
            menu_launcher: MenuLauncherTokens::default(),
            menu_fullscreen: MenuFullscreenTokens::default(),
            menu_greeting: MenuGreetingTokens::default(),
            menu_search_bar: MenuSearchBarTokens::default(),
            menu_favorites: MenuFavoritesGridTokens::default(),
            menu_all_apps: MenuAllAppsGridTokens::default(),
            menu_all_apps_link: MenuAllAppsLinkTokens::default(),
            menu_session_bar: MenuSessionBarTokens::default(),
            accessibility: AccessibilityTokens::default(),
            menu_profile_bar: MenuProfileBarTokens::default(),
            menu_search: MenuSearchTokens::default(),
            menu_row: MenuRowTokens::default(),
            menu_session: MenuSessionTokens::default(),
            menu_column_divider: MenuColumnDividerTokens::default(),
            search: SearchTokens::default(),
            workspaces: WorkspacesTokens::default(),
            taskbar: TaskbarTokens::default(),
            window_button: WindowButtonTokens::default(),
            window_controls: WindowControlsTokens::default(),
            show_desktop: ShowDesktopTokens::default(),
            clock: ClockTokens::default(),
        }
    }
}

impl ColorTokens {
    /// Paleta Golden Hour: Coffee #1F1611, Coral #D97757, Amber #E89B3D,
    /// Cream #FBF6EE, Sage #6B8E7F, Honey #F5D5A8.
    fn golden_hour() -> Self {
        Self {
            taskbar_bg_top: hex_color("#1F1611", 0.94),
            taskbar_bg_bottom: hex_color("#0F0A07", 0.97),
            applet_bg: hex_color("#2A1F1A", 1.0),
            applet_bg_hover: hex_color("#3D2520", 1.0),
            applet_bg_active: hex_color("#3D2520", 1.0),
            accent_amber: hex_color("#E89B3D", 1.0),
            // H33.taskbar.2: alpha do top accent subiu de 0.25 → 0.5.
            accent_amber_alpha_top: 0.5,
            accent_amber_glow: 3,
            hover_minimize: hex_color("#F5D5A8", 1.0),
            hover_maximize: hex_color("#6B8E7F", 1.0),
            hover_close: hex_color("#D97757", 1.0),
            hover_glow_radius: 4,
            hover_glow_opacity: 0.4,
            text_primary: hex_color("#FBF6EE", 1.0),
            text_secondary: hex_color("#A89D92", 1.0),
            text_inactive_opacity: 0.70,
            text_minimized_opacity: 0.40,
            window_control_icon_light: hex_color("#FBF6EE", 1.0),
            window_control_light_alpha: 0.85,
            window_control_icon_dark: hex_color("#1F1611", 1.0),
            window_control_dark_alpha: 0.70,
            // H32.6 — decoração de janela
            window_header_bg_dark: hex_color("#2A1F1A", 1.0),
            window_header_bg_light: hex_color("#E8DDD0", 1.0),
            window_bg_dark: hex_color("#1F1611", 1.0),
            window_bg_light: hex_color("#FBF6EE", 1.0),
            window_border_focused: hex_color("#E89B3D", 1.0),
            window_border_focused_alpha: 0.7,
            window_border_focused_glow: 0.5,
            window_border_unfocused: hex_color("#FBF6EE", 1.0),
            window_border_unfocused_alpha: 0.08,
            window_header_top_accent: hex_color("#E89B3D", 1.0),
            window_header_top_alpha: 0.4,
            window_title_color_dark: hex_color("#FBF6EE", 1.0),
            window_title_color_light: hex_color("#1F1611", 1.0),
            window_title_unfocused_alpha: 0.55,
            window_shadow_color: hex_color("#000000", 1.0),
            window_shadow_alpha: 0.4,
            window_shadow_focused_multiplier: 1.2,
            // H32.7 — menu launcher
            menu_bg_top: hex_color("#1F1611", 1.0),
            menu_bg_top_alpha: 0.97,
            menu_bg_bottom: hex_color("#0F0A07", 1.0),
            menu_bg_bottom_alpha: 0.98,
            search_bg: hex_color("#2A1F1A", 1.0),
            search_bg_focused: hex_color("#3D2520", 1.0),
            search_border_focused: hex_color("#E89B3D", 1.0),
            search_border_focused_width: 1.5,
            search_placeholder: hex_color("#A89D92", 1.0),
            search_text: hex_color("#FBF6EE", 1.0),
            search_icon: hex_color("#A89D92", 1.0),
            search_icon_focused: hex_color("#E89B3D", 1.0),
            row_bg_hover: hex_color("#3D2520", 1.0),
            row_bg_keyboard_focus: hex_color("#3D2520", 1.0),
            row_focus_indicator: hex_color("#E89B3D", 1.0),
            row_text: hex_color("#FBF6EE", 1.0),
            row_text_focused_weight: 500,
            row_recently_used_dot: hex_color("#E89B3D", 1.0),
            session_bg: hex_color("#0F0A07", 1.0),
            session_icon_default: hex_color("#FBF6EE", 1.0),
            session_icon_default_alpha: 0.7,
            session_lock_hover: hex_color("#F5D5A8", 1.0),
            session_sleep_hover: hex_color("#6B8E7F", 1.0),
            session_restart_hover: hex_color("#E89B3D", 1.0),
            session_power_hover: hex_color("#D97757", 1.0),
            greeting_color: hex_color("#A89D92", 1.0),
            column_header_color: hex_color("#8B7D72", 1.0),
            column_header_size: 10.0,
            column_header_letter_spacing: 2,
            divider: hex_color("#FBF6EE", 1.0),
            divider_alpha: 0.15,
        }
    }
}

impl Default for WindowTokens {
    fn default() -> Self {
        Self {
            corner_radius: 8,
            header_height: 40,
            title_size: 13.0,
            title_weight: 500,
            title_font_family: String::from("Manrope"),
            title_padding_left: 16,
            controls_padding_right: 12,
            controls_gap: 6,
            shadow_blur: 8,
            shadow_offset_y: 4,
        }
    }
}

impl Default for WindowThemeDetectionTokens {
    fn default() -> Self {
        Self {
            mode: WindowThemeDetectionMode::Luminance,
            luminance_threshold: 0.5,
        }
    }
}

impl Default for MenuLauncherTokens {
    fn default() -> Self {
        Self {
            width: 560,
            height: 540,
            corner_radius: 10,
            // Linha âmbar 1px topo (assinatura visual). 0.5 deixa
            // perceptível sobre o menu_bg dark. H32.7.fix.6.
            border_top_alpha: 0.5,
            shadow_blur: 12,
            shadow_offset_y: 6,
            shadow_alpha: 0.4,
            animation_duration_ms: 150,
        }
    }
}

impl Default for MenuFullscreenTokens {
    fn default() -> Self {
        Self {
            backdrop_color: hex_color("#1F1611", 1.0),
            backdrop_alpha: 0.94,
            top_accent_color: hex_color("#E89B3D", 1.0),
            top_accent_alpha: 0.6,
            top_accent_height: 2,
            fade_in_ms: 200,
            fade_out_ms: 150,
        }
    }
}

impl Default for MenuGreetingTokens {
    fn default() -> Self {
        Self {
            padding_top: 40,
            padding_right: 60,
            avatar_size: 56,
            avatar_gradient_top: hex_color("#F5D5A8", 1.0),    // Honey
            avatar_gradient_bottom: hex_color("#9F4A35", 1.0), // Rust
            avatar_letter_size: 24.0,
            username_size: 18.0,
            username_color: hex_color("#FBF6EE", 1.0),
            greeting_size: 14.0,
            greeting_color: hex_color("#A89D92", 1.0),
            avatar_text_gap: 12,
            name_greeting_gap: 4,
            morning_text: String::from("Bom dia"),
            afternoon_text: String::from("Boa tarde"),
            evening_text: String::from("Boa noite"),
        }
    }
}

impl Default for MenuSearchBarTokens {
    fn default() -> Self {
        Self {
            width: 500,
            height: 64,
            corner_radius: 32,
            bg_default: hex_color("#2A1F1A", 1.0),
            bg_focused: hex_color("#3D2520", 1.0),
            border_color: hex_color("#E89B3D", 1.0),
            border_alpha_default: 0.4,
            border_alpha_focused: 1.0,
            border_width: 2,
            icon_size: 20,
            icon_color_default: hex_color("#A89D92", 1.0),
            icon_color_focused: hex_color("#E89B3D", 1.0),
            text_size: 18.0,
            text_color: hex_color("#FBF6EE", 1.0),
            placeholder_color: hex_color("#A89D92", 1.0),
            placeholder_alpha: 0.6,
            placeholder_text: String::from("Buscar apps..."),
            padding_horizontal: 24,
            icon_text_gap: 14,
            chip_label: String::from("Super K"),
            chip_width: 70,
            chip_height: 22,
            chip_bg: hex_color("#1F1611", 1.0),
            chip_color: hex_color("#E89B3D", 1.0),
            chip_text_size: 10.0,
        }
    }
}

impl Default for MenuFavoritesGridTokens {
    fn default() -> Self {
        Self {
            columns: 5,
            rows: 2,
            icon_size: 80,
            icon_corner_radius: 18,
            icon_inner_scale: 0.45,
            gap_horizontal: 60,
            gap_vertical: 60,
            label_size: 12.0,
            label_color: hex_color("#FBF6EE", 1.0),
            label_margin_top: 20,
            label_max_chars: 12,
            selected_glow_padding: 12,
            selected_glow_color: hex_color("#E89B3D", 1.0),
            selected_glow_alpha: 0.7,
            selected_label_color: hex_color("#E89B3D", 1.0),
        }
    }
}

impl Default for MenuAllAppsGridTokens {
    fn default() -> Self {
        Self {
            columns: 8,
            rows_per_page: 4,
            icon_size: 60,
            icon_corner_radius: 14,
            gap_horizontal: 22,
            gap_vertical: 16,
            label_size: 10.0,
            label_color: hex_color("#FBF6EE", 1.0),
            label_margin_top: 8,
            label_max_chars: 10,
            fallback_color: hex_color("#3D2520", 1.0),
            page_dot_size: 5,
            page_dot_color_active: hex_color("#E89B3D", 1.0),
            page_dot_color_inactive: hex_color("#FBF6EE", 1.0),
            page_dot_alpha_inactive: 0.25,
            page_dot_gap: 12,
            page_dots_margin_top: 24,
            page_label_color: hex_color("#A89D92", 1.0),
            page_label_size: 10.0,
        }
    }
}

impl Default for MenuAllAppsLinkTokens {
    fn default() -> Self {
        Self {
            // ASCII-safe: → não rasteriza no font atual (vira □).
            label_text: String::from("TODOS OS APPS"),
            label_color: hex_color("#E89B3D", 1.0),
            label_size: 13.0,
            hint_text: String::from("clique ou pressione Tab"),
            hint_color: hex_color("#A89D92", 1.0),
            hint_size: 10.0,
            label_hint_gap: 4,
        }
    }
}

impl Default for MenuSessionBarTokens {
    fn default() -> Self {
        Self {
            bg_color: hex_color("#0F0A07", 1.0),
            bg_alpha: 0.7,
            corner_radius: 12,
            padding_horizontal: 20,
            button_size: 44,
            button_corner_radius: 8,
            button_gap: 12,
            button_default_color: hex_color("#FBF6EE", 1.0),
            button_default_alpha: 0.0, // transparent (= invisível) por default
            power_default_color: hex_color("#D97757", 1.0), // Coral
            hover_colors: [
                hex_color("#F5D5A8", 1.0), // Lock: Honey
                hex_color("#3D2520", 1.0), // Logout: Coffee+
                hex_color("#6B8E7F", 1.0), // Sleep: Sage
                hex_color("#E89B3D", 1.0), // Restart: Amber
                hex_color("#E89B3D", 1.0), // Power: Amber (Coral fica como default)
            ],
            hover_alpha: 0.9,
            // ASCII-safe: fontdue carrega 1 font único sem fallback stack,
            // então ↑↓↵ ficavam □□. Texto puro garante render em
            // qualquer fonte. H32.10.polish.1.
            hint_text: String::from("Setas navegar  ·  Enter abrir  ·  Esc fechar  ·  Tab ver todos"),
            hint_color: hex_color("#A89D92", 1.0),
            hint_size: 11.0,
            place_icon_size: 18,
            place_icon_color: hex_color("#A89D92", 1.0),
            place_gap: 22,
        }
    }
}

impl Default for AccessibilityTokens {
    fn default() -> Self {
        Self {
            keyboard_first: true,
            auto_focus_search_on_open: true,
            hit_target_min: 44, // WCAG AAA
            focus_ring_visible: true,
            focus_ring_min_glow: 4,
            reduce_motion_respect: true,
            large_text_scale: 1.0,
            high_contrast_mode: false,
            dwell_click_ms: 0,
            sticky_modifiers: false,
            dyslexic_font: false,
        }
    }
}

impl Default for MenuProfileBarTokens {
    fn default() -> Self {
        Self {
            avatar_size: 40,
            avatar_radius: 20,
            username_font: String::from("Fraunces"),
            username_size: 18.0,
            username_weight: 600,
            subtitle_size: 11.0,
            padding: 20,
        }
    }
}

impl Default for MenuSearchTokens {
    fn default() -> Self {
        Self {
            height: 40,
            corner_radius: 8,
            padding: 12,
            placeholder_text: String::from("Buscar apps, lugares, configurações..."),
            shortcut_keybind: String::from("Super+K"),
            shortcut_label_size: 9.0,
            shortcut_label_bg: hex_color("#1F1611", 1.0),
        }
    }
}

impl Default for MenuRowTokens {
    fn default() -> Self {
        Self {
            height: 36,
            icon_size: 20,
            icon_corner_radius: 4,
            icon_text_gap: 12,
            focus_indicator_width: 3,
            focus_indicator_height: 24,
            text_size: 13.0,
            transition_ms: 120,
            recently_used_dot_size: 3,
            recently_used_max_age_hours: 24,
        }
    }
}

impl Default for MenuSessionTokens {
    fn default() -> Self {
        Self {
            hit_area: 32,
            gap: 8,
            icon_size: 16,
            hover_radius: 6,
            hover_glow_blur: 4,
            hover_glow_alpha: 0.4,
            transition_ms: 120,
        }
    }
}

impl Default for MenuColumnDividerTokens {
    fn default() -> Self {
        Self {
            width: 1,
            height_ratio: 0.9,
            gradient: true,
        }
    }
}

impl Default for SearchTokens {
    fn default() -> Self {
        Self {
            debounce_ms: 80,
            max_results_per_category: 5,
            fuzzy_threshold: 0.3,
            home_index_depth: 3,
            home_index_cache_seconds: 300,
            web_search_engine: "duckduckgo".to_string(),
            apps_accent: hex_color("#E89B3D", 1.0),
            folders_accent: hex_color("#F5D5A8", 1.0),
            calc_accent: hex_color("#6B8E7F", 1.0),
            web_accent: hex_color("#D97757", 1.0),
            empty_state_logo_alpha: 0.25,
            placeholder_empty: "Comece a digitar...".to_string(),
            placeholder_default: "Buscar apps, lugares, configurações...".to_string(),
            show_tips: true,
            empty_title: "Nada por aqui".to_string(),
            empty_subtitle: "Sem resultados".to_string(),
            suggest_web_search: true,
        }
    }
}

impl Default for TaskbarTokens {
    fn default() -> Self {
        Self {
            // H33.taskbar.2: altura 64 (era 48) + border top 2px (era 1).
            height: 64,
            padding_left: 12,
            padding_right: 12,
            gap_between_applets: 8,
            applet_corner_radius: 9,
            border_top_height: 2,
            launcher: TaskbarLauncherTokens::default(),
            apps: TaskbarAppsTokens::default(),
            tooltip: TaskbarTooltipTokens::default(),
        }
    }
}

impl Default for TaskbarTooltipTokens {
    fn default() -> Self {
        Self {
            // #1F1611 — taskbar_bg_top (Coffee).
            bg: hex_color("#1F1611", 1.0),
            // Âmbar com alpha 0.3 (sutil).
            border_color: hex_color("#E89B3D", 1.0),
            border_alpha: 0.3,
            // Cream #FBF6EE.
            text_color: hex_color("#FBF6EE", 1.0),
            text_size: 11.0,
            padding: 10,
            corner_radius: 6,
            margin_above: 8,
        }
    }
}

impl Default for TaskbarAppsTokens {
    fn default() -> Self {
        Self {
            button_size: 44,
            button_corner_radius: 9,
            icon_size: 32,
            icon_corner_radius: 6,
            gap: 8,
            // #3D2520 — applet_bg_active.
            active_bg: hex_color("#3D2520", 1.0),
            // #2A1F1A — applet_bg.
            hover_bg: hex_color("#2A1F1A", 1.0),
            // Âmbar Golden Hour.
            indicator_color: hex_color("#E89B3D", 1.0),
            indicator_width: 24,
            indicator_height: 3,
            tooltip_delay_ms: 500,
        }
    }
}

impl Default for TaskbarLauncherTokens {
    fn default() -> Self {
        Self {
            size: 44,
            corner_radius: 9,
            // #3D2520 — applet_bg_active da Golden Hour.
            bg: hex_color("#3D2520", 1.0),
            // #2A1F1A — applet_bg.
            active_bg: hex_color("#2A1F1A", 1.0),
            divider_after: true,
            divider_alpha: 0.1,
            divider_gap_left: 6,
        }
    }
}

impl Default for WindowButtonTokens {
    fn default() -> Self {
        Self {
            max_width: 220,
            height: 32,
            active_indicator_height: 3,
            icon_size: 16,
            icon_text_gap: 8,
        }
    }
}

impl Default for WindowControlsTokens {
    fn default() -> Self {
        Self {
            hit_area: 24,
            icon_size: 12,
            gap: 6,
            header_padding_right: 12,
            icon_stroke_width: 1.5,
            hover_radius: 6,
        }
    }
}

impl Default for ShowDesktopTokens {
    fn default() -> Self {
        Self {
            width: 10,
            border_left_alpha: 0.15,
            hover_bg_alpha: 0.3,
        }
    }
}

impl Default for ClockTokens {
    fn default() -> Self {
        Self {
            time_size: 13.0,
            date_size: 9.0,
            padding_right: 16,
        }
    }
}

impl Theme {
    /// Decide se a cor do header de uma janela é DARK (true) ou LIGHT (false)
    /// pra escolher cor do título, dos window controls e da borda. H32.6.
    ///
    /// Modo `Luminance`: usa a fórmula WCAG sobre `header_bg` recebida.
    /// Modos `ClientHint` e `ManualOverride` ainda fazem fallback pra
    /// `Luminance` (débito documentado em visual-debt.md).
    pub fn is_dark_header(&self, header_bg: Color32F) -> bool {
        match self.window_theme_detection.mode {
            WindowThemeDetectionMode::Luminance
            | WindowThemeDetectionMode::ClientHint
            | WindowThemeDetectionMode::ManualOverride => {
                wcag_luminance(header_bg) < self.window_theme_detection.luminance_threshold
            }
        }
    }
}

/// Luminância WCAG 2.0 (sRGB→linear + pesos rec.709). Range 0.0–1.0.
/// Usada por `Theme::is_dark_header`. H32.6.1.
pub fn wcag_luminance(c: Color32F) -> f32 {
    fn to_linear(channel: f32) -> f32 {
        if channel <= 0.03928 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * to_linear(c.r()) + 0.7152 * to_linear(c.g()) + 0.0722 * to_linear(c.b())
}

impl Theme {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        if !path.exists() {
            info!("luma.toml não encontrado em {:?}, usando tema padrão Golden Hour", path);
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<ThemeFile>(&content) {
                Ok(file) => {
                    info!("tema carregado de {:?}", path);
                    file.into_theme()
                }
                Err(err) => {
                    warn!(
                        "erro parseando luma.toml: {err}, usando tema padrão Golden Hour"
                    );
                    Self::default()
                }
            },
            Err(err) => {
                warn!("erro lendo luma.toml: {err}, usando tema padrão Golden Hour");
                Self::default()
            }
        }
    }
}

/// Caminho do arquivo de tema: `$XDG_CONFIG_HOME/luma/luma.toml` ou
/// `~/.config/luma/luma.toml` como fallback.
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("luma").join("luma.toml"))
}

/// Schema do arquivo TOML. Suporta dois formatos:
/// 1. Campos legados no topo (compat pré-H32.5): `taskbar_bg = "#..."` etc
/// 2. Sub-tabelas H32.5: `[colors]`, `[taskbar]`, `[window_button]`,
///    `[window_controls]`, `[show_desktop]`, `[clock]`
///
/// Tudo opcional. Faltando = default Golden Hour ou derivado.
#[derive(Debug, Deserialize, Default)]
struct ThemeFile {
    // Legados pré-H32.5 (mantém compat com luma.toml antigos do user).
    clear: Option<String>,
    border: Option<String>,
    taskbar_bg: Option<String>,
    taskbar_block_inactive: Option<String>,
    taskbar_block_active: Option<String>,
    taskbar_launcher: Option<String>,
    menu_bg: Option<String>,
    menu_bg_right: Option<String>,
    menu_header_bar: Option<String>,
    menu_item_hover: Option<String>,
    menu_border: Option<String>,
    wallpaper_mode: Option<String>,

    // Tokens H32.5/.6/.7 — sub-tabelas opcionais.
    colors: Option<ColorTokensFile>,
    window: Option<WindowTokensFile>,
    menu: Option<MenuFile>,
    taskbar: Option<TaskbarTokensFile>,
    window_button: Option<WindowButtonTokensFile>,
    window_controls: Option<WindowControlsTokensFile>,
    show_desktop: Option<ShowDesktopTokensFile>,
    clock: Option<ClockTokensFile>,
}

#[derive(Debug, Deserialize, Default)]
struct ColorTokensFile {
    taskbar_bg_top: Option<String>,
    taskbar_bg_top_alpha: Option<f32>,
    taskbar_bg_bottom: Option<String>,
    taskbar_bg_bottom_alpha: Option<f32>,
    applet_bg: Option<String>,
    applet_bg_hover: Option<String>,
    applet_bg_active: Option<String>,
    accent_amber: Option<String>,
    accent_amber_alpha_top: Option<f32>,
    accent_amber_glow: Option<i32>,
    hover_minimize: Option<String>,
    hover_maximize: Option<String>,
    hover_close: Option<String>,
    hover_glow_radius: Option<i32>,
    hover_glow_opacity: Option<f32>,
    text_primary: Option<String>,
    text_secondary: Option<String>,
    text_inactive_opacity: Option<f32>,
    text_minimized_opacity: Option<f32>,
    window_control_icon_light: Option<String>,
    window_control_light_alpha: Option<f32>,
    window_control_icon_dark: Option<String>,
    window_control_dark_alpha: Option<f32>,
    // H32.6
    window_header_bg_dark: Option<String>,
    window_header_bg_light: Option<String>,
    window_bg_dark: Option<String>,
    window_bg_light: Option<String>,
    window_border_focused: Option<String>,
    window_border_focused_alpha: Option<f32>,
    window_border_focused_glow: Option<f32>,
    window_border_unfocused: Option<String>,
    window_border_unfocused_alpha: Option<f32>,
    window_header_top_accent: Option<String>,
    window_header_top_alpha: Option<f32>,
    window_title_color_dark: Option<String>,
    window_title_color_light: Option<String>,
    window_title_unfocused_alpha: Option<f32>,
    window_shadow_color: Option<String>,
    window_shadow_alpha: Option<f32>,
    window_shadow_focused_multiplier: Option<f32>,
    // H32.7
    menu_bg_top: Option<String>,
    menu_bg_top_alpha: Option<f32>,
    menu_bg_bottom: Option<String>,
    menu_bg_bottom_alpha: Option<f32>,
    search_bg: Option<String>,
    search_bg_focused: Option<String>,
    search_border_focused: Option<String>,
    search_border_focused_width: Option<f32>,
    search_placeholder: Option<String>,
    search_text: Option<String>,
    search_icon: Option<String>,
    search_icon_focused: Option<String>,
    row_bg_hover: Option<String>,
    row_bg_keyboard_focus: Option<String>,
    row_focus_indicator: Option<String>,
    row_text: Option<String>,
    row_text_focused_weight: Option<u16>,
    row_recently_used_dot: Option<String>,
    session_bg: Option<String>,
    session_icon_default: Option<String>,
    session_icon_default_alpha: Option<f32>,
    session_lock_hover: Option<String>,
    session_sleep_hover: Option<String>,
    session_restart_hover: Option<String>,
    session_power_hover: Option<String>,
    greeting_color: Option<String>,
    column_header_color: Option<String>,
    column_header_size: Option<f32>,
    column_header_letter_spacing: Option<i32>,
    divider: Option<String>,
    divider_alpha: Option<f32>,
}

#[derive(Debug, Deserialize, Default)]
struct WindowTokensFile {
    corner_radius: Option<i32>,
    header_height: Option<i32>,
    title_size: Option<f32>,
    title_weight: Option<u16>,
    title_font_family: Option<String>,
    title_padding_left: Option<i32>,
    controls_padding_right: Option<i32>,
    controls_gap: Option<i32>,
    shadow_blur: Option<i32>,
    shadow_offset_y: Option<i32>,
    theme_detection: Option<WindowThemeDetectionFile>,
}

#[derive(Debug, Deserialize, Default)]
struct WindowThemeDetectionFile {
    mode: Option<String>,
    luminance_threshold: Option<f32>,
}

impl WindowTokensFile {
    fn into_tokens(self, def: &WindowTokens) -> WindowTokens {
        WindowTokens {
            corner_radius: self.corner_radius.unwrap_or(def.corner_radius),
            header_height: self.header_height.unwrap_or(def.header_height),
            title_size: self.title_size.unwrap_or(def.title_size),
            title_weight: self.title_weight.unwrap_or(def.title_weight),
            title_font_family: self
                .title_font_family
                .unwrap_or_else(|| def.title_font_family.clone()),
            title_padding_left: self
                .title_padding_left
                .unwrap_or(def.title_padding_left),
            controls_padding_right: self
                .controls_padding_right
                .unwrap_or(def.controls_padding_right),
            controls_gap: self.controls_gap.unwrap_or(def.controls_gap),
            shadow_blur: self.shadow_blur.unwrap_or(def.shadow_blur),
            shadow_offset_y: self.shadow_offset_y.unwrap_or(def.shadow_offset_y),
        }
    }
}

impl WindowThemeDetectionFile {
    fn into_tokens(self, def: &WindowThemeDetectionTokens) -> WindowThemeDetectionTokens {
        let mode = self
            .mode
            .as_deref()
            .map(parse_window_theme_mode)
            .unwrap_or(def.mode);
        WindowThemeDetectionTokens {
            mode,
            luminance_threshold: self
                .luminance_threshold
                .unwrap_or(def.luminance_threshold),
        }
    }
}

fn parse_window_theme_mode(s: &str) -> WindowThemeDetectionMode {
    match s.trim().to_ascii_lowercase().as_str() {
        "luminance" => WindowThemeDetectionMode::Luminance,
        "client_hint" => WindowThemeDetectionMode::ClientHint,
        "manual_override" => WindowThemeDetectionMode::ManualOverride,
        other => {
            warn!(
                "theme: window.theme_detection.mode desconhecido {:?}, usando luminance",
                other
            );
            WindowThemeDetectionMode::Luminance
        }
    }
}

// ── H32.7 — menu launcher parser ──

#[derive(Debug, Deserialize, Default)]
struct MenuFile {
    width: Option<i32>,
    height: Option<i32>,
    corner_radius: Option<i32>,
    border_top_alpha: Option<f32>,
    shadow_blur: Option<i32>,
    shadow_offset_y: Option<i32>,
    shadow_alpha: Option<f32>,
    animation_duration_ms: Option<u32>,
    profile_bar: Option<MenuProfileBarFile>,
    search: Option<MenuSearchFile>,
    row: Option<MenuRowFile>,
    session: Option<MenuSessionFile>,
    column_divider: Option<MenuColumnDividerFile>,
}

#[derive(Debug, Deserialize, Default)]
struct MenuProfileBarFile {
    avatar_size: Option<i32>,
    avatar_radius: Option<i32>,
    username_font: Option<String>,
    username_size: Option<f32>,
    username_weight: Option<u16>,
    subtitle_size: Option<f32>,
    padding: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
struct MenuSearchFile {
    height: Option<i32>,
    corner_radius: Option<i32>,
    padding: Option<i32>,
    placeholder_text: Option<String>,
    shortcut_keybind: Option<String>,
    shortcut_label_size: Option<f32>,
    shortcut_label_bg: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct MenuRowFile {
    height: Option<i32>,
    icon_size: Option<i32>,
    icon_corner_radius: Option<i32>,
    icon_text_gap: Option<i32>,
    focus_indicator_width: Option<i32>,
    focus_indicator_height: Option<i32>,
    text_size: Option<f32>,
    transition_ms: Option<u32>,
    recently_used_dot_size: Option<i32>,
    recently_used_max_age_hours: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct MenuSessionFile {
    hit_area: Option<i32>,
    gap: Option<i32>,
    icon_size: Option<i32>,
    hover_radius: Option<i32>,
    hover_glow_blur: Option<i32>,
    hover_glow_alpha: Option<f32>,
    transition_ms: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct MenuColumnDividerFile {
    width: Option<i32>,
    height_ratio: Option<f32>,
    gradient: Option<bool>,
}

impl MenuFile {
    fn into_launcher(self, def: &MenuLauncherTokens) -> MenuLauncherTokens {
        MenuLauncherTokens {
            width: self.width.unwrap_or(def.width),
            height: self.height.unwrap_or(def.height),
            corner_radius: self.corner_radius.unwrap_or(def.corner_radius),
            border_top_alpha: self.border_top_alpha.unwrap_or(def.border_top_alpha),
            shadow_blur: self.shadow_blur.unwrap_or(def.shadow_blur),
            shadow_offset_y: self.shadow_offset_y.unwrap_or(def.shadow_offset_y),
            shadow_alpha: self.shadow_alpha.unwrap_or(def.shadow_alpha),
            animation_duration_ms: self
                .animation_duration_ms
                .unwrap_or(def.animation_duration_ms),
        }
    }
}

impl MenuProfileBarFile {
    fn into_tokens(self, def: &MenuProfileBarTokens) -> MenuProfileBarTokens {
        MenuProfileBarTokens {
            avatar_size: self.avatar_size.unwrap_or(def.avatar_size),
            avatar_radius: self.avatar_radius.unwrap_or(def.avatar_radius),
            username_font: self
                .username_font
                .unwrap_or_else(|| def.username_font.clone()),
            username_size: self.username_size.unwrap_or(def.username_size),
            username_weight: self.username_weight.unwrap_or(def.username_weight),
            subtitle_size: self.subtitle_size.unwrap_or(def.subtitle_size),
            padding: self.padding.unwrap_or(def.padding),
        }
    }
}

impl MenuSearchFile {
    fn into_tokens(self, def: &MenuSearchTokens) -> MenuSearchTokens {
        MenuSearchTokens {
            height: self.height.unwrap_or(def.height),
            corner_radius: self.corner_radius.unwrap_or(def.corner_radius),
            padding: self.padding.unwrap_or(def.padding),
            placeholder_text: self
                .placeholder_text
                .unwrap_or_else(|| def.placeholder_text.clone()),
            shortcut_keybind: self
                .shortcut_keybind
                .unwrap_or_else(|| def.shortcut_keybind.clone()),
            shortcut_label_size: self
                .shortcut_label_size
                .unwrap_or(def.shortcut_label_size),
            shortcut_label_bg: self
                .shortcut_label_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.shortcut_label_bg),
        }
    }
}

impl MenuRowFile {
    fn into_tokens(self, def: &MenuRowTokens) -> MenuRowTokens {
        MenuRowTokens {
            height: self.height.unwrap_or(def.height),
            icon_size: self.icon_size.unwrap_or(def.icon_size),
            icon_corner_radius: self
                .icon_corner_radius
                .unwrap_or(def.icon_corner_radius),
            icon_text_gap: self.icon_text_gap.unwrap_or(def.icon_text_gap),
            focus_indicator_width: self
                .focus_indicator_width
                .unwrap_or(def.focus_indicator_width),
            focus_indicator_height: self
                .focus_indicator_height
                .unwrap_or(def.focus_indicator_height),
            text_size: self.text_size.unwrap_or(def.text_size),
            transition_ms: self.transition_ms.unwrap_or(def.transition_ms),
            recently_used_dot_size: self
                .recently_used_dot_size
                .unwrap_or(def.recently_used_dot_size),
            recently_used_max_age_hours: self
                .recently_used_max_age_hours
                .unwrap_or(def.recently_used_max_age_hours),
        }
    }
}

impl MenuSessionFile {
    fn into_tokens(self, def: &MenuSessionTokens) -> MenuSessionTokens {
        MenuSessionTokens {
            hit_area: self.hit_area.unwrap_or(def.hit_area),
            gap: self.gap.unwrap_or(def.gap),
            icon_size: self.icon_size.unwrap_or(def.icon_size),
            hover_radius: self.hover_radius.unwrap_or(def.hover_radius),
            hover_glow_blur: self.hover_glow_blur.unwrap_or(def.hover_glow_blur),
            hover_glow_alpha: self.hover_glow_alpha.unwrap_or(def.hover_glow_alpha),
            transition_ms: self.transition_ms.unwrap_or(def.transition_ms),
        }
    }
}

impl MenuColumnDividerFile {
    fn into_tokens(self, def: &MenuColumnDividerTokens) -> MenuColumnDividerTokens {
        MenuColumnDividerTokens {
            width: self.width.unwrap_or(def.width),
            height_ratio: self.height_ratio.unwrap_or(def.height_ratio),
            gradient: self.gradient.unwrap_or(def.gradient),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct TaskbarTokensFile {
    height: Option<i32>,
    padding_left: Option<i32>,
    padding_right: Option<i32>,
    gap_between_applets: Option<i32>,
    applet_corner_radius: Option<i32>,
    border_top_height: Option<i32>,
    launcher: Option<TaskbarLauncherTokensFile>,
    apps: Option<TaskbarAppsTokensFile>,
    tooltip: Option<TaskbarTooltipTokensFile>,
}

#[derive(Debug, Deserialize, Default)]
struct TaskbarTooltipTokensFile {
    bg: Option<String>,
    border_color: Option<String>,
    border_alpha: Option<f32>,
    text_color: Option<String>,
    text_size: Option<f32>,
    padding: Option<i32>,
    corner_radius: Option<i32>,
    margin_above: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
struct TaskbarAppsTokensFile {
    button_size: Option<i32>,
    button_corner_radius: Option<i32>,
    icon_size: Option<i32>,
    icon_corner_radius: Option<i32>,
    gap: Option<i32>,
    active_bg: Option<String>,
    hover_bg: Option<String>,
    indicator_color: Option<String>,
    indicator_width: Option<i32>,
    indicator_height: Option<i32>,
    tooltip_delay_ms: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct TaskbarLauncherTokensFile {
    size: Option<i32>,
    corner_radius: Option<i32>,
    bg: Option<String>,
    active_bg: Option<String>,
    divider_after: Option<bool>,
    divider_alpha: Option<f32>,
    divider_gap_left: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
struct WindowButtonTokensFile {
    max_width: Option<i32>,
    height: Option<i32>,
    active_indicator_height: Option<i32>,
    icon_size: Option<i32>,
    icon_text_gap: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
struct WindowControlsTokensFile {
    hit_area: Option<i32>,
    icon_size: Option<i32>,
    gap: Option<i32>,
    header_padding_right: Option<i32>,
    icon_stroke_width: Option<f32>,
    hover_radius: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
struct ShowDesktopTokensFile {
    width: Option<i32>,
    border_left_alpha: Option<f32>,
    hover_bg_alpha: Option<f32>,
}

#[derive(Debug, Deserialize, Default)]
struct ClockTokensFile {
    time_size: Option<f32>,
    date_size: Option<f32>,
    padding_right: Option<i32>,
}

impl ThemeFile {
    fn into_theme(self) -> Theme {
        let default = Theme::default();
        let colors = self
            .colors
            .map(|c| c.into_tokens(&default.colors))
            .unwrap_or_else(|| default.colors.clone());
        let (window, window_theme_detection) = match self.window {
            Some(w) => {
                let detection = w
                    .theme_detection
                    .as_ref()
                    .map(|d| {
                        WindowThemeDetectionFile {
                            mode: d.mode.clone(),
                            luminance_threshold: d.luminance_threshold,
                        }
                        .into_tokens(&default.window_theme_detection)
                    })
                    .unwrap_or_else(|| default.window_theme_detection.clone());
                (w.into_tokens(&default.window), detection)
            }
            None => (default.window.clone(), default.window_theme_detection.clone()),
        };
        // H32.7: sub-tabela [menu] com sub-aninhadas profile_bar, search,
        // row, session, column_divider.
        let (
            menu_launcher,
            menu_profile_bar,
            menu_search,
            menu_row,
            menu_session,
            menu_column_divider,
        ) = match self.menu {
            Some(mut m) => {
                let profile_bar = m
                    .profile_bar
                    .take()
                    .map(|p| p.into_tokens(&default.menu_profile_bar))
                    .unwrap_or_else(|| default.menu_profile_bar.clone());
                let search = m
                    .search
                    .take()
                    .map(|s| s.into_tokens(&default.menu_search))
                    .unwrap_or_else(|| default.menu_search.clone());
                let row = m
                    .row
                    .take()
                    .map(|r| r.into_tokens(&default.menu_row))
                    .unwrap_or_else(|| default.menu_row.clone());
                let session = m
                    .session
                    .take()
                    .map(|s| s.into_tokens(&default.menu_session))
                    .unwrap_or_else(|| default.menu_session.clone());
                let column_divider = m
                    .column_divider
                    .take()
                    .map(|c| c.into_tokens(&default.menu_column_divider))
                    .unwrap_or_else(|| default.menu_column_divider.clone());
                let launcher = m.into_launcher(&default.menu_launcher);
                (launcher, profile_bar, search, row, session, column_divider)
            }
            None => (
                default.menu_launcher.clone(),
                default.menu_profile_bar.clone(),
                default.menu_search.clone(),
                default.menu_row.clone(),
                default.menu_session.clone(),
                default.menu_column_divider.clone(),
            ),
        };
        let taskbar = self
            .taskbar
            .map(|t| t.into_tokens(&default.taskbar))
            .unwrap_or(default.taskbar);
        let window_button = self
            .window_button
            .map(|t| t.into_tokens(&default.window_button))
            .unwrap_or(default.window_button);
        let window_controls = self
            .window_controls
            .map(|t| t.into_tokens(&default.window_controls))
            .unwrap_or(default.window_controls);
        let show_desktop = self
            .show_desktop
            .map(|t| t.into_tokens(&default.show_desktop))
            .unwrap_or(default.show_desktop);
        let clock = self
            .clock
            .map(|t| t.into_tokens(&default.clock))
            .unwrap_or(default.clock);

        // Campos legados: parser pré-H32.5 mantido por compat. Quando
        // ausente no toml, deriva dos novos tokens (que já têm Golden Hour).
        let border = self
            .border
            .as_deref()
            .and_then(parse_color)
            .unwrap_or(colors.applet_bg);
        let taskbar_bg = self
            .taskbar_bg
            .as_deref()
            .and_then(parse_color)
            .unwrap_or(colors.taskbar_bg_top);
        let taskbar_block_inactive = self
            .taskbar_block_inactive
            .as_deref()
            .and_then(parse_color)
            .unwrap_or(colors.applet_bg);

        Theme {
            clear: self.clear.as_deref().and_then(parse_rgba).unwrap_or(default.clear),
            border,
            taskbar_bg,
            taskbar_block_inactive,
            taskbar_block_active: self
                .taskbar_block_active
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(colors.applet_bg_active),
            taskbar_launcher: self
                .taskbar_launcher
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(colors.applet_bg),
            menu_bg: self
                .menu_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(colors.taskbar_bg_top),
            menu_bg_right: self
                .menu_bg_right
                .as_deref()
                .and_then(parse_color)
                .unwrap_or_else(|| darken(colors.taskbar_bg_top, 0.7)),
            menu_header_bar: self
                .menu_header_bar
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(colors.taskbar_bg_bottom),
            menu_item_hover: self
                .menu_item_hover
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(colors.applet_bg_hover),
            menu_border: self
                .menu_border
                .as_deref()
                .and_then(parse_color)
                .unwrap_or_else(|| with_alpha(colors.divider, colors.divider_alpha)),
            wallpaper_mode: self
                .wallpaper_mode
                .as_deref()
                .and_then(parse_wallpaper_mode)
                .unwrap_or(default.wallpaper_mode),
            colors,
            window,
            window_theme_detection,
            menu_launcher,
            menu_fullscreen: default.menu_fullscreen.clone(),
            menu_greeting: default.menu_greeting.clone(),
            menu_search_bar: default.menu_search_bar.clone(),
            menu_favorites: default.menu_favorites.clone(),
            menu_all_apps: default.menu_all_apps.clone(),
            menu_all_apps_link: default.menu_all_apps_link.clone(),
            menu_session_bar: default.menu_session_bar.clone(),
            accessibility: default.accessibility.clone(),
            menu_profile_bar,
            menu_search,
            menu_row,
            menu_session,
            menu_column_divider,
            search: default.search.clone(),
            workspaces: default.workspaces.clone(),
            taskbar,
            window_button,
            window_controls,
            show_desktop,
            clock,
        }
    }
}

impl ColorTokensFile {
    fn into_tokens(self, def: &ColorTokens) -> ColorTokens {
        // Cor + alpha por campo: se user passa "#RRGGBB" e separately
        // taskbar_bg_top_alpha = 0.5, combina. Default herda do golden_hour.
        let parse_with = |hex: Option<String>, alpha: Option<f32>, fallback: Color32F| {
            let base = hex
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(fallback);
            match alpha {
                Some(a) => with_alpha(base, a),
                None => base,
            }
        };
        ColorTokens {
            taskbar_bg_top: parse_with(
                self.taskbar_bg_top,
                self.taskbar_bg_top_alpha,
                def.taskbar_bg_top,
            ),
            taskbar_bg_bottom: parse_with(
                self.taskbar_bg_bottom,
                self.taskbar_bg_bottom_alpha,
                def.taskbar_bg_bottom,
            ),
            applet_bg: self
                .applet_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.applet_bg),
            applet_bg_hover: self
                .applet_bg_hover
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.applet_bg_hover),
            applet_bg_active: self
                .applet_bg_active
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.applet_bg_active),
            accent_amber: self
                .accent_amber
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.accent_amber),
            accent_amber_alpha_top: self
                .accent_amber_alpha_top
                .unwrap_or(def.accent_amber_alpha_top),
            accent_amber_glow: self.accent_amber_glow.unwrap_or(def.accent_amber_glow),
            hover_minimize: self
                .hover_minimize
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.hover_minimize),
            hover_maximize: self
                .hover_maximize
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.hover_maximize),
            hover_close: self
                .hover_close
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.hover_close),
            hover_glow_radius: self.hover_glow_radius.unwrap_or(def.hover_glow_radius),
            hover_glow_opacity: self.hover_glow_opacity.unwrap_or(def.hover_glow_opacity),
            text_primary: self
                .text_primary
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.text_primary),
            text_secondary: self
                .text_secondary
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.text_secondary),
            text_inactive_opacity: self
                .text_inactive_opacity
                .unwrap_or(def.text_inactive_opacity),
            text_minimized_opacity: self
                .text_minimized_opacity
                .unwrap_or(def.text_minimized_opacity),
            window_control_icon_light: self
                .window_control_icon_light
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_control_icon_light),
            window_control_light_alpha: self
                .window_control_light_alpha
                .unwrap_or(def.window_control_light_alpha),
            window_control_icon_dark: self
                .window_control_icon_dark
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_control_icon_dark),
            window_control_dark_alpha: self
                .window_control_dark_alpha
                .unwrap_or(def.window_control_dark_alpha),
            // H32.6
            window_header_bg_dark: self
                .window_header_bg_dark
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_header_bg_dark),
            window_header_bg_light: self
                .window_header_bg_light
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_header_bg_light),
            window_bg_dark: self
                .window_bg_dark
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_bg_dark),
            window_bg_light: self
                .window_bg_light
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_bg_light),
            window_border_focused: self
                .window_border_focused
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_border_focused),
            window_border_focused_alpha: self
                .window_border_focused_alpha
                .unwrap_or(def.window_border_focused_alpha),
            window_border_focused_glow: self
                .window_border_focused_glow
                .unwrap_or(def.window_border_focused_glow),
            window_border_unfocused: self
                .window_border_unfocused
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_border_unfocused),
            window_border_unfocused_alpha: self
                .window_border_unfocused_alpha
                .unwrap_or(def.window_border_unfocused_alpha),
            window_header_top_accent: self
                .window_header_top_accent
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_header_top_accent),
            window_header_top_alpha: self
                .window_header_top_alpha
                .unwrap_or(def.window_header_top_alpha),
            window_title_color_dark: self
                .window_title_color_dark
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_title_color_dark),
            window_title_color_light: self
                .window_title_color_light
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_title_color_light),
            window_title_unfocused_alpha: self
                .window_title_unfocused_alpha
                .unwrap_or(def.window_title_unfocused_alpha),
            window_shadow_color: self
                .window_shadow_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.window_shadow_color),
            window_shadow_alpha: self
                .window_shadow_alpha
                .unwrap_or(def.window_shadow_alpha),
            window_shadow_focused_multiplier: self
                .window_shadow_focused_multiplier
                .unwrap_or(def.window_shadow_focused_multiplier),
            // H32.7
            menu_bg_top: self
                .menu_bg_top
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.menu_bg_top),
            menu_bg_top_alpha: self.menu_bg_top_alpha.unwrap_or(def.menu_bg_top_alpha),
            menu_bg_bottom: self
                .menu_bg_bottom
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.menu_bg_bottom),
            menu_bg_bottom_alpha: self.menu_bg_bottom_alpha.unwrap_or(def.menu_bg_bottom_alpha),
            search_bg: self
                .search_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.search_bg),
            search_bg_focused: self
                .search_bg_focused
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.search_bg_focused),
            search_border_focused: self
                .search_border_focused
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.search_border_focused),
            search_border_focused_width: self
                .search_border_focused_width
                .unwrap_or(def.search_border_focused_width),
            search_placeholder: self
                .search_placeholder
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.search_placeholder),
            search_text: self
                .search_text
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.search_text),
            search_icon: self
                .search_icon
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.search_icon),
            search_icon_focused: self
                .search_icon_focused
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.search_icon_focused),
            row_bg_hover: self
                .row_bg_hover
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.row_bg_hover),
            row_bg_keyboard_focus: self
                .row_bg_keyboard_focus
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.row_bg_keyboard_focus),
            row_focus_indicator: self
                .row_focus_indicator
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.row_focus_indicator),
            row_text: self
                .row_text
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.row_text),
            row_text_focused_weight: self
                .row_text_focused_weight
                .unwrap_or(def.row_text_focused_weight),
            row_recently_used_dot: self
                .row_recently_used_dot
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.row_recently_used_dot),
            session_bg: self
                .session_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.session_bg),
            session_icon_default: self
                .session_icon_default
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.session_icon_default),
            session_icon_default_alpha: self
                .session_icon_default_alpha
                .unwrap_or(def.session_icon_default_alpha),
            session_lock_hover: self
                .session_lock_hover
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.session_lock_hover),
            session_sleep_hover: self
                .session_sleep_hover
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.session_sleep_hover),
            session_restart_hover: self
                .session_restart_hover
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.session_restart_hover),
            session_power_hover: self
                .session_power_hover
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.session_power_hover),
            greeting_color: self
                .greeting_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.greeting_color),
            column_header_color: self
                .column_header_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.column_header_color),
            column_header_size: self
                .column_header_size
                .unwrap_or(def.column_header_size),
            column_header_letter_spacing: self
                .column_header_letter_spacing
                .unwrap_or(def.column_header_letter_spacing),
            divider: self
                .divider
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.divider),
            divider_alpha: self.divider_alpha.unwrap_or(def.divider_alpha),
        }
    }
}

impl TaskbarTokensFile {
    fn into_tokens(self, def: &TaskbarTokens) -> TaskbarTokens {
        let launcher = self
            .launcher
            .map(|l| l.into_tokens(&def.launcher))
            .unwrap_or_else(|| def.launcher.clone());
        let apps = self
            .apps
            .map(|a| a.into_tokens(&def.apps))
            .unwrap_or_else(|| def.apps.clone());
        let tooltip = self
            .tooltip
            .map(|t| t.into_tokens(&def.tooltip))
            .unwrap_or_else(|| def.tooltip.clone());
        TaskbarTokens {
            height: self.height.unwrap_or(def.height),
            padding_left: self.padding_left.unwrap_or(def.padding_left),
            padding_right: self.padding_right.unwrap_or(def.padding_right),
            gap_between_applets: self
                .gap_between_applets
                .unwrap_or(def.gap_between_applets),
            applet_corner_radius: self
                .applet_corner_radius
                .unwrap_or(def.applet_corner_radius),
            border_top_height: self.border_top_height.unwrap_or(def.border_top_height),
            launcher,
            apps,
            tooltip,
        }
    }
}

impl TaskbarTooltipTokensFile {
    fn into_tokens(self, def: &TaskbarTooltipTokens) -> TaskbarTooltipTokens {
        TaskbarTooltipTokens {
            bg: self
                .bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.bg),
            border_color: self
                .border_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.border_color),
            border_alpha: self.border_alpha.unwrap_or(def.border_alpha),
            text_color: self
                .text_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.text_color),
            text_size: self.text_size.unwrap_or(def.text_size),
            padding: self.padding.unwrap_or(def.padding),
            corner_radius: self.corner_radius.unwrap_or(def.corner_radius),
            margin_above: self.margin_above.unwrap_or(def.margin_above),
        }
    }
}

impl TaskbarAppsTokensFile {
    fn into_tokens(self, def: &TaskbarAppsTokens) -> TaskbarAppsTokens {
        TaskbarAppsTokens {
            button_size: self.button_size.unwrap_or(def.button_size),
            button_corner_radius: self
                .button_corner_radius
                .unwrap_or(def.button_corner_radius),
            icon_size: self.icon_size.unwrap_or(def.icon_size),
            icon_corner_radius: self
                .icon_corner_radius
                .unwrap_or(def.icon_corner_radius),
            gap: self.gap.unwrap_or(def.gap),
            active_bg: self
                .active_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.active_bg),
            hover_bg: self
                .hover_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.hover_bg),
            indicator_color: self
                .indicator_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.indicator_color),
            indicator_width: self.indicator_width.unwrap_or(def.indicator_width),
            indicator_height: self.indicator_height.unwrap_or(def.indicator_height),
            tooltip_delay_ms: self.tooltip_delay_ms.unwrap_or(def.tooltip_delay_ms),
        }
    }
}

impl TaskbarLauncherTokensFile {
    fn into_tokens(self, def: &TaskbarLauncherTokens) -> TaskbarLauncherTokens {
        TaskbarLauncherTokens {
            size: self.size.unwrap_or(def.size),
            corner_radius: self.corner_radius.unwrap_or(def.corner_radius),
            bg: self
                .bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.bg),
            active_bg: self
                .active_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.active_bg),
            divider_after: self.divider_after.unwrap_or(def.divider_after),
            divider_alpha: self.divider_alpha.unwrap_or(def.divider_alpha),
            divider_gap_left: self.divider_gap_left.unwrap_or(def.divider_gap_left),
        }
    }
}

impl WindowButtonTokensFile {
    fn into_tokens(self, def: &WindowButtonTokens) -> WindowButtonTokens {
        WindowButtonTokens {
            max_width: self.max_width.unwrap_or(def.max_width),
            height: self.height.unwrap_or(def.height),
            active_indicator_height: self
                .active_indicator_height
                .unwrap_or(def.active_indicator_height),
            icon_size: self.icon_size.unwrap_or(def.icon_size),
            icon_text_gap: self.icon_text_gap.unwrap_or(def.icon_text_gap),
        }
    }
}

impl WindowControlsTokensFile {
    fn into_tokens(self, def: &WindowControlsTokens) -> WindowControlsTokens {
        WindowControlsTokens {
            hit_area: self.hit_area.unwrap_or(def.hit_area),
            icon_size: self.icon_size.unwrap_or(def.icon_size),
            gap: self.gap.unwrap_or(def.gap),
            header_padding_right: self
                .header_padding_right
                .unwrap_or(def.header_padding_right),
            icon_stroke_width: self.icon_stroke_width.unwrap_or(def.icon_stroke_width),
            hover_radius: self.hover_radius.unwrap_or(def.hover_radius),
        }
    }
}

impl ShowDesktopTokensFile {
    fn into_tokens(self, def: &ShowDesktopTokens) -> ShowDesktopTokens {
        ShowDesktopTokens {
            width: self.width.unwrap_or(def.width),
            border_left_alpha: self
                .border_left_alpha
                .unwrap_or(def.border_left_alpha),
            hover_bg_alpha: self.hover_bg_alpha.unwrap_or(def.hover_bg_alpha),
        }
    }
}

impl ClockTokensFile {
    fn into_tokens(self, def: &ClockTokens) -> ClockTokens {
        ClockTokens {
            time_size: self.time_size.unwrap_or(def.time_size),
            date_size: self.date_size.unwrap_or(def.date_size),
            padding_right: self.padding_right.unwrap_or(def.padding_right),
        }
    }
}

fn parse_wallpaper_mode(s: &str) -> Option<WallpaperMode> {
    match s.trim().to_ascii_lowercase().as_str() {
        "stretch" => Some(WallpaperMode::Stretch),
        "center" => Some(WallpaperMode::Center),
        "cover" => Some(WallpaperMode::Cover),
        _ => None,
    }
}

/// Multiplica R/G/B por `factor` (mantém alpha). Usado pra derivar tons.
fn darken(c: Color32F, factor: f32) -> Color32F {
    Color32F::new(c.r() * factor, c.g() * factor, c.b() * factor, c.a())
}

/// Cria nova Color32F com o alpha trocado (mantém R/G/B).
fn with_alpha(c: Color32F, alpha: f32) -> Color32F {
    Color32F::new(c.r(), c.g(), c.b(), alpha)
}

/// Helper: parseia hex `"#RRGGBB"` e aplica alpha. Pra defaults estáticos.
fn hex_color(hex: &str, alpha: f32) -> Color32F {
    let base = parse_color(hex).expect("hex literal default inválido");
    with_alpha(base, alpha)
}

/// Parse string "#RRGGBB" ou "#RRGGBBAA" pra Color32F. Inválido vira `None`.
fn parse_color(s: &str) -> Option<Color32F> {
    let [r, g, b, a] = parse_rgba(s)?;
    Some(Color32F::new(r, g, b, a))
}

fn parse_rgba(s: &str) -> Option<[f32; 4]> {
    let hex = s.trim().trim_start_matches('#');
    let bytes = match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b, 255u8)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };
    Some([
        bytes.0 as f32 / 255.0,
        bytes.1 as f32 / 255.0,
        bytes.2 as f32 / 255.0,
        bytes.3 as f32 / 255.0,
    ])
}
