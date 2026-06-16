//! App iced mínimo do M1 — padrão Elm (State / Message / update / view).
//!
//! A view é deliberadamente mínima: um placeholder + uma linha de
//! `CategoryTile` provando os tokens. **Não** porta a sidebar nem as
//! páginas antigas (estão em `legacy/`, viram grade na 2b.2). Sem
//! navegação, sem D-Bus religado ainda (2c).

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

/// Mensagens da UI. Vazio em M1 — não há interação ainda (clique e
/// navegação voltam na 2c).
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

    let heading = text("Configurações do Luma").size(24).color(gh.cream);
    let sub = text("Esqueleto iced (M1) — tokens e CategoryTile.")
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
