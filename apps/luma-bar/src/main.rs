//! `luma-bar` — a barra do Luma como **cliente layer-shell** próprio (igual o
//! `luma-config`), no modelo limpo: libluma (Golden Hour) + o canal D-Bus, daí
//! acento/HC de graça. Passo 1 (mínimo): ancora na borda com zona exclusiva,
//! renderiza a moldura + relógio, e segue o acento/HC ao vivo. Sem taskbar/
//! applets ainda; coexiste com a barra velha do compositor pra comparar.

mod ipc;

use std::time::Duration;

use iced::widget::{column, container, row, text, Space};
use iced::{Background, Border, Element, Length, Task};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::to_layer_message;

/// Altura do conteúdo da barra (px).
const BAR_HEIGHT: u32 = 36;
/// Espessura da linha de acento na base da barra.
const ACCENT_LINE: u32 = 2;

struct Bar {
    /// Hora atual "HH:MM" — o conteúdo simples que prova render + update.
    clock: String,
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    /// Tick de 1s — atualiza o relógio.
    Tick,
    /// Acento lido no boot / mudado pelo sinal — re-resolve o token ao vivo.
    AccentLoaded(Result<String, String>),
    AccentChanged(String),
    /// Alto contraste lido no boot / mudado pelo sinal.
    HighContrastLoaded(Result<bool, String>),
    HighContrastChanged(bool),
}

fn now_hm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

fn boot() -> (Bar, Task<Message>) {
    // Lê acento + HC no boot pra a barra abrir já no estado salvo.
    let read = Task::batch([
        Task::perform(ipc::read_accent(), Message::AccentLoaded),
        Task::perform(ipc::read_high_contrast(), Message::HighContrastLoaded),
    ]);
    (Bar { clock: now_hm() }, read)
}

fn update(state: &mut Bar, message: Message) -> Task<Message> {
    match message {
        Message::Tick => state.clock = now_hm(),
        // Acento/HC: giram os flags globais do libluma; o próximo render
        // re-resolve `accent()`/cores. Fonte única = boot + sinal.
        Message::AccentLoaded(Ok(id)) => libluma::tokens::set_accent_id(&id),
        Message::AccentLoaded(Err(e)) => eprintln!("luma-bar: ler acento falhou: {e}"),
        Message::AccentChanged(id) => libluma::tokens::set_accent_id(&id),
        Message::HighContrastLoaded(Ok(v)) => libluma::tokens::set_high_contrast(v),
        Message::HighContrastLoaded(Err(e)) => {
            eprintln!("luma-bar: ler alto contraste falhou: {e}")
        }
        Message::HighContrastChanged(v) => libluma::tokens::set_high_contrast(v),
        _ => {}
    }
    Task::none()
}

fn view(state: &Bar) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    let bar_bg = libluma::tokens::to_iced(sem.surface);
    let accent = libluma::tokens::accent();

    // Conteúdo: dot de acento + relógio à direita. O dot e a linha de base
    // seguem o acento — a personalização aparece na hora.
    let dot = container(Space::new().width(Length::Fixed(7.0)).height(Length::Fixed(7.0))).style(move |_| {
        container::Style {
            background: Some(Background::Color(accent)),
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    });
    let content = container(
        row![
            Space::new().width(Length::Fill),
            dot,
            text(state.clock.clone()).size(15.0).color(accent),
            Space::new().width(Length::Fixed(16.0)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(iced::alignment::Vertical::Center)
    .style(move |_| container::Style {
        background: Some(Background::Color(bar_bg)),
        ..Default::default()
    });

    // Linha de acento na base (a moldura Golden Hour da barra).
    let accent_line = container(Space::new().width(Length::Fill).height(Length::Fixed(ACCENT_LINE as f32)))
        .style(move |_| container::Style {
            background: Some(Background::Color(accent)),
            ..Default::default()
        });

    column![content, accent_line]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn subscription(_state: &Bar) -> iced::Subscription<Message> {
    iced::Subscription::batch([
        iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick),
        iced::Subscription::run(accent_subscription),
        iced::Subscription::run(high_contrast_subscription),
    ])
}

fn accent_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::accent_signal_stream().await {
            Ok(s) => s.map(Message::AccentChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de acento falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn high_contrast_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::high_contrast_signal_stream().await {
            Ok(s) => s.map(Message::HighContrastChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de HC falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn main() -> Result<(), iced_layershell::Error> {
    let settings = Settings {
        layer_settings: LayerShellSettings {
            // Topo, full-width (âncora L+R), camada Top, com zona exclusiva =
            // altura → janelas não sobrepõem. Coexiste com a barra velha
            // (que fica embaixo), pra comparar lado a lado.
            anchor: Anchor::Top | Anchor::Left | Anchor::Right,
            layer: Layer::Top,
            exclusive_zone: (BAR_HEIGHT + ACCENT_LINE) as i32,
            size: Some((0, BAR_HEIGHT + ACCENT_LINE)),
            margin: (0, 0, 0, 0),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        },
        ..Default::default()
    };

    iced_layershell::application(boot, "luma-bar", update, view)
        .settings(settings)
        .theme(|_state: &Bar| libluma::golden_hour_theme())
        .subscription(subscription)
        .run()
}
