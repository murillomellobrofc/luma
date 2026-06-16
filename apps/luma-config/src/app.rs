//! App iced do luma-config — padrão Elm (State / Message / update / view).
//!
//! M2: a moldura Golden Hour custom (`libluma::title_bar`) no topo, com
//! os controles ligados aos window actions do iced. O corpo do M1
//! (heading + linha de tiles) segue intacto embaixo. A grade (2b.2) e a
//! navegação/D-Bus (2c) ainda não entraram.

use iced::widget::{column, container, row, text};
use iced::{window, Element, Length, Task, Theme};
use libluma::{GoldenHour, TitleBarMessage};

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

/// Mensagens da UI. M2 só tem as da barra de título (controles + drag).
#[derive(Debug, Clone)]
pub enum Message {
    TitleBar(TitleBarMessage),
}

pub fn update(_state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::TitleBar(tb) => match tb {
            TitleBarMessage::Close => for_window(window::close),
            // minimize recebe (Id, bool); o resto recebe só o Id.
            TitleBarMessage::Minimize => for_window(|id| window::minimize(id, true)),
            TitleBarMessage::Maximize => for_window(window::toggle_maximize),
            TitleBarMessage::Drag => for_window(window::drag),
        },
    }
}

/// Roda um window action na janela atual. `window::latest()` devolve
/// `Task<Option<Id>>`; resolvemos o Id e aplicamos a ação.
fn for_window(action: fn(window::Id) -> Task<Message>) -> Task<Message> {
    window::latest().then(move |maybe_id| match maybe_id {
        Some(id) => action(id),
        None => Task::none(),
    })
}

/// Theme Golden Hour (iced) — vem do `libluma`, resolvido dos tokens.
pub fn theme(_state: &State) -> Theme {
    libluma::golden_hour_theme()
}

pub fn view(state: &State) -> Element<'_, Message> {
    let gh = state.gh;

    // Barra de título custom no topo (mensagens mapeadas pras do app).
    let title = libluma::title_bar("Configurações").map(Message::TitleBar);

    // Corpo do M1 — heading + sub + linha de CategoryTiles.
    let heading = text("Configurações do Luma").size(24).color(gh.cream);
    let sub = text("Esqueleto iced (M2) — moldura custom, tokens e tiles.")
        .size(13)
        .color(gh.honey_dim);

    let tiles = row(vec![
        libluma::category_tile::<Message>("Tema e cores", "", gh.amber, true),
        libluma::category_tile::<Message>("Acessibilidade", "", gh.sage, true),
        libluma::category_tile::<Message>("Privacidade", "", gh.coral, true),
        libluma::category_tile::<Message>("Barra de tarefas", "", gh.warm_neutral, false),
    ])
    .spacing(12);

    let body_content = column(vec![heading.into(), sub.into(), tiles.into()]).spacing(24);
    let body = container(body_content)
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill);

    column(vec![title, body.into()])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
