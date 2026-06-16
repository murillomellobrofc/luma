//! App iced do luma-config — padrão Elm (State / Message / update / view).
//!
//! **Frameless**: o Luma decora as janelas dos apps dele server-side (o
//! header oficial Golden Hour é desenhado pelo compositor), então o app
//! NÃO desenha barra própria — o corpo começa direto no conteúdo. O
//! título da janela é setado no `main` e o compositor lê pra desenhar o
//! header. A grade (2b.2) e a navegação/D-Bus (2c) ainda não entraram.

use iced::widget::{column, container, row, text};
use iced::{Element, Length, Task, Theme};
use libluma::GoldenHour;

/// Estado do app. Por ora só a paleta Golden Hour resolvida (tokens).
pub struct State {
    gh: GoldenHour,
}

impl State {
    pub fn new() -> Self {
        Self {
            gh: GoldenHour::load(),
        }
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

    // Sem barra própria: o corpo começa direto. O header é do compositor.
    let heading = text("Configurações do Luma").size(24).color(gh.cream);
    let sub = text("Esqueleto iced — app frameless, header é do compositor.")
        .size(13)
        .color(gh.honey_dim);

    let tiles = row(vec![
        libluma::category_tile::<Message>("Tema e cores", "", gh.amber, true),
        libluma::category_tile::<Message>("Acessibilidade", "", gh.sage, true),
        libluma::category_tile::<Message>("Privacidade", "", gh.coral, true),
        libluma::category_tile::<Message>("Barra de tarefas", "", gh.warm_neutral, false),
    ])
    .spacing(12);

    let content = column(vec![heading.into(), sub.into(), tiles.into()]).spacing(24);

    container(content)
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
