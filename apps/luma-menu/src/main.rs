//! `luma-menu` — o menu/launcher do Luma como **cliente layer-shell overlay**
//! próprio (igual a luma-bar virou a barra). Passo 1 (mínimo): abre uma
//! surface `Overlay` sob demanda (gatilho `menu_toggle` via D-Bus), com
//! backdrop escuro + saudação + busca (esqueleto), e fecha no clique fora.
//! Sem grade de apps (passo 2), sessão (3) nem teclado/busca real (4).

mod ipc;

use iced::widget::{column, container, mouse_area, stack, text, Space};
use iced::{Background, Border, Color, Element, Length, Task};
use iced_layershell::reexport::{
    Anchor, IcedId, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

struct Menu {
    /// Id da surface overlay aberta, ou `None` (menu fechado).
    overlay: Option<IcedId>,
    /// Saudação ("Boa noite, user"), recalculada a cada abertura.
    greeting: String,
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    /// Gatilho (sinal `menu_toggle`) → abre/fecha.
    Toggle,
    /// Fecha (clique no backdrop).
    Close,
    /// Captura clique no card (não fecha).
    Noop,
    AccentLoaded(Result<String, String>),
    AccentChanged(String),
    HighContrastLoaded(Result<bool, String>),
    HighContrastChanged(bool),
}

/// Saudação por hora do dia + usuário.
fn greeting() -> String {
    use chrono::Timelike;
    let h = chrono::Local::now().hour();
    let part = if h < 12 {
        "Bom dia"
    } else if h < 18 {
        "Boa tarde"
    } else {
        "Boa noite"
    };
    let user = std::env::var("USER").unwrap_or_else(|_| "você".to_string());
    format!("{part}, {user}")
}

/// Settings da surface overlay: tela cheia (âncora nos 4 lados), camada
/// Overlay (acima de tudo, inclusive a barra), sem zona exclusiva.
fn overlay_settings() -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: None,
        layer: Layer::Overlay,
        anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
        exclusive_zone: Some(0),
        margin: None,
        // Teclado fica pro passo 4 (precisa do roteamento de teclado no
        // compositor). Por ora fecha por clique fora (pointer já roteia).
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::LastOutput,
        events_transparent: false,
        namespace: Some("luma-menu".to_string()),
    }
}

fn boot() -> (Menu, Task<Message>) {
    // Lê acento/HC no boot pra o menu nascer no tema. Começa fechado
    // (StartMode::Background = sem surface até o gatilho).
    let read = Task::batch([
        Task::perform(ipc::read_accent(), Message::AccentLoaded),
        Task::perform(ipc::read_high_contrast(), Message::HighContrastLoaded),
    ]);
    (
        Menu {
            overlay: None,
            greeting: greeting(),
        },
        read,
    )
}

fn update(state: &mut Menu, message: Message) -> Task<Message> {
    match message {
        Message::Toggle => {
            if let Some(id) = state.overlay.take() {
                return Task::done(Message::RemoveWindow(id));
            }
            state.greeting = greeting();
            let (id, task) = Message::layershell_open(overlay_settings());
            state.overlay = Some(id);
            return task;
        }
        Message::Close => {
            if let Some(id) = state.overlay.take() {
                return Task::done(Message::RemoveWindow(id));
            }
        }
        Message::Noop => {}
        Message::AccentLoaded(Ok(id)) => libluma::tokens::set_accent_id(&id),
        Message::AccentLoaded(Err(e)) => eprintln!("luma-menu: ler acento falhou: {e}"),
        Message::AccentChanged(id) => libluma::tokens::set_accent_id(&id),
        Message::HighContrastLoaded(Ok(v)) => libluma::tokens::set_high_contrast(v),
        Message::HighContrastLoaded(Err(e)) => eprintln!("luma-menu: ler HC falhou: {e}"),
        Message::HighContrastChanged(v) => libluma::tokens::set_high_contrast(v),
        _ => {}
    }
    Task::none()
}

fn view(state: &Menu, _id: iced::window::Id) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    let surface = libluma::tokens::to_iced(sem.surface);
    let cream = libluma::tokens::to_iced(sem.text);
    let secondary = libluma::tokens::to_iced(sem.text_secondary);
    let accent = libluma::tokens::accent();
    let input_bg = libluma::tokens::to_iced(sem.background);

    // Backdrop escuro full-screen — clique fecha (clique fora do card).
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill)).style(|_theme| {
            container::Style {
                background: Some(Background::Color(Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.55,
                })),
                ..Default::default()
            }
        }),
    )
    .on_press(Message::Close);

    // Busca (esqueleto visual): caixa com placeholder, borda no acento.
    let search = container(text("Buscar…").size(15.0).color(secondary))
        .padding(iced::Padding::from([11.0, 16.0]))
        .width(Length::Fixed(440.0))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(input_bg)),
            border: Border {
                radius: 12.0.into(),
                width: 1.5,
                color: accent,
            },
            ..Default::default()
        });

    // Card: saudação (Fraunces) + busca. mouse_area(Noop) captura o clique
    // no card pra não vazar pro backdrop (não fecha ao clicar dentro).
    let card_inner = column![
        text(state.greeting.clone())
            .size(28.0)
            .color(cream)
            .font(iced::Font {
                family: iced::font::Family::Name(libluma::tokens::FONT_DISPLAY),
                weight: iced::font::Weight::Semibold,
                ..iced::Font::DEFAULT
            }),
        search,
    ]
    .spacing(20)
    .align_x(iced::Alignment::Center);
    let card = mouse_area(
        container(card_inner)
            .padding(30)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(surface)),
                border: Border {
                    radius: 20.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_press(Message::Noop);

    stack![
        backdrop,
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    ]
    .into()
}

fn subscription(_state: &Menu) -> iced::Subscription<Message> {
    iced::Subscription::batch([
        iced::Subscription::run(toggle_subscription),
        iced::Subscription::run(accent_subscription),
        iced::Subscription::run(high_contrast_subscription),
    ])
}

fn toggle_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::menu_toggle_stream().await {
            Ok(s) => s.map(|_| Message::Toggle).boxed(),
            Err(e) => {
                eprintln!("luma-menu: subscription de menu_toggle falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn accent_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::accent_signal_stream().await {
            Ok(s) => s.map(Message::AccentChanged).boxed(),
            Err(e) => {
                eprintln!("luma-menu: subscription de acento falhou: {e}");
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
                eprintln!("luma-menu: subscription de HC falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn main() -> Result<(), iced_layershell::Error> {
    // Começa em background (sem surface) — a overlay abre no gatilho. Fonte
    // Fraunces registrada pra a saudação no display.
    let settings = Settings {
        layer_settings: LayerShellSettings {
            start_mode: StartMode::Background,
            ..Default::default()
        },
        fonts: vec![libluma::tokens::DISPLAY_FONT_BYTES.into()],
        ..Default::default()
    };

    iced_layershell::daemon(boot, "luma-menu", update, view)
        .settings(settings)
        .theme(|_state: &Menu, _id| libluma::golden_hour_theme())
        .subscription(subscription)
        .run()
}
