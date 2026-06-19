//! App iced do luma-config — padrão Elm (State / Message / update / view).
//!
//! **Frameless** (o Luma decora server-side). 2b.2: a landing é a
//! `CategoryGrid` renderizando a taxonomia (5 seções), com uma barra de
//! busca estática no topo. O dado da taxonomia mora aqui (é a estrutura
//! deste app); o `libluma` fornece só o componente. Navegação ao clicar
//! (2c), busca funcional (2e) e ícones de verdade (2b.3) ainda não entram.

use iced::widget::{column, container, responsive, scrollable, svg, text};
use iced::{Background, Border, Element, Length, Padding, Task, Theme};
use libluma::{GoldenHour, GridItem, GridSection};

/// Estado do app: paleta + taxonomia + qual view está aberta + estado
/// local dos controles (sem efeito real ainda; o wiring no compositor via
/// D-Bus é um passo depois).
pub struct State {
    gh: GoldenHour,
    taxonomy: Vec<GridSection<'static, Message>>,
    view: View,
    touch: TouchTargets,
    contrast: Contrast,
    motion: Motion,
    bell: VisualBell,
}

/// Tela visível: a grade (landing) ou a página de uma categoria.
#[derive(Clone, Copy)]
enum View {
    Grid,
    Category(&'static str),
}

/// Estado local da página **Alvos de toque** (2d). Modelo A: cada
/// categoria tem suas próprias configurações. Sem efeito real ainda.
#[derive(Debug, Clone, Copy)]
struct TouchTargets {
    /// Alvos de toque maiores (default ligado, igual o mockup).
    larger: bool,
    /// Tamanho mínimo do alvo em px (>= 44, o piso WCAG AAA).
    min_size: f32,
    /// Realçar o foco do teclado.
    highlight_focus: bool,
}

impl Default for TouchTargets {
    fn default() -> Self {
        Self {
            larger: true,
            min_size: 44.0,
            highlight_focus: false,
        }
    }
}

/// Estado local da página **Contraste** (2d.2).
#[derive(Debug, Clone, Copy, Default)]
struct Contrast {
    high_contrast: bool,
    reduce_transparency: bool,
}

/// Estado local da página **Movimento** (2d.2). `anim_speed` em %
/// (100 = normal).
#[derive(Debug, Clone, Copy)]
struct Motion {
    reduce_motion: bool,
    anim_speed: f32,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            reduce_motion: false,
            anim_speed: 100.0,
        }
    }
}

/// Estado local da página **Sino visual** (2d.2). `photosafe` (modo seguro
/// pra fotossensibilidade) vem **ligado** por padrão — flash suave, sem
/// estrobo. `flash_intensity` em %.
#[derive(Debug, Clone, Copy)]
struct VisualBell {
    enabled: bool,
    photosafe: bool,
    flash_intensity: f32,
}

impl Default for VisualBell {
    fn default() -> Self {
        Self {
            enabled: false,
            photosafe: true,
            flash_intensity: 60.0,
        }
    }
}

impl State {
    pub fn new() -> Self {
        let gh = GoldenHour::load();
        let taxonomy = build_taxonomy(&gh);
        Self {
            gh,
            taxonomy,
            view: View::Grid,
            touch: TouchTargets::default(),
            contrast: Contrast::default(),
            motion: Motion::default(),
            bell: VisualBell::default(),
        }
    }
}

/// Mensagens da UI (navegação 2c + controles locais 2d).
#[derive(Debug, Clone)]
pub enum Message {
    /// Abrir a página de uma categoria (o `&str` é o título/id dela).
    OpenCategory(&'static str),
    /// Voltar pra grade.
    Back,
    // Página Alvos de toque (estado local; sem backend ainda).
    TouchLarger(bool),
    TouchMinSize(f32),
    TouchHighlightFocus(bool),
    // Página Contraste (2d.2).
    ContrastHigh(bool),
    ContrastReduceTransparency(bool),
    // Página Movimento (2d.2).
    MotionReduce(bool),
    MotionAnimSpeed(f32),
    // Página Sino visual (2d.2).
    BellEnabled(bool),
    BellPhotosafe(bool),
    BellFlashIntensity(f32),
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::OpenCategory(label) => state.view = View::Category(label),
        Message::Back => state.view = View::Grid,
        Message::TouchLarger(v) => state.touch.larger = v,
        Message::TouchMinSize(v) => state.touch.min_size = v,
        Message::TouchHighlightFocus(v) => state.touch.highlight_focus = v,
        Message::ContrastHigh(v) => state.contrast.high_contrast = v,
        Message::ContrastReduceTransparency(v) => state.contrast.reduce_transparency = v,
        Message::MotionReduce(v) => state.motion.reduce_motion = v,
        Message::MotionAnimSpeed(v) => state.motion.anim_speed = v,
        Message::BellEnabled(v) => state.bell.enabled = v,
        Message::BellPhotosafe(v) => state.bell.photosafe = v,
        Message::BellFlashIntensity(v) => state.bell.flash_intensity = v,
    }
    Task::none()
}

/// Theme Golden Hour (iced) — vem do `libluma`, resolvido dos tokens.
pub fn theme(_state: &State) -> Theme {
    libluma::golden_hour_theme()
}

pub fn view(state: &State) -> Element<'_, Message> {
    let gh = state.gh;
    let surface = libluma::tokens::to_iced(libluma::tokens::semantic_default().surface);

    responsive(move |size| {
        // Largura confortável do conteúdo (chute ~860px). Abaixo disso
        // enche; no cap, para e centraliza. `side` = metade do excesso
        // (margem dos dois lados), com mínimo pra não colar nas bordas.
        const MIN_PAD: f32 = 32.0;
        const MAX_W: f32 = 860.0;
        let content_w = (size.width - 2.0 * MIN_PAD).clamp(0.0, MAX_W);
        let side = ((size.width - content_w) / 2.0).max(MIN_PAD);

        // O conteúdo depende da view: grade (landing) ou página da categoria.
        let inner: Element<'_, Message> = match state.view {
            View::Grid => {
                let cols = libluma::columns_for(content_w);
                // Barra de busca estática (só visual; 2e), alinhada com a grade.
                let search =
                    container(text("Buscar configurações…").size(14).color(gh.honey_dim))
                        .padding([10, 14])
                        .width(Length::Fill)
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(surface)),
                            border: Border {
                                radius: 12.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        });
                let grid = libluma::category_grid::<Message>(&state.taxonomy, cols);
                column(vec![search.into(), grid.into()])
                    .spacing(20)
                    .width(Length::Fill)
                    .into()
            }
            View::Category(label) => {
                // Drill-in (2c) + conteúdo (2d). Modelo A: cada categoria
                // tem sua página. Por ora só "Alvos de toque" é real; o
                // resto segue stub até a sua vez.
                let header = libluma::page_header(label, Message::Back);
                let body: Element<'_, Message> = match label {
                    "Alvos de toque" => touch_targets_page(state.touch),
                    "Contraste" => contrast_page(state.contrast),
                    "Movimento" => motion_page(state.motion),
                    "Sino visual" => visual_bell_page(state.bell),
                    _ => text("Conteúdo em breve")
                        .size(14)
                        .color(gh.honey_dim)
                        .into(),
                };
                column(vec![header, body])
                    .spacing(24)
                    .width(Length::Fill)
                    .into()
            }
        };

        // Conteúdo centralizado via padding lateral; o scrollable é a janela
        // inteira (barra na borda direita), estilizado em Golden Hour.
        let centered = container(inner)
            .padding(Padding::from([24.0, side]))
            .width(Length::Fill);
        scrollable(centered)
            .height(Length::Fixed(size.height))
            .style(libluma::scrollbar_style())
            .into()
    })
    .into()
}

/// Página da categoria **Alvos de toque** (2d). Um `SettingsGroup` com as
/// configurações DESSA categoria; estado local (sem backend ainda). As
/// outras configs de acessibilidade (contraste, movimento, sino) moram nas
/// suas próprias páginas, que vêm depois.
fn touch_targets_page(t: TouchTargets) -> Element<'static, Message> {
    libluma::settings_group(
        None,
        vec![
            libluma::toggle_row("Alvos de toque maiores", t.larger, Message::TouchLarger),
            libluma::slider_row(
                "Tamanho mínimo do alvo",
                t.min_size,
                44.0..=64.0,
                1.0,
                "px",
                Message::TouchMinSize,
            ),
            libluma::toggle_row(
                "Realçar foco do teclado",
                t.highlight_focus,
                Message::TouchHighlightFocus,
            ),
        ],
    )
}

/// Página da categoria **Contraste** (2d.2). Estado local.
fn contrast_page(c: Contrast) -> Element<'static, Message> {
    libluma::settings_group(
        None,
        vec![
            libluma::toggle_row("Alto contraste", c.high_contrast, Message::ContrastHigh),
            libluma::toggle_row(
                "Reduzir transparência",
                c.reduce_transparency,
                Message::ContrastReduceTransparency,
            ),
        ],
    )
}

/// Página da categoria **Movimento** (2d.2). Velocidade das animações em %
/// (100 = normal). Estado local.
fn motion_page(m: Motion) -> Element<'static, Message> {
    libluma::settings_group(
        None,
        vec![
            libluma::toggle_row("Reduzir movimento", m.reduce_motion, Message::MotionReduce),
            libluma::slider_row(
                "Velocidade das animações",
                m.anim_speed,
                0.0..=200.0,
                25.0,
                "%",
                Message::MotionAnimSpeed,
            ),
        ],
    )
}

/// Página da categoria **Sino visual** (2d.2). Flash da tela no lugar do
/// som; modo seguro pra fotossensibilidade ligado por padrão. Estado local.
fn visual_bell_page(b: VisualBell) -> Element<'static, Message> {
    libluma::settings_group(
        None,
        vec![
            libluma::toggle_row("Sino visual", b.enabled, Message::BellEnabled),
            libluma::toggle_row(
                "Modo seguro para fotossensibilidade",
                b.photosafe,
                Message::BellPhotosafe,
            ),
            libluma::slider_row(
                "Intensidade do flash",
                b.flash_intensity,
                0.0..=100.0,
                10.0,
                "%",
                Message::BellFlashIntensity,
            ),
        ],
    )
}

/// Taxonomia travada. Hues resolvidas da paleta Golden Hour; cada
/// categoria mapeia pro seu ícone do pack (`assets/icons/<nome>.svg`,
/// monocromático `currentColor`). O `libluma` recebe o handle e tinge na
/// hue da seção. Enabled hoje: Widgets, Wallpaper e a seção Acessibilidade
/// inteira (Alvos de toque, Contraste, Sino visual, Movimento).
fn build_taxonomy(gh: &GoldenHour) -> Vec<GridSection<'static, Message>> {
    // Handle de svg do pack (bundlado via include_bytes — sem dor de path).
    macro_rules! icon {
        ($name:literal) => {
            svg::Handle::from_memory(
                include_bytes!(concat!("../assets/icons/", $name, ".svg")).as_slice(),
            )
        };
    }
    // Habilitado => clicável (abre a página da categoria); "em breve" => sem
    // clique. O id da categoria é o próprio label.
    let item = |label: &'static str, icon: svg::Handle, enabled: bool| GridItem {
        label,
        icon,
        enabled,
        on_press: if enabled {
            Some(Message::OpenCategory(label))
        } else {
            None
        },
    };
    vec![
        GridSection {
            label: "Aparência",
            hue: gh.amber,
            items: vec![
                item("Tema e cores", icon!("tema-e-cores"), false),
                item("Fontes", icon!("fontes"), false),
                item("Wallpaper", icon!("wallpaper"), true),
                item("Barra de tarefas", icon!("barra-de-tarefas"), false),
                item("Widgets", icon!("widgets"), true),
            ],
        },
        GridSection {
            label: "Acessibilidade",
            hue: gh.sage,
            items: vec![
                item("Alvos de toque", icon!("alvos-de-toque"), true),
                item("Contraste", icon!("contraste"), true),
                item("Sino visual", icon!("sino-visual"), true),
                item("Movimento", icon!("movimento"), true),
            ],
        },
        GridSection {
            label: "Privacidade",
            hue: gh.coral,
            items: vec![
                item("Permissões de tela", icon!("permissoes-de-tela"), false),
                item("Câmera e microfone", icon!("camera-e-microfone"), false),
                item("Área de transferência", icon!("area-de-transferencia"), false),
                item("Telemetria", icon!("telemetria"), false),
            ],
        },
        GridSection {
            label: "Preferências",
            hue: gh.honey,
            items: vec![
                item("Notificações", icon!("notificacoes"), false),
                item("Área de trabalho", icon!("area-de-trabalho"), false),
                item("Tela de bloqueio", icon!("tela-de-bloqueio"), false),
            ],
        },
        GridSection {
            label: "Sistema",
            hue: gh.warm_neutral,
            items: vec![
                item("Sobre", icon!("sobre"), false),
                item("Restaurar padrões", icon!("restaurar-padroes"), false),
            ],
        },
    ]
}
