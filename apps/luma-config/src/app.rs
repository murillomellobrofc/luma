//! App iced do luma-config — padrão Elm (State / Message / update / view).
//!
//! **Frameless** (o Luma decora server-side). 2b.2: a landing é a
//! `CategoryGrid` renderizando a taxonomia (5 seções), com uma barra de
//! busca estática no topo. O dado da taxonomia mora aqui (é a estrutura
//! deste app); o `libluma` fornece só o componente. Navegação ao clicar
//! (2c), busca funcional (2e) e ícones de verdade (2b.3) ainda não entram.

use iced::widget::{column, container, responsive, scrollable, text};
use iced::{Background, Border, Element, Length, Padding, Task, Theme};
use libluma::{GoldenHour, GridItem, GridSection};

/// Estado do app: paleta + taxonomia (construída uma vez, com as hues
/// resolvidas dos tokens). A view empresta `&taxonomy` pra grade.
pub struct State {
    gh: GoldenHour,
    taxonomy: Vec<GridSection<'static>>,
}

impl State {
    pub fn new() -> Self {
        let gh = GoldenHour::load();
        let taxonomy = build_taxonomy(&gh);
        Self { gh, taxonomy }
    }
}

/// Mensagens da UI. Vazio por ora — sem interação até a 2c (navegação).
#[derive(Debug, Clone)]
pub enum Message {}

pub fn update(_state: &mut State, _message: Message) -> Task<Message> {
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
        let cols = libluma::columns_for(content_w);

        // Barra de busca estática (só visual; 2e). Entra DENTRO do
        // container centralizado, alinhada com a grade.
        let search = container(text("Buscar configurações…").size(14).color(gh.honey_dim))
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

        let content = column(vec![search.into(), grid.into()])
            .spacing(20)
            .width(Length::Fill);

        // Conteúdo centralizado via padding lateral (= side): a largura
        // interna fica content_w, o resto sobra de margem dos dois lados.
        let centered = container(content)
            .padding(Padding::from([24.0, side]))
            .width(Length::Fill);

        // O scrollable é a JANELA INTEIRA (barra na borda direita),
        // estilizado em Golden Hour. Rola só quando passa da altura.
        scrollable(centered)
            .height(Length::Fixed(size.height))
            .style(libluma::scrollbar_style())
            .into()
    })
    .into()
}

/// Taxonomia travada (2b.2). Hues resolvidas da paleta Golden Hour.
/// Enabled hoje: Widgets e Wallpaper; o resto entra como "Em breve".
fn build_taxonomy(gh: &GoldenHour) -> Vec<GridSection<'static>> {
    let item = |label: &'static str, enabled: bool| GridItem {
        label,
        icon: "", // glifo é 2b.3
        enabled,
    };
    vec![
        GridSection {
            label: "Aparência",
            hue: gh.amber,
            items: vec![
                item("Tema e cores", false),
                item("Fontes", false),
                item("Wallpaper", true),
                item("Barra de tarefas", false),
                item("Widgets", true),
            ],
        },
        GridSection {
            label: "Acessibilidade",
            hue: gh.sage,
            items: vec![
                item("Alvos de toque", false),
                item("Contraste", false),
                item("Sino visual", false),
                item("Movimento", false),
            ],
        },
        GridSection {
            label: "Privacidade",
            hue: gh.coral,
            items: vec![
                item("Permissões de tela", false),
                item("Câmera e microfone", false),
                item("Área de transferência", false),
                item("Telemetria", false),
            ],
        },
        GridSection {
            label: "Preferências",
            hue: gh.honey,
            items: vec![
                item("Notificações", false),
                item("Área de trabalho", false),
                item("Tela de bloqueio", false),
            ],
        },
        GridSection {
            label: "Sistema",
            hue: gh.warm_neutral,
            items: vec![
                item("Sobre", false),
                item("Restaurar padrões", false),
            ],
        },
    ]
}
