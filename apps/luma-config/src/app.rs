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

/// Estado do app: paleta + taxonomia + qual view está aberta.
pub struct State {
    gh: GoldenHour,
    taxonomy: Vec<GridSection<'static, Message>>,
    view: View,
}

/// Tela visível: a grade (landing) ou a página de uma categoria.
#[derive(Clone, Copy)]
enum View {
    Grid,
    Category(&'static str),
}

impl State {
    pub fn new() -> Self {
        let gh = GoldenHour::load();
        let taxonomy = build_taxonomy(&gh);
        Self {
            gh,
            taxonomy,
            view: View::Grid,
        }
    }
}

/// Mensagens da UI (navegação, 2c).
#[derive(Debug, Clone)]
pub enum Message {
    /// Abrir a página de uma categoria (o `&str` é o título/id dela).
    OpenCategory(&'static str),
    /// Voltar pra grade.
    Back,
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::OpenCategory(label) => state.view = View::Category(label),
        Message::Back => state.view = View::Grid,
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
                // Drill-in (2c): cabeçalho de voltar + stub (conteúdo é 2d).
                let header = libluma::page_header(label, Message::Back);
                let stub = text("Conteúdo em breve").size(14).color(gh.honey_dim);
                column(vec![header, stub.into()])
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

/// Taxonomia travada. Hues resolvidas da paleta Golden Hour; cada
/// categoria mapeia pro seu ícone do pack (`assets/icons/<nome>.svg`,
/// monocromático `currentColor`). O `libluma` recebe o handle e tinge na
/// hue da seção. Enabled hoje: Widgets e Wallpaper.
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
                item("Alvos de toque", icon!("alvos-de-toque"), false),
                item("Contraste", icon!("contraste"), false),
                item("Sino visual", icon!("sino-visual"), false),
                item("Movimento", icon!("movimento"), false),
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
