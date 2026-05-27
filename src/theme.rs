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
///
/// Muitos campos são tokens reservados — declarados no contrato
/// público + lidos do TOML, mas o render ainda não os consome.
/// Mantidos pra evitar churn quando a feature correspondente liga.
#[allow(dead_code)]
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
    /// Layout/cores do popup de applet. H44.volume.4.
    pub popup: PopupTokens,
    /// Layout/cores do OSD centralizado (volume + brilho). H45.osd.
    pub osd: OsdTokens,
    /// Layout/cores do OSD de mídia (now playing). H46.osd-media.
    pub osd_media: OsdMediaTokens,
    /// Layout/cores do menu de contexto do desktop. H48.
    pub context_menu: ContextMenuTokens,
    /// Wallpaper padrão Golden Hour quando o user não tem
    /// `~/.config/luma/wallpaper.{png,jpg,jpeg}`. H49.
    pub wallpaper: WallpaperTokens,
    /// Grid de ícones de arquivo no desktop. H50.
    pub desktop_icons: DesktopIconsTokens,
    /// Toast de notificação D-Bus (canto superior direito). H54.
    pub notifications: NotificationsTokens,
    /// Popup de calendário ao clicar no relógio. H55.
    pub calendario: CalendarTokens,
    /// Popup do applet de bateria (% + estado + slider de modo
    /// de energia). H57.
    pub battery_popup: BatteryPopupTokens,
    /// Popup do applet de Wi-Fi (header com toggle + lista de
    /// redes + senha inline). H57.2.
    pub wifi_popup: WifiPopupTokens,
    /// Tela de bloqueio (lock screen). Fundo Golden Hour + relógio
    /// + (na fatia 4+) avatar/saudação/campo de senha. H58.
    pub lock: LockTokens,
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
    /// Sub-tokens do system tray (zona D). H33.taskbar.9-11.
    pub system_tray: TaskbarSystemTrayTokens,
    /// Setinha de overflow do tray + painel. H47a.tray.
    pub tray_overflow: TrayOverflowTokens,
    /// Grid de ícones SNI dentro do painel de overflow. H47b.tray.
    pub tray_sni: TraySniTokens,
    /// Sub-tokens do grupo direito (D + E + F). H33.taskbar.12.
    pub right_group: TaskbarRightGroupTokens,
}

/// Sub-tokens do system tray (zona D) — battery, wifi, volume, notifications.
/// H33.taskbar.9-11.
///
/// Ícones renderizados inline (paths SVG no Rust), stroke fino estilo
/// outline. Battery muda fill por %; notifications mostram dot coral
/// quando há não lidas. Em desktops sem bateria, battery não aparece.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TaskbarSystemTrayTokens {
    pub icon_size: i32,
    pub icon_stroke_width: f32,
    pub icon_color: Color32F,
    pub icon_alpha: f32,
    /// Gap horizontal entre os ícones do tray (battery/wifi/bt/volume/bell).
    pub gap: i32,
    /// Path da pasta dos SVGs do tray. Default `assets/branding/icons`.
    /// Arquivos esperados: `icon-<name>.svg` onde `<name>` é o estado
    /// (battery-high, wifi-full, bluetooth-on, volume-muted, bell-normal,
    /// ethernet-on, ethernet-off, ...). H33.taskbar.SVG.
    pub icons_path: String,
    /// Cabo ethernet plugado tem prioridade sobre wifi no slot de rede
    /// (mostra `ethernet-on` se há interface eth com carrier=1).
    /// H33.taskbar.10.eth.
    pub network_prefer_ethernet: bool,
    pub battery_pct_color: Color32F,
    pub battery_pct_size: f32,
    pub notif_dot_color: Color32F,
    pub notif_dot_size: i32,
}

/// Setinha de overflow + painel do system tray. H47a.
/// Setinha fica à esquerda dos 5 ícones nativos. Painel abre ao
/// clicar (acima da setinha). Enquanto não há SNI (H47b), a setinha
/// fica em `arrow_disabled_alpha` e o painel mostra placeholder.
#[derive(Debug, Clone)]
pub struct TrayOverflowTokens {
    pub arrow_size: i32,
    pub arrow_corner_radius: i32,
    pub arrow_bg: Color32F,
    pub arrow_bg_open: Color32F,
    pub arrow_color: Color32F,
    /// Alpha aplicada quando não há itens SNI (dim). H47a.4.
    pub arrow_disabled_alpha: f32,
    pub panel_bg: Color32F,
    pub panel_corner: i32,
    pub panel_top_accent: Color32F,
    /// Largura do painel quando aberto. Default 280 (cabe placeholder
    /// + futura grid 4-col de ícones SNI). H47a.2.
    pub panel_width: i32,
    /// Altura mínima do painel (placeholder + futura grid). H47a.2.
    pub panel_height: i32,
    /// Tamanho da fonte do placeholder "Sem itens".
    pub placeholder_text_size: f32,
    pub placeholder_text_color: Color32F,
}

/// Grid de itens SNI dentro do painel de overflow. H47b.
#[derive(Debug, Clone)]
pub struct TraySniTokens {
    pub icon_size: i32,
    pub button_size: i32,
    pub button_corner_radius: i32,
    pub button_bg: Color32F,
    pub hover_bg: Color32F,
    pub gap: i32,
    pub grid_columns: i32,
    /// Padding interno do painel pro grid.
    pub padding: i32,
}

/// Sub-tokens do grupo direito da taskbar. H33.taskbar.12.
///
/// As 3 zonas D/E/F (tray, clock, show-desktop) ficam ancoradas na
/// borda direita, separadas pelo `gap_between_zones`. `margin_right` é
/// a distância da borda direita do output até a zona F.
#[derive(Debug, Clone)]
pub struct TaskbarRightGroupTokens {
    pub gap_between_zones: i32,
    pub margin_right: i32,
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
    /// Blur da sombra quando a janela está fora de foco. H_deco.3.
    pub shadow_blur_inactive: i32,
    pub shadow_offset_y: i32,
    /// Opacity da sombra da janela focada. H_deco.2.
    pub shadow_opacity: f32,
    /// Opacity da sombra da janela fora de foco. H_deco.3.
    pub shadow_opacity_inactive: f32,
    /// Cor base da sombra (Black default). H_deco.2.
    pub shadow_color: Color32F,
    /// Cor do topo do gradient da titlebar. H_deco.4.
    pub titlebar_bg_top: Color32F,
    /// Cor da base do gradient da titlebar. H_deco.4.
    pub titlebar_bg_bottom: Color32F,
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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

/// Tokens dos workspaces (pager na taskbar + overview screen). H36 +
/// H33.taskbar.7 (re-skin: 32×32 quadrados numerados em vez de dots).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WorkspacesTokens {
    /// Esconde o pager quando há apenas 1 workspace (default `true`).
    /// H33.taskbar.8 — substitui `pager_show_if_count: usize` legado
    /// do H36 (semântica equivalente mas era contraintuitivo).
    pub pager_hide_if_single: bool,
    pub pager_padding_left: i32,
    pub pager_gap: i32,
    /// Tamanho de cada item do pager (ativo e inativos). 32 default em
    /// H33 — substitui o `pager_dot_radius` legado.
    pub pager_item_size: i32,
    /// Corner radius dos quadrados do pager. 9 em H33.
    pub pager_corner_radius: i32,
    pub pager_inactive_color: Color32F,
    pub pager_hover_color: Color32F,
    pub pager_active_color: Color32F,
    /// Tamanho do número renderizado dentro do quadrado. 13 default em H33.
    pub pager_text_size: f32,
    /// Cor do número no item ativo (texto sobre pill âmbar — coffee). H33.
    pub pager_text_color_active: Color32F,
    /// Cor do número nos itens inativos (cream). H33.
    pub pager_text_color_inactive: Color32F,
    /// Alpha aplicado ao número dos itens inativos (45% default). H33.
    pub pager_text_alpha_inactive: f32,
    pub overview_backdrop: Color32F,
    pub overview_thumb_width: i32,
    pub overview_thumb_height: i32,
    pub overview_thumb_gap: i32,
}

impl Default for WorkspacesTokens {
    fn default() -> Self {
        Self {
            pager_hide_if_single: true,
            pager_padding_left: 14,
            pager_gap: 8,
            pager_item_size: 32,
            pager_corner_radius: 9,
            pager_inactive_color: hex_color("#FBF6EE", 0.25),
            pager_hover_color: hex_color("#FBF6EE", 0.55),
            pager_active_color: hex_color("#E89B3D", 1.0),
            pager_text_size: 13.0,
            pager_text_color_active: hex_color("#1F1611", 1.0),
            pager_text_color_inactive: hex_color("#FBF6EE", 1.0),
            pager_text_alpha_inactive: 0.45,
            overview_backdrop: hex_color("#1F1611", 0.92),
            overview_thumb_width: 190,
            overview_thumb_height: 120,
            overview_thumb_gap: 20,
        }
    }
}

/// Tokens da search Spotlight-like. H32.9.
#[allow(dead_code)]
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
    /// Cor dos ícones quando a janela está fora de foco. H_deco.8.
    /// Default `#8B7D72` (cinza Golden Hour neutro).
    pub inactive_color: Color32F,
}

#[derive(Debug, Clone)]
pub struct ShowDesktopTokens {
    /// Largura legacy da faixinha (10px Win7-style). H33.taskbar.F.16:
    /// substituído pelo `size` 44, mas o campo continua usado por
    /// `show_desktop_width()` por compat (= alias pra size).
    pub width: i32,
    pub border_left_alpha: f32,
    pub hover_bg_alpha: f32,
    /// Tamanho do botão (44 default). H33.taskbar.F.16.
    pub size: i32,
    pub corner_radius: i32,
    /// Alpha da borda 1px à esquerda do botão. H33.taskbar.F.16.
    pub border_alpha: f32,
    /// Alpha default do ícone (0.5 = sutil). H33.taskbar.F.16.
    pub icon_alpha_default: f32,
    pub hover_bg: Color32F,
    pub active_bg: Color32F,
    /// Tamanho (px) do ícone dentro do botão 44×44 (centralizado).
    pub icon_size: i32,
    /// Nome do SVG mostrado por default (janelas visíveis).
    /// Default `show-desktop`. H33.taskbar.F.16.
    pub icon_normal_name: String,
    /// Nome do SVG mostrado quando desktop está visível (todas
    /// janelas minimizadas). Default `show-desktop-active`. H33.taskbar.F.16.
    pub icon_active_name: String,
}

/// Tokens do popup de applet (volume, futuros). H44.volume.4.
#[derive(Debug, Clone)]
pub struct PopupTokens {
    pub corner_radius: i32,
    pub padding: i32,
    pub tab_height: i32,
    pub tab_text_size: f32,
    pub tab_indicator_width: i32,
    pub tab_indicator_height: i32,
    pub tab_indicator_color: Color32F,
    pub slider: PopupSliderTokens,
}

#[derive(Debug, Clone)]
pub struct PopupSliderTokens {
    pub track_height: i32,
    pub track_color: Color32F,
    pub fill_color: Color32F,
    pub handle_radius: i32,
    pub handle_color: Color32F,
}

/// Tokens do OSD centralizado de volume/brilho. H45.osd.
///
/// Componente compartilhado: muda só o ícone + a fonte de dados.
/// Pill escuro 260×64 centralizado em ambos eixos, auto-dismiss 1.5s.
#[derive(Debug, Clone)]
pub struct OsdTokens {
    pub width: i32,
    pub height: i32,
    pub corner_radius: i32,
    pub bg: Color32F,
    pub icon_size: i32,
    pub icon_padding_left: i32,
    pub bar_height: i32,
    pub bar_padding_right: i32,
    pub bar_bg: Color32F,
    /// Cor da barra em modo normal (0..100%).
    pub bar_fill: Color32F,
    /// Cor da barra quando volume > 100% (boost). Coral.
    pub bar_fill_boost: Color32F,
    /// Cor da barra quando muted. Cinza desbotado.
    pub bar_fill_muted: Color32F,
    pub text_color: Color32F,
    pub text_size: f32,
    /// Tempo de auto-dismiss em milissegundos. Default 1500.
    pub dismiss_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ClockTokens {
    /// Tamanho da hora (HH:MM). Default 17 (Fraunces no spec H33; hoje
    /// renderizado com a sans-serif do sistema — TextRenderer ainda é
    /// 1 fonte só, débito documentado).
    pub time_size: f32,
    /// Tamanho da data. Default 10 (Manrope no spec H33).
    pub date_size: f32,
    /// Padding direito legacy. Posicionamento real do clock vem do
    /// `right_group`. H33.taskbar.E.14.
    pub padding_right: i32,
    /// Cor da hora (Cream).
    pub time_color: Color32F,
    /// Cor da data (Cream desbotado).
    pub date_color: Color32F,
    /// Strftime da hora. Default `%H:%M` (24h).
    pub time_format: String,
    /// Strftime da data. Default `%a · %d %b` (pt-BR map em winit.rs).
    pub date_format: String,
    /// Family da fonte da hora. Default "Fraunces" (debt: 1 fonte só).
    #[allow(dead_code)]
    pub font_family: String,
    /// Peso da fonte da hora (não aplicado: fontdue sem variable fonts).
    #[allow(dead_code)]
    pub time_weight: u16,
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
            popup: PopupTokens::default(),
            osd: OsdTokens::default(),
            osd_media: OsdMediaTokens::default(),
            context_menu: ContextMenuTokens::default(),
            wallpaper: WallpaperTokens::default(),
            desktop_icons: DesktopIconsTokens::default(),
            notifications: NotificationsTokens::default(),
            calendario: CalendarTokens::default(),
            battery_popup: BatteryPopupTokens::default(),
            wifi_popup: WifiPopupTokens::default(),
            lock: LockTokens::default(),
        }
    }
}

impl Default for NotificationsTokens {
    fn default() -> Self {
        Self {
            width: 380,
            corner: 22,
            // H54.P3.fix2: dois layers — bg translúcido (estética) +
            // scrim quase opaco (contraste). Combinado: ~94% opacidade
            // efetiva sobre wallpaper qualquer, garantindo AA 4.5:1 do
            // body text mesmo em fundo claro.
            // bg alpha 0.55 = 45% wallpaper leak (forma o "blur" feel).
            bg: hex_color("#2D2219", 0.55),
            // Scrim alpha 0.88 = combinado 1-(1-0.55)*(1-0.88) = 0.946
            // → 5.4% wallpaper leak total. Contraste body cream sobre
            // bg coffee ~6:1, passa AA com folga (worst case wallpaper
            // 100% branco). H54.P3.fix2.
            scrim: hex_color("#1F1611", 0.88),
            // PCD Max preset: combinado 1-(1-0.55)*(1-0.96) = 0.982
            // → 1.8% leak. Contraste body sobre quase-opaco ~10:1, AAA.
            scrim_pcd: hex_color("#1F1611", 0.96),
            // Sombra: espresso #15090A (coffee bem escuro com hint
            // brown-vinho, vem da família Golden Hour mas mais escuro
            // que o bg pra dar profundidade real).
            // Contact: opacidade 0.32 — define a borda.
            // Ambient: opacidade 0.14 — difusão larga.
            shadow_color_contact: hex_color("#15090A", 0.32),
            shadow_color_ambient: hex_color("#15090A", 0.14),
            shadow_contact_blur: 8,
            shadow_contact_offset_y: 3,
            shadow_ambient_blur: 28,
            shadow_ambient_offset_y: 10,
            // Action buttons (H54.P4): ghost sobre o scrim coffee.
            // Fill sutil pra textura sem ofuscar o body.
            action_btn_bg: hex_color("#3D2520", 0.55),
            action_btn_bg_hover: hex_color("#5A3A30", 0.75),
            action_btn_label_color: hex_color("#FBF6EE", 1.0),
            action_btn_corner: 10,
            action_btn_padding_h: 18,
            action_btn_gap: 10,
            action_btn_label_size: 13.0,
            // H54.P4.fix: bump 8 → 16 pra respirar entre "agora" e a
            // linha de botões.
            action_btn_top_margin: 16,
            // Alvo de clique confortável (PCD-first). Hit target = altura
            // visível, sem padding invisível.
            action_btn_height: 40,
            action_btn_height_pcd: 48,
            title_color: hex_color("#FBF6EE", 1.0),
            // H54.P3.fix: body bumpado pra contraste AAA (~12:1) —
            // cream-cinza claro. Antes #C9BDB0 era ~8:1 (passa AAA mas
            // visualmente fraco em texto pequeno).
            body_color: hex_color("#E5DACF", 1.0),
            // Time/horário continua text_secondary discreto (~5:1, AA).
            time_color: hex_color("#9A8B7E", 1.0),
            icon_size: 62,
            icon_corner: 14,
            title_size: 14.0,
            body_size: 12.0,
            time_size: 11.0,
            timeout_ms: 5000,
            margin: 20,
            fade_ms: 250,
            max_body_lines: 2,
            padding: 14,
            // H54.P5: central de notificações.
            panel_width: 400,
            panel_padding: 16,
            // H54.P5.fix: mesmos tokens de cor do scrim dos toasts, com
            // alpha calibrada pra AA 4.5:1 default e folga pra AAA com
            // wallpaper claro. Combinado: 1-(1-0.55)*(1-0.96) = 0.982
            // → 1.8% leak de wallpaper, contraste cream-sobre-coffee
            // ~10:1 mesmo no pior caso.
            panel_bg: hex_color("#2D2219", 0.55),
            panel_scrim: hex_color("#1F1611", 0.96),
            // Tokens panel_shadow_* mantidos por compat — render usa
            // os tokens shadow_color_contact/ambient + shadow_*_blur
            // dos toasts diretamente (mesma linguagem por spec).
            panel_shadow_blur: 28,
            panel_shadow_color: hex_color("#15090A", 0.32),
            panel_header_height: 56,
            panel_header_title_color: hex_color("#FBF6EE", 1.0),
            panel_header_title_size: 16.0,
            panel_btn_bg: hex_color("#3D2520", 0.55),
            panel_btn_bg_hover: hex_color("#5A3A30", 0.75),
            panel_btn_label_color: hex_color("#FBF6EE", 1.0),
            panel_btn_corner: 8,
            panel_btn_padding_h: 12,
            panel_btn_height: 32,
            panel_btn_label_size: 12.0,
            // DND on: amber Golden Hour discreto pra sinalizar ativo
            // sem virar botão "perigo".
            panel_dnd_active_bg: hex_color("#E89B3D", 0.30),
            panel_item_height: 82,
            panel_item_corner: 10,
            panel_item_padding: 12,
            panel_item_gap: 8,
            panel_item_icon_size: 36,
            panel_item_title_color: hex_color("#FBF6EE", 1.0),
            panel_item_title_size: 13.0,
            panel_item_body_color: hex_color("#E5DACF", 1.0),
            panel_item_body_size: 12.0,
            panel_item_time_color: hex_color("#9A8B7E", 1.0),
            panel_item_time_size: 11.0,
            panel_item_hover_bg: hex_color("#3D2520", 0.45),
            panel_item_dismiss_size: 22,
            panel_item_dismiss_color: hex_color("#C9BDB0", 1.0),
            panel_item_max_body_lines: 2,
            panel_empty_label_color: hex_color("#9A8B7E", 1.0),
            panel_empty_label_size: 14.0,
            // Badge: coral Golden Hour pra contraste com bell (cream).
            bell_badge_bg: hex_color("#D97757", 1.0),
            bell_badge_text_color: hex_color("#FBF6EE", 1.0),
            bell_badge_size: 16,
            bell_badge_font_size: 10.0,
            // H54.P6: paths dos assets de áudio. Resolvidos via
            // crate::theme::asset_path() (cwd ou exe-dir). Se o arquivo
            // não existir em runtime, falha em silêncio.
            sound_default_path: "assets/sounds/notification-default.ogg".into(),
            sound_critical_path: "assets/sounds/notification-critical.ogg".into(),
            sound_debounce_ms: 200,
            sound_enabled_default: true,
            // H54.P7: cores quentes do Golden Hour.
            // Honey amber (#E89B3D) pra normal — mesmo tom do active_bg
            // do DND, coerente com a paleta.
            flash_normal_color: hex_color("#E89B3D", 1.0),
            // Coral (#D97757) pra critical — mesma família do bell badge.
            flash_critical_color: hex_color("#D97757", 1.0),
            // Tetos de opacidade conservadores. NUNCA aumentar sem
            // reavaliar fotossensibilidade. WCAG seizure safety: <3
            // flashes/s e contraste limitado. Aqui é 1 flash, não strobe.
            flash_normal_peak_alpha: 0.18,
            flash_critical_peak_alpha: 0.28,
            // Total ~300ms: fade-in 80 + hold 80 + fade-out 140.
            // Curva ease_out na entrada e na saída — sem corte abrupto.
            flash_fade_in_ms: 80,
            flash_hold_ms: 80,
            flash_fade_out_ms: 140,
            // Debounce 600ms — rajada de 6 toasts vira 1 piscada.
            // Anti-strobe: garante < 1 flash/s no pior caso.
            flash_debounce_ms: 600,
            // Default OFF. Só preset PCD Max liga (theme.toml override).
            flash_enabled_default: false,
        }
    }
}

impl Default for DesktopIconsTokens {
    fn default() -> Self {
        Self {
            icon_size: 64,
            cell_width: 110,
            cell_height: 120,
            grid_margin: 40,
            label_color: hex_color("#FBF6EE", 1.0),
            label_size: 12.0,
            label_max_chars: 14,
            selection_bg: hex_color("#3D2520", 0.65), // applet_bg_active alpha
            selection_corner_radius: 10,
            rubber_band_bg: hex_color("#3D2520", 0.35),  // mais transparente
            rubber_band_border: hex_color("#E89B3D", 0.85), // amber
            rename_field_bg: hex_color("#2A1F1A", 1.0),         // applet_bg
            rename_field_border: hex_color("#E89B3D", 1.0),     // amber
            rename_field_text: hex_color("#FBF6EE", 1.0),       // cream
            rename_field_selection_bg: hex_color("#D97757", 1.0), // coral
            rename_field_cursor: hex_color("#FBF6EE", 1.0),     // cream
            rename_field_border_width: 2,
            rename_field_corner_radius: 6,
            rename_field_cursor_blink_ms: 500,
            // H56: 260ms ease_out — suave mas decidido. Override via
            // theme.toml; preset PCD Max pode reduzir / zerar.
            arrange_anim_duration_ms: 260,
            // H56 parte 3: 0.55 alpha — texto cream sobre coffee fica
            // contraste efetivo ~5.5:1 (AA com margem). Não derrubar
            // pra <0.50 sem reavaliar contraste do label.
            cut_opacity: 0.55,
        }
    }
}

/// Popup de bateria. H57. Hero block + slider de 3 paradas pra
/// modo de energia. Tudo aqui — zero hex hardcoded.
#[derive(Debug, Clone)]
pub struct BatteryPopupTokens {
    pub padding: i32,
    pub header_height: i32,
    pub header_color: Color32F,
    pub header_size: f32,
    /// Hero: % grande, estado, tempo, barra.
    pub hero_pct_size: f32,
    pub hero_pct_color: Color32F,
    /// Cor do % quando bateria está baixa (< low_threshold).
    pub hero_pct_low_color: Color32F,
    pub hero_state_size: f32,
    pub hero_state_color: Color32F,
    pub level_bar_height: i32,
    pub level_bar_corner: i32,
    pub level_bar_bg_color: Color32F,
    pub level_bar_fill_color: Color32F,
    pub level_bar_fill_low_color: Color32F,
    /// % abaixo da qual fg vira coral.
    pub low_threshold_pct: u8,
    /// Slider de modo de energia.
    pub mode_label_color: Color32F,
    pub mode_label_size: f32,
    pub mode_slider_track_height: i32,
    pub mode_slider_track_color: Color32F,
    pub mode_slider_fill_color: Color32F,
    pub mode_slider_thumb_radius: i32,
    pub mode_slider_thumb_color: Color32F,
    pub mode_slider_thumb_ring_color: Color32F,
    pub mode_stop_label_size: f32,
    pub mode_stop_label_color: Color32F,
    pub mode_stop_label_active_color: Color32F,
    pub divider_color: Color32F,
}

/// Popup de Wi-Fi. H57.2. Header com toggle, lista de redes, e
/// expansão inline com campo de senha + botão "Conectar".
#[derive(Debug, Clone)]
pub struct WifiPopupTokens {
    pub padding: i32,
    pub header_height: i32,
    pub header_color: Color32F,
    pub header_size: f32,
    /// Toggle (chave mestra) à direita do header.
    pub toggle_w: i32,
    pub toggle_h: i32,
    pub toggle_off_bg: Color32F,
    pub toggle_on_bg: Color32F,
    pub toggle_knob_color: Color32F,
    pub toggle_corner: i32,
    /// Lista de redes.
    pub row_height: i32,
    pub row_padding_h: i32,
    pub row_hover_bg: Color32F,
    pub ssid_color: Color32F,
    pub ssid_size: f32,
    pub ssid_connected_color: Color32F,
    pub state_label_color: Color32F,
    pub state_label_size: f32,
    pub signal_color: Color32F,
    pub signal_connected_color: Color32F,
    /// Estado vazio (wifi desligado).
    pub empty_label_color: Color32F,
    pub empty_label_size: f32,
    /// Password inline.
    pub pw_field_bg: Color32F,
    pub pw_field_text_color: Color32F,
    pub pw_field_size: f32,
    pub pw_field_corner: i32,
    pub pw_field_height: i32,
    pub pw_btn_bg: Color32F,
    pub pw_btn_bg_hover: Color32F,
    pub pw_btn_fg: Color32F,
    pub pw_btn_corner: i32,
    pub pw_btn_height: i32,
    pub pw_btn_label_size: f32,
    pub pw_error_color: Color32F,
    pub divider_color: Color32F,
}

/// Tela de bloqueio. H58.
#[derive(Debug, Clone)]
pub struct LockTokens {
    /// Cor de fundo da capa (cobertura full screen). Coffee profundo
    /// Golden Hour. NUNCA mostra o wallpaper do desktop.
    pub bg_color: Color32F,
    /// Relógio HH:MM grande centralizado.
    pub hour_size: f32,
    pub hour_color: Color32F,
    /// Data por extenso pequena abaixo da hora.
    pub date_size: f32,
    pub date_color: Color32F,
    /// Dica "tecle pra desbloquear" — aparece depois de
    /// `hint_delay_ms` parado.
    pub hint_text: String,
    pub hint_size: f32,
    pub hint_color: Color32F,
    pub hint_delay_ms: u64,
}

impl Default for LockTokens {
    fn default() -> Self {
        Self {
            bg_color: hex_color("#1F1611", 1.0),
            hour_size: 96.0,
            hour_color: hex_color("#FBF6EE", 1.0),
            date_size: 18.0,
            date_color: hex_color("#C9BDB0", 1.0),
            hint_text: "tecle pra desbloquear".to_string(),
            hint_size: 13.0,
            hint_color: hex_color("#9A8B7E", 1.0),
            hint_delay_ms: 3000,
        }
    }
}

impl Default for WifiPopupTokens {
    fn default() -> Self {
        Self {
            padding: 16,
            header_height: 48,
            header_color: hex_color("#FBF6EE", 1.0),
            header_size: 14.0,
            toggle_w: 44,
            toggle_h: 24,
            toggle_off_bg: hex_color("#2A1F1A", 1.0),
            toggle_on_bg: hex_color("#E89B3D", 1.0),
            toggle_knob_color: hex_color("#FBF6EE", 1.0),
            toggle_corner: 12,
            row_height: 48,
            row_padding_h: 14,
            row_hover_bg: hex_color("#3D2520", 0.45),
            ssid_color: hex_color("#FBF6EE", 1.0),
            ssid_size: 13.0,
            // Sage pra rede conectada.
            ssid_connected_color: hex_color("#A7C796", 1.0),
            state_label_color: hex_color("#A7C796", 1.0),
            state_label_size: 11.0,
            signal_color: hex_color("#C9BDB0", 1.0),
            signal_connected_color: hex_color("#A7C796", 1.0),
            empty_label_color: hex_color("#9A8B7E", 1.0),
            empty_label_size: 14.0,
            pw_field_bg: hex_color("#2A1F1A", 1.0),
            pw_field_text_color: hex_color("#FBF6EE", 1.0),
            pw_field_size: 12.0,
            pw_field_corner: 8,
            pw_field_height: 36,
            pw_btn_bg: hex_color("#E89B3D", 1.0),
            pw_btn_bg_hover: hex_color("#F0AD55", 1.0),
            pw_btn_fg: hex_color("#1F1611", 1.0),
            pw_btn_corner: 8,
            pw_btn_height: 36,
            pw_btn_label_size: 12.0,
            pw_error_color: hex_color("#D97757", 1.0),
            divider_color: hex_color("#3D2520", 0.55),
        }
    }
}

impl Default for BatteryPopupTokens {
    fn default() -> Self {
        Self {
            padding: 20,
            header_height: 40,
            header_color: hex_color("#FBF6EE", 1.0),
            header_size: 14.0,
            hero_pct_size: 48.0,
            hero_pct_color: hex_color("#FBF6EE", 1.0),
            // Coral pra baixa (mesma família do bell badge).
            hero_pct_low_color: hex_color("#D97757", 1.0),
            hero_state_size: 12.0,
            hero_state_color: hex_color("#C9BDB0", 1.0),
            level_bar_height: 6,
            level_bar_corner: 3,
            level_bar_bg_color: hex_color("#2A1F1A", 1.0),
            // Honey amber pra preenchimento normal.
            level_bar_fill_color: hex_color("#E89B3D", 1.0),
            level_bar_fill_low_color: hex_color("#D97757", 1.0),
            low_threshold_pct: 20,
            mode_label_color: hex_color("#9A8B7E", 1.0),
            mode_label_size: 10.0,
            mode_slider_track_height: 6,
            mode_slider_track_color: hex_color("#2A1F1A", 1.0),
            mode_slider_fill_color: hex_color("#E89B3D", 1.0),
            mode_slider_thumb_radius: 12,
            mode_slider_thumb_color: hex_color("#FBF6EE", 1.0),
            mode_slider_thumb_ring_color: hex_color("#E89B3D", 1.0),
            mode_stop_label_size: 11.0,
            mode_stop_label_color: hex_color("#9A8B7E", 1.0),
            mode_stop_label_active_color: hex_color("#E89B3D", 1.0),
            divider_color: hex_color("#3D2520", 0.55),
        }
    }
}

impl Default for CalendarTokens {
    fn default() -> Self {
        Self {
            // Alvo de 40px = PCD hit target mínimo do Luma. Preset
            // PCD Max sobe via theme.toml.
            cell_size: 40,
            padding_lateral: 12,
            padding_vertical: 12,
            header_height: 48,
            diasemana_height: 28,
            footer_height: 48,
            // Cream pra título — alto contraste sobre coffee.
            titulo_fg: hex_color("#FBF6EE", 1.0),
            titulo_size: 16.0,
            // Setas: cream apagado (~70%) que ganha alpha cheio no hover.
            seta_fg: hex_color("#C9BDB0", 1.0),
            seta_fg_hover: hex_color("#FBF6EE", 1.0),
            // Dias da semana: cinza-quente, contraste ~5:1 (AA).
            diasemana_fg: hex_color("#9A8B7E", 1.0),
            diasemana_size: 11.0,
            // Número do dia: cream cheio.
            dia_fg: hex_color("#FBF6EE", 1.0),
            dia_size: 14.0,
            // Dia vizinho: apagado por matiz (cinza médio), NÃO por
            // alpha. Contraste contra bg coffee ainda ~4.5:1 (AA).
            dia_vizinho_fg: hex_color("#7A6F66", 1.0),
            // Hoje: bg coral Golden Hour + número cream pra contraste
            // 5.4:1 (AA). Raio do quadrado inscrito.
            hoje_bg: hex_color("#D97757", 1.0),
            hoje_fg: hex_color("#FBF6EE", 1.0),
            hoje_raio: 6,
            // Rodapé (Fatia 4).
            rodape_fg: hex_color("#C9BDB0", 1.0),
            rodape_size: 12.0,
            hoje_botao_bg: hex_color("#3D2520", 0.55),
            hoje_botao_fg: hex_color("#FBF6EE", 1.0),
            hoje_botao_bg_hover: hex_color("#5A3A30", 0.75),
        }
    }
}

impl Default for WallpaperTokens {
    fn default() -> Self {
        Self {
            default_path: "assets/wallpapers/luma-gradient-dusk-1920x1080.png".to_string(),
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
            // Cantos da janela RETOS (0) até a reescrita do
            // renderer/shader pra suportar alpha mask de verdade.
            // Tentativas de simular round com cor sólida deixavam
            // remendo em apps dark. Débito em docs/DEBT.md.
            corner_radius: 0,
            header_height: 40,
            title_size: 13.0,
            title_weight: 500,
            title_font_family: String::from("Manrope"),
            title_padding_left: 16,
            controls_padding_right: 8,
            controls_gap: 6,
            shadow_blur: 16,
            shadow_blur_inactive: 8,
            shadow_offset_y: 8,
            shadow_opacity: 0.35,
            shadow_opacity_inactive: 0.20,
            shadow_color: hex_color("#000000", 1.0),
            // H_deco.4: gradiente da titlebar.
            titlebar_bg_top: hex_color("#2A1F1A", 1.0),
            titlebar_bg_bottom: hex_color("#1F1611", 1.0),
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
            system_tray: TaskbarSystemTrayTokens::default(),
            tray_overflow: TrayOverflowTokens::default(),
            tray_sni: TraySniTokens::default(),
            right_group: TaskbarRightGroupTokens::default(),
        }
    }
}

impl Default for TrayOverflowTokens {
    fn default() -> Self {
        Self {
            arrow_size: 36,
            arrow_corner_radius: 9,
            arrow_bg: hex_color("#3D2520", 1.0),       // applet_bg_active
            arrow_bg_open: hex_color("#E89B3D", 1.0),  // accent_amber
            arrow_color: hex_color("#FBF6EE", 1.0),    // cream
            arrow_disabled_alpha: 0.35,
            panel_bg: hex_color("#1F1611", 0.96),
            panel_corner: 14,
            panel_top_accent: hex_color("#E89B3D", 1.0),
            panel_width: 280,
            panel_height: 120,
            placeholder_text_size: 12.0,
            placeholder_text_color: hex_color("#A89D92", 1.0),  // text_secondary
        }
    }
}

impl Default for TraySniTokens {
    fn default() -> Self {
        Self {
            icon_size: 22,
            button_size: 40,
            button_corner_radius: 9,
            button_bg: hex_color("#2A1F1A", 1.0),
            hover_bg: hex_color("#3D2520", 1.0),
            gap: 12,
            grid_columns: 4,
            padding: 16,
        }
    }
}

impl Default for TaskbarSystemTrayTokens {
    fn default() -> Self {
        Self {
            icon_size: 20,
            icon_stroke_width: 1.5,
            icon_color: hex_color("#FBF6EE", 1.0),
            icon_alpha: 0.85,
            gap: 16,
            icons_path: "assets/branding/icons".to_string(),
            network_prefer_ethernet: true,
            battery_pct_color: hex_color("#FBF6EE", 1.0),
            battery_pct_size: 12.0,
            notif_dot_color: hex_color("#D97757", 1.0),
            notif_dot_size: 4,
        }
    }
}

impl Default for TaskbarRightGroupTokens {
    fn default() -> Self {
        Self {
            gap_between_zones: 16,
            margin_right: 12,
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
            // H_deco.4: margin pros botoes 8 (era 12) -- combina com
            // a spec v3 que usa margin_right 8.
            header_padding_right: 8,
            icon_stroke_width: 1.5,
            hover_radius: 6,
            // H_deco.8: cinza neutro pros botoes da janela fora de foco.
            inactive_color: hex_color("#8B7D72", 1.0),
        }
    }
}

impl Default for ShowDesktopTokens {
    fn default() -> Self {
        Self {
            // H33.taskbar.F.16: width legacy = size pra compat.
            width: 44,
            border_left_alpha: 0.15,
            hover_bg_alpha: 0.3,
            size: 44,
            corner_radius: 9,
            border_alpha: 0.15,
            icon_alpha_default: 0.5,
            hover_bg: hex_color("#3D2520", 1.0),
            active_bg: hex_color("#E89B3D", 1.0),
            icon_size: 24,
            icon_normal_name: "show-desktop".to_string(),
            icon_active_name: "show-desktop-active".to_string(),
        }
    }
}

impl Default for PopupTokens {
    fn default() -> Self {
        Self {
            corner_radius: 12,
            padding: 16,
            tab_height: 40,
            tab_text_size: 13.0,
            tab_indicator_width: 32,
            tab_indicator_height: 3,
            tab_indicator_color: hex_color("#E89B3D", 1.0),
            slider: PopupSliderTokens::default(),
        }
    }
}

impl Default for PopupSliderTokens {
    fn default() -> Self {
        Self {
            track_height: 6,
            track_color: hex_color("#2A1F1A", 1.0),  // applet_bg_hover
            fill_color: hex_color("#E89B3D", 1.0),   // accent_amber
            handle_radius: 10,
            handle_color: hex_color("#F5D5A8", 1.0), // honey
        }
    }
}

/// Tokens do OSD de mídia (now playing card). H46.osd-media.
///
/// Card 360×108 com arte à esquerda, título/artista + 3 controles
/// + barra de progresso à direita. Fade-out suave nos últimos
/// `fade_ms` antes do dismiss.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OsdMediaTokens {
    pub width: i32,
    pub height: i32,
    pub corner_radius: i32,
    pub bg: Color32F,
    /// Lado do quadrado de arte de álbum.
    pub art_size: i32,
    /// Padding interno em volta de tudo.
    pub padding: i32,
    pub title_color: Color32F,
    pub title_size: f32,
    pub artist_color: Color32F,
    pub artist_size: f32,
    /// Cor de fundo do botão play/pause (highlight do controle central).
    pub play_btn_bg: Color32F,
    /// Diâmetro do círculo do botão play/pause.
    pub play_btn_size: i32,
    /// Diâmetro dos botões prev/next (sem fundo, só icone).
    pub side_btn_size: i32,
    /// Cor dos ícones dos controles (todos: prev, play, pause, next).
    pub control_color: Color32F,
    pub progress_bg: Color32F,
    pub progress_fill: Color32F,
    pub progress_height: i32,
    /// Duração total de visibilidade em ms (timeout).
    pub timeout_ms: u64,
    /// Janela de fade-out dentro do timeout (últimos `fade_ms` em ms).
    pub fade_ms: u64,
}

impl Default for OsdTokens {
    fn default() -> Self {
        Self {
            width: 260,
            height: 64,
            corner_radius: 32,                          // pill
            bg: hex_color("#1F1611", 0.92),             // coffee + alpha
            icon_size: 32,
            icon_padding_left: 16,
            bar_height: 6,
            bar_padding_right: 16,
            bar_bg: hex_color("#2A1F1A", 1.0),          // applet_bg
            bar_fill: hex_color("#E89B3D", 1.0),        // accent_amber
            bar_fill_boost: hex_color("#D97757", 1.0),  // coral (hover_close)
            bar_fill_muted: hex_color("#A89D92", 1.0),  // text_secondary
            text_color: hex_color("#FBF6EE", 1.0),      // cream
            text_size: 13.0,
            dismiss_ms: 1500,
        }
    }
}

/// Toast de notificação D-Bus (canto superior direito). H54.
#[derive(Debug, Clone)]
pub struct NotificationsTokens {
    pub width: i32,
    pub corner: i32,
    /// Camada inferior "blur" — translúcida, mantém estética.
    /// Wallpaper vaza por aqui. H54.P3.fix2.
    pub bg: Color32F,
    /// Scrim: camada escura semi-opaca por cima do bg, garante piso
    /// de contraste fixo independente do wallpaper. Cor carvão Golden
    /// Hour. Alpha calibrada pra AA 4.5:1 mesmo sobre wallpaper claro.
    /// H54.P3.fix2.
    pub scrim: Color32F,
    /// Mesma família do scrim mas mais opaco — preset PCD Max
    /// (AAA 7:1) ativável via config futuro. Hoje não usado, fica
    /// pronto pro toggle de acessibilidade.
    #[allow(dead_code)]
    pub scrim_pcd: Color32F,
    /// Sombra macOS-style dois layers (contact + ambient) — dá
    /// profundidade, separa o toast de qualquer wallpaper. Cor não é
    /// preto puro; é espresso quente da paleta Golden Hour (luz vem
    /// de cima → sombra cai). H54.P3.shadow.
    pub shadow_color_contact: Color32F,
    pub shadow_color_ambient: Color32F,
    /// Raio de blur da contact shadow (px). Pequeno — cola o toast
    /// na superfície.
    pub shadow_contact_blur: i32,
    /// Offset vertical da contact shadow (px). Curto.
    pub shadow_contact_offset_y: i32,
    /// Raio de blur da ambient shadow (px). Grande — difusão larga.
    pub shadow_ambient_blur: i32,
    /// Offset vertical da ambient shadow (px). Maior.
    pub shadow_ambient_offset_y: i32,
    /// Botões de ação (H54.P4). Estilo ghost com fill sutil; hover
    /// destaca. Sem distinção de "primary" — todos iguais nesta fase.
    pub action_btn_bg: Color32F,
    pub action_btn_bg_hover: Color32F,
    pub action_btn_label_color: Color32F,
    pub action_btn_corner: i32,
    pub action_btn_padding_h: i32,
    pub action_btn_gap: i32,
    pub action_btn_label_size: f32,
    /// Espaço entre o body do toast e a linha de botões.
    pub action_btn_top_margin: i32,
    /// H54.P4.fix: altura visível do botão (= hit target, sem padding
    /// invisível). PCD-first: alvo de clique confortável. Default 40,
    /// PCD Max 48+ via `action_btn_height_pcd`.
    pub action_btn_height: i32,
    #[allow(dead_code)]
    pub action_btn_height_pcd: i32,
    pub title_color: Color32F,
    pub body_color: Color32F,
    pub time_color: Color32F,
    pub icon_size: i32,
    pub icon_corner: i32,
    pub title_size: f32,
    pub body_size: f32,
    pub time_size: f32,
    pub timeout_ms: u64,
    pub margin: i32,
    pub fade_ms: u64,
    pub max_body_lines: usize,
    pub padding: i32,
    /// H54.P5: central de notificações — painel borda direita.
    pub panel_width: i32,
    pub panel_padding: i32,
    pub panel_bg: Color32F,
    pub panel_scrim: Color32F,
    /// Sombra na borda esquerda do painel (mesma linguagem dos toasts).
    pub panel_shadow_blur: i32,
    pub panel_shadow_color: Color32F,
    /// Header do painel: título "Notificações", toggle DND, "Limpar tudo".
    pub panel_header_height: i32,
    pub panel_header_title_color: Color32F,
    pub panel_header_title_size: f32,
    pub panel_btn_bg: Color32F,
    pub panel_btn_bg_hover: Color32F,
    pub panel_btn_label_color: Color32F,
    pub panel_btn_corner: i32,
    pub panel_btn_padding_h: i32,
    pub panel_btn_height: i32,
    pub panel_btn_label_size: f32,
    /// Bg do toggle DND quando ativo (estado on). Cor amber discreta.
    pub panel_dnd_active_bg: Color32F,
    /// Lista de histórico — cada entry.
    pub panel_item_height: i32,
    pub panel_item_corner: i32,
    pub panel_item_padding: i32,
    pub panel_item_gap: i32,
    pub panel_item_icon_size: i32,
    pub panel_item_title_color: Color32F,
    pub panel_item_title_size: f32,
    pub panel_item_body_color: Color32F,
    pub panel_item_body_size: f32,
    pub panel_item_time_color: Color32F,
    pub panel_item_time_size: f32,
    pub panel_item_hover_bg: Color32F,
    pub panel_item_dismiss_size: i32,
    pub panel_item_dismiss_color: Color32F,
    /// Máximo de linhas do body de cada entrada do histórico. Overflow
    /// vira "…" na última linha. H54.P5.fix.
    pub panel_item_max_body_lines: usize,
    /// Empty state quando não há histórico.
    pub panel_empty_label_color: Color32F,
    pub panel_empty_label_size: f32,
    /// Badge de não-vistas no sino.
    pub bell_badge_bg: Color32F,
    pub bell_badge_text_color: Color32F,
    /// Diâmetro do círculo do badge.
    pub bell_badge_size: i32,
    pub bell_badge_font_size: f32,
    /// H54.P6 som de notificação.
    /// Path dos assets de áudio (resolvidos contra cwd ou assets/).
    /// Se ausente em runtime, falha em silêncio (per spec).
    pub sound_default_path: String,
    pub sound_critical_path: String,
    /// Intervalo mínimo entre dois plays — rajada de 6 notif = 1 som.
    pub sound_debounce_ms: u64,
    /// Estado inicial do toggle de som no painel. Default = on.
    pub sound_enabled_default: bool,
    /// H54.P7 piscada de tela (visual bell). Acessibilidade pra quem
    /// não ouve. LIMITES DE SEGURANÇA são duros (risco fotossensível),
    /// ajustar com critério.
    /// Cores do overlay (alpha embutida = 1.0; alpha real vem dos peaks).
    pub flash_normal_color: Color32F,
    pub flash_critical_color: Color32F,
    /// Teto de opacidade. Default conservador. NUNCA aumentar sem
    /// reavaliar segurança.
    pub flash_normal_peak_alpha: f32,
    pub flash_critical_peak_alpha: f32,
    /// Durações (ms) — fade-in + hold + fade-out, total 250-400ms.
    pub flash_fade_in_ms: u64,
    pub flash_hold_ms: u64,
    pub flash_fade_out_ms: u64,
    /// Debounce — rajada vira 1 piscada só. Anti-strobe.
    pub flash_debounce_ms: u64,
    /// Default desligada. Só PCD Max liga (via theme.toml override).
    pub flash_enabled_default: bool,
}

/// Grid de ícones de arquivo no desktop. H50.
#[derive(Debug, Clone)]
pub struct DesktopIconsTokens {
    /// Lado do quadrado de ícone (px).
    pub icon_size: i32,
    /// Largura de cada célula do grid (icon + padding lateral).
    pub cell_width: i32,
    /// Altura de cada célula (icon + label + padding vertical).
    pub cell_height: i32,
    /// Distância da borda do output ao primeiro ícone.
    pub grid_margin: i32,
    pub label_color: Color32F,
    pub label_size: f32,
    /// Máximo de chars antes de truncar com '...'.
    pub label_max_chars: usize,
    /// Cor do bg do ícone selecionado (rect arredondado atrás).
    /// H50.2a.
    pub selection_bg: Color32F,
    /// Raio dos cantos do bg de seleção.
    pub selection_corner_radius: i32,
    /// Cor do rubber-band (caixa de seleção desenhada com drag em
    /// área vazia). Mesma família do selection_bg, alpha um pouco
    /// menor. H50.2c.
    pub rubber_band_bg: Color32F,
    /// Cor da borda do rubber-band (amber 1px).
    pub rubber_band_border: Color32F,
    /// Campo de renomear inline (H53). Substitui o KDialog.
    pub rename_field_bg: Color32F,
    pub rename_field_border: Color32F,
    pub rename_field_text: Color32F,
    pub rename_field_selection_bg: Color32F,
    pub rename_field_cursor: Color32F,
    pub rename_field_border_width: i32,
    pub rename_field_corner_radius: i32,
    pub rename_field_cursor_blink_ms: u64,
    /// H56: duração da animação de slide ao "Organizar ícones".
    /// Ease_out cubic. Se accessibility.reduce_motion estiver ativo,
    /// o slide é pulado e as posições mudam direto.
    pub arrange_anim_duration_ms: u64,
    /// H56 parte 3 (pre-Colar): opacidade dos ícones marcados como
    /// "Recortar" no clipboard interno. Default 0.55 — translúcido
    /// claro pra sinalizar pending sem quebrar AA do label abaixo.
    pub cut_opacity: f32,
}

/// Popup de calendário ao clicar no relógio. H55.
/// Tudo deriva de `cell_size` — preset PCD Max sobe cell_size e o
/// popup inteiro escala junto. Grade sempre 6×7 (tamanho do popup
/// fixo independente do mês exibido).
#[derive(Debug, Clone)]
pub struct CalendarTokens {
    /// Lado da célula de dia (px). Default 40 — alvo de clique
    /// confortável. Preset PCD Max pode subir.
    pub cell_size: i32,
    /// Padding lateral interno do popup.
    pub padding_lateral: i32,
    /// Padding vertical (topo + base).
    pub padding_vertical: i32,
    /// Altura do cabeçalho (mês/ano + setas — setas vêm na Fatia 3).
    pub header_height: i32,
    /// Altura da linha dos dias da semana.
    pub diasemana_height: i32,
    /// Altura do rodapé (data por extenso + botão Hoje — Fatia 4).
    pub footer_height: i32,
    /// Cor do título "Maio 2026" no cabeçalho.
    pub titulo_fg: Color32F,
    pub titulo_size: f32,
    /// Cor das setas anterior/próximo (Fatia 3).
    pub seta_fg: Color32F,
    pub seta_fg_hover: Color32F,
    /// Cor dos rótulos dom/seg/ter... na linha dos dias da semana.
    pub diasemana_fg: Color32F,
    pub diasemana_size: f32,
    /// Cor do número do dia (mês exibido).
    pub dia_fg: Color32F,
    pub dia_size: f32,
    /// Cor do número de dia de mês vizinho (apagado por matiz, não
    /// por contraste — passa AA mesmo apagado).
    pub dia_vizinho_fg: Color32F,
    /// Destaque do hoje: bg coral, fg cream, raio do quadrado
    /// inscrito na célula com folga ~3px.
    pub hoje_bg: Color32F,
    pub hoje_fg: Color32F,
    pub hoje_raio: i32,
    /// Cor da data por extenso no rodapé (Fatia 4).
    pub rodape_fg: Color32F,
    pub rodape_size: f32,
    /// Botão "Hoje" do rodapé (Fatia 4).
    pub hoje_botao_bg: Color32F,
    pub hoje_botao_fg: Color32F,
    pub hoje_botao_bg_hover: Color32F,
}

/// Wallpaper padrão shipado com o luma. H49.
///
/// `default_path` é relativo ao CWD do binário (geralmente o root
/// do repo em dev). Quando o user tem
/// `~/.config/luma/wallpaper.{png,jpg,jpeg}`, esse override.
#[derive(Debug, Clone)]
pub struct WallpaperTokens {
    pub default_path: String,
}

/// Tokens do menu de contexto do desktop. H48.
///
/// Abre no botão direito sobre área vazia do desktop, posicionado no
/// cursor. Lista vertical de items 40px, separadores 1px, ícones 16px
/// à esquerda do label, atalho à direita (cinza). Items desabilitados
/// renderizam em `item_disabled` color.
#[derive(Debug, Clone)]
pub struct ContextMenuTokens {
    pub width: i32,
    pub bg: Color32F,
    pub corner_radius: i32,
    pub top_accent: Color32F,
    pub item_height: i32,
    pub item_text: Color32F,
    pub item_text_size: f32,
    pub item_hover_bg: Color32F,
    pub item_disabled: Color32F,
    pub shortcut_color: Color32F,
    pub separator: Color32F,
    pub icon_size: i32,
    /// Padding horizontal interno em cada item.
    pub item_padding_horizontal: i32,
    /// Gap entre ícone e label.
    pub icon_text_gap: i32,
    /// Altura do separator (linha horizontal entre grupos).
    pub separator_height: i32,
    /// Margem vertical do separator dentro do "slot" alocado.
    pub separator_padding: i32,
}

impl Default for OsdMediaTokens {
    fn default() -> Self {
        Self {
            width: 360,
            height: 108,
            corner_radius: 18,
            bg: hex_color("#1F1611", 0.95),
            art_size: 76,
            padding: 12,
            title_color: hex_color("#FBF6EE", 1.0),       // cream
            title_size: 14.0,
            artist_color: hex_color("#A89D92", 1.0),      // text_secondary
            artist_size: 11.0,
            play_btn_bg: hex_color("#E89B3D", 1.0),       // accent_amber
            play_btn_size: 30,
            side_btn_size: 22,
            control_color: hex_color("#FBF6EE", 1.0),     // cream
            progress_bg: hex_color("#3D2520", 1.0),       // applet_bg_active
            progress_fill: hex_color("#E89B3D", 1.0),     // accent_amber
            progress_height: 3,
            timeout_ms: 3000,
            fade_ms: 200,
        }
    }
}

impl Default for ContextMenuTokens {
    fn default() -> Self {
        Self {
            width: 320,
            bg: hex_color("#1F1611", 0.97),
            corner_radius: 12,
            top_accent: hex_color("#E89B3D", 1.0),
            item_height: 40,
            item_text: hex_color("#FBF6EE", 1.0),
            item_text_size: 13.0,
            item_hover_bg: hex_color("#3D2520", 1.0),
            item_disabled: hex_color("#8B7D72", 1.0),
            shortcut_color: hex_color("#8B7D72", 1.0),
            separator: hex_color("#3D2520", 1.0),
            icon_size: 16,
            item_padding_horizontal: 14,
            icon_text_gap: 12,
            separator_height: 1,
            separator_padding: 7,
        }
    }
}

impl Default for ClockTokens {
    fn default() -> Self {
        Self {
            // H33.taskbar.E.14: 17 / 10 (era 13 / 9).
            time_size: 17.0,
            date_size: 10.0,
            padding_right: 16,
            time_color: hex_color("#FBF6EE", 1.0),
            date_color: hex_color("#A89D92", 1.0),
            time_format: "%H:%M".to_string(),
            date_format: "%a · %d %b".to_string(),
            font_family: "Fraunces".to_string(),
            time_weight: 600,
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
    notifications: Option<NotificationsTokensFile>,
}

/// Overrides do bloco `[notifications]` no luma.toml.
/// Apenas tokens com motivo válido pra serem mutados por user moram
/// aqui — toggles de gate de alerta (acessibilidade). Cores/tamanhos
/// ficam fora pra preservar a estética Golden Hour curada.
#[derive(Debug, Deserialize, Default)]
struct NotificationsTokensFile {
    /// H54.P7: liga a piscada de tela. Default false. PCD Max preset
    /// põe true. Pode ser sobrescrita por qualquer user no luma.toml.
    flash_enabled_default: Option<bool>,
    /// H54.P6: idem pro som. Default true. Inverter no toml = mudo
    /// por default sem precisar do toggle no painel.
    sound_enabled_default: Option<bool>,
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
    shadow_blur_inactive: Option<i32>,
    shadow_offset_y: Option<i32>,
    shadow_opacity: Option<f32>,
    shadow_opacity_inactive: Option<f32>,
    shadow_color: Option<String>,
    titlebar_bg_top: Option<String>,
    titlebar_bg_bottom: Option<String>,
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
            shadow_blur_inactive: self
                .shadow_blur_inactive
                .unwrap_or(def.shadow_blur_inactive),
            shadow_offset_y: self.shadow_offset_y.unwrap_or(def.shadow_offset_y),
            shadow_opacity: self.shadow_opacity.unwrap_or(def.shadow_opacity),
            shadow_opacity_inactive: self
                .shadow_opacity_inactive
                .unwrap_or(def.shadow_opacity_inactive),
            shadow_color: self
                .shadow_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.shadow_color),
            titlebar_bg_top: self
                .titlebar_bg_top
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.titlebar_bg_top),
            titlebar_bg_bottom: self
                .titlebar_bg_bottom
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.titlebar_bg_bottom),
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
    inactive_color: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ShowDesktopTokensFile {
    width: Option<i32>,
    border_left_alpha: Option<f32>,
    hover_bg_alpha: Option<f32>,
    size: Option<i32>,
    corner_radius: Option<i32>,
    border_alpha: Option<f32>,
    icon_alpha_default: Option<f32>,
    hover_bg: Option<String>,
    active_bg: Option<String>,
    icon_size: Option<i32>,
    icon_normal_name: Option<String>,
    icon_active_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ClockTokensFile {
    time_size: Option<f32>,
    date_size: Option<f32>,
    padding_right: Option<i32>,
    time_color: Option<String>,
    date_color: Option<String>,
    time_format: Option<String>,
    date_format: Option<String>,
    font_family: Option<String>,
    time_weight: Option<u16>,
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
        let notifications = self
            .notifications
            .map(|t| t.into_tokens(&default.notifications))
            .unwrap_or_else(|| default.notifications.clone());

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
            popup: default.popup.clone(),
            osd: default.osd.clone(),
            osd_media: default.osd_media.clone(),
            context_menu: default.context_menu.clone(),
            wallpaper: default.wallpaper.clone(),
            desktop_icons: default.desktop_icons.clone(),
            notifications,
            calendario: default.calendario.clone(),
            battery_popup: default.battery_popup.clone(),
            wifi_popup: default.wifi_popup.clone(),
            lock: default.lock.clone(),
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
            // System tray e right_group ainda não têm parser TOML; usam
            // defaults (sem overrides). Adicionar TaskbarSystemTrayTokensFile
            // + TaskbarRightGroupTokensFile em commit fix se precisar.
            system_tray: def.system_tray.clone(),
            tray_overflow: def.tray_overflow.clone(),
            tray_sni: def.tray_sni.clone(),
            right_group: def.right_group.clone(),
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
            inactive_color: self
                .inactive_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.inactive_color),
        }
    }
}

impl NotificationsTokensFile {
    /// Override mínimo: só flags de gate de alerta. Todos os outros
    /// tokens vêm do default Golden Hour pra preservar a estética
    /// curada. Cores/tamanhos não são overridáveis por design — se
    /// alguém quiser mudar, edita theme.rs.
    fn into_tokens(self, def: &NotificationsTokens) -> NotificationsTokens {
        let mut t = def.clone();
        if let Some(v) = self.flash_enabled_default {
            t.flash_enabled_default = v;
        }
        if let Some(v) = self.sound_enabled_default {
            t.sound_enabled_default = v;
        }
        t
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
            size: self.size.unwrap_or(def.size),
            corner_radius: self.corner_radius.unwrap_or(def.corner_radius),
            border_alpha: self.border_alpha.unwrap_or(def.border_alpha),
            icon_alpha_default: self.icon_alpha_default.unwrap_or(def.icon_alpha_default),
            hover_bg: self
                .hover_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.hover_bg),
            active_bg: self
                .active_bg
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.active_bg),
            icon_size: self.icon_size.unwrap_or(def.icon_size),
            icon_normal_name: self
                .icon_normal_name
                .unwrap_or_else(|| def.icon_normal_name.clone()),
            icon_active_name: self
                .icon_active_name
                .unwrap_or_else(|| def.icon_active_name.clone()),
        }
    }
}

impl ClockTokensFile {
    fn into_tokens(self, def: &ClockTokens) -> ClockTokens {
        ClockTokens {
            time_size: self.time_size.unwrap_or(def.time_size),
            date_size: self.date_size.unwrap_or(def.date_size),
            padding_right: self.padding_right.unwrap_or(def.padding_right),
            time_color: self
                .time_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.time_color),
            date_color: self
                .date_color
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(def.date_color),
            time_format: self.time_format.unwrap_or_else(|| def.time_format.clone()),
            date_format: self.date_format.unwrap_or_else(|| def.date_format.clone()),
            font_family: self.font_family.unwrap_or_else(|| def.font_family.clone()),
            time_weight: self.time_weight.unwrap_or(def.time_weight),
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
