//! `luma-menu` — o menu/launcher do Luma como **cliente layer-shell overlay**
//! próprio (igual a luma-bar virou a barra). Passo 1 (mínimo): abre uma
//! surface `Overlay` sob demanda (gatilho `menu_toggle` via D-Bus), com
//! backdrop escuro + saudação + busca (esqueleto), e fecha no clique fora.
//! Sem grade de apps (passo 2), sessão (3) nem teclado/busca real (4).

mod apps;
mod ipc;

use iced::widget::{
    button, column, container, image, mouse_area, row, scrollable, stack, svg, text, Space,
};
use iced::{Background, Border, Color, Element, Length, Padding, Task};
use iced_layershell::reexport::{
    Anchor, IcedId, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

/// Qual painel a coluna esquerda mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    /// Início: mais usados + recentes.
    Home,
    /// "Todos os programas": a lista cheia, rolável.
    All,
}

struct Menu {
    /// Id da surface overlay aberta, ou `None` (menu fechado).
    overlay: Option<IcedId>,
    /// Saudação ("Boa noite, user"), recalculada a cada abertura.
    greeting: String,
    /// Todos os apps descobertos (alfabético).
    apps: Vec<apps::App>,
    /// Favoritos pra "Mais usados".
    favorites: Vec<apps::App>,
    /// Painel atual da coluna esquerda.
    pane: Pane,
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
    /// Clica num app → roda o Exec desanexado e fecha o menu.
    LaunchApp(String),
    /// Abre um caminho no gerenciador de arquivos e fecha o menu.
    OpenPath(String),
    /// Abre o luma-config e fecha o menu.
    OpenSettings,
    /// Alterna a coluna esquerda entre Início e "Todos os programas".
    SetPane(Pane),
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
    // Descobre os apps uma vez no boot (o menu é daemon persistente).
    let all = apps::load();
    let favorites = apps::favorites(&all, 6);
    (
        Menu {
            overlay: None,
            greeting: greeting(),
            apps: all,
            favorites,
            pane: Pane::Home,
        },
        read,
    )
}

/// Fecha o menu (remove a surface), se aberto.
fn close(state: &mut Menu) -> Task<Message> {
    if let Some(id) = state.overlay.take() {
        return Task::done(Message::RemoveWindow(id));
    }
    Task::none()
}

fn update(state: &mut Menu, message: Message) -> Task<Message> {
    match message {
        Message::Toggle => {
            if let Some(id) = state.overlay.take() {
                return Task::done(Message::RemoveWindow(id));
            }
            state.greeting = greeting();
            state.pane = Pane::Home; // sempre reabre no Início
            let (id, task) = Message::layershell_open(overlay_settings());
            state.overlay = Some(id);
            return task;
        }
        Message::Close => return close(state),
        Message::Noop => {}
        Message::LaunchApp(exec) => {
            apps::launch(&exec);
            return close(state);
        }
        Message::OpenPath(path) => {
            apps::open_path(&path);
            return close(state);
        }
        Message::OpenSettings => {
            apps::launch_config();
            return close(state);
        }
        Message::SetPane(pane) => state.pane = pane,
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

/// Fonte display (Fraunces), pra saudação e títulos.
fn display_font(weight: iced::font::Weight) -> iced::Font {
    iced::Font {
        family: iced::font::Family::Name(libluma::tokens::FONT_DISPLAY),
        weight,
        ..iced::Font::DEFAULT
    }
}

/// Inicial do usuário, pro avatar.
fn user_initial() -> String {
    std::env::var("USER")
        .ok()
        .and_then(|u| u.chars().next())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// Handle do SVG de um ícone dos assets (tingido no uso).
fn icon_handle(name: &str) -> svg::Handle {
    let bytes: &'static [u8] = match name {
        "file-executable" => {
            include_bytes!("../../../assets/branding/icons/file-executable.svg")
        }
        "file-text" => include_bytes!("../../../assets/branding/icons/file-text.svg"),
        "file-image" => include_bytes!("../../../assets/branding/icons/file-image.svg"),
        "file-folder" => include_bytes!("../../../assets/branding/icons/file-folder.svg"),
        "places-computer" => {
            include_bytes!("../../../assets/branding/icons/places-computer.svg")
        }
        "ctx-settings" => include_bytes!("../../../assets/branding/icons/icon-ctx-settings.svg"),
        "desligar" => include_bytes!("../../../assets/branding/icons/icon-desligar.svg"),
        "reiniciar" => include_bytes!("../../../assets/branding/icons/icon-reiniciar.svg"),
        "suspender" => include_bytes!("../../../assets/branding/icons/icon-suspender.svg"),
        "bloquear" => include_bytes!("../../../assets/branding/icons/icon-bloquear.svg"),
        _ => include_bytes!("../../../assets/branding/icons/file-generic.svg"),
    };
    svg::Handle::from_memory(bytes)
}

/// Ícone SVG tingido, tamanho fixo.
fn svg_icon<'a>(name: &str, size: f32, color: Color) -> Element<'a, Message> {
    svg(icon_handle(name))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

/// Rótulo de seção ("Mais usados", "Recentes") — pequeno, secundário.
fn section_label<'a>(s: impl Into<String>, color: Color) -> Element<'a, Message> {
    container(text(s.into()).size(11.0).color(color))
        .padding(Padding::from([6.0, 10.0]))
        .into()
}

/// Linha de texto apagada (estado vazio: "Nenhum recente").
fn muted_line<'a>(s: impl Into<String>, color: Color) -> Element<'a, Message> {
    container(text(s.into()).size(13.0).color(color))
        .padding(Padding::from([6.0, 12.0]))
        .into()
}

/// Divisória horizontal sutil.
fn divider<'a>(color: Color) -> Element<'a, Message> {
    container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
        .padding(Padding::from([2.0, 8.0]))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(Color { a: 0.18, ..color })),
            ..Default::default()
        })
        .into()
}

/// Divisória vertical entre as duas colunas.
fn vdivider<'a>(color: Color) -> Element<'a, Message> {
    container(Space::new().width(Length::Fixed(1.0)).height(Length::Fill))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(Color { a: 0.15, ..color })),
            ..Default::default()
        })
        .into()
}

/// Linha clicável genérica (conteúdo arbitrário) com hover no acento.
fn clickable_row<'a>(content: Element<'a, Message>, msg: Message) -> Element<'a, Message> {
    button(content)
        .on_press(msg)
        .width(Length::Fill)
        .padding(Padding::from([7.0, 10.0]))
        .style(|_theme, status| {
            let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hover.then(|| {
                    Background::Color(Color {
                        a: 0.12,
                        ..libluma::tokens::accent()
                    })
                }),
                text_color: libluma::tokens::to_iced(libluma::tokens::semantic_default().text),
                border: Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

/// Botão de linha (ícone de asset + rótulo) — lugares, "Todos os programas".
fn row_button<'a>(name: &str, label: impl Into<String>, color: Color, msg: Message) -> Element<'a, Message> {
    let content = row![svg_icon(name, 20.0, color), text(label.into()).size(14.0).color(color)]
        .spacing(10.0)
        .align_y(iced::Alignment::Center)
        .into();
    clickable_row(content, msg)
}

/// Ícone real de um app (SVG ou raster), com fallback num quadrado + inicial.
fn app_icon_el<'a>(app: &apps::App, size: f32) -> Element<'a, Message> {
    match &app.icon_path {
        Some(apps::IconKind::Svg(p)) => svg(svg::Handle::from_path(p.clone()))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .into(),
        Some(apps::IconKind::Raster(p)) => image(image::Handle::from_path(p.clone()))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .into(),
        None => fallback_icon(&app.name, size),
    }
}

/// Quadradinho de acento com a inicial do app (ícone ausente).
fn fallback_icon<'a>(name: &str, size: f32) -> Element<'a, Message> {
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    let accent = libluma::tokens::accent();
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    container(
        text(initial)
            .size(size * 0.6)
            .color(cream)
            .font(display_font(iced::font::Weight::Semibold)),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(Color { a: 0.30, ..accent })),
        border: Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

/// Botão de um app real: ícone + nome, clique lança o Exec.
fn app_button<'a>(app: &apps::App, color: Color) -> Element<'a, Message> {
    let content = row![app_icon_el(app, 22.0), text(app.name.clone()).size(14.0).color(color)]
        .spacing(10.0)
        .align_y(iced::Alignment::Center)
        .into();
    clickable_row(content, Message::LaunchApp(app.exec.clone()))
}

/// Botão só-ícone (sessão secundária / reiniciar, suspender, bloquear).
fn icon_button<'a>(name: &str, color: Color) -> Element<'a, Message> {
    button(container(svg_icon(name, 18.0, color)).center_x(Length::Fill).center_y(Length::Fill))
        .on_press(Message::Noop)
        .width(Length::Fixed(34.0))
        .height(Length::Fixed(34.0))
        .padding(0)
        .style(|_theme, status| {
            let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hover.then(|| {
                    Background::Color(Color {
                        a: 0.12,
                        ..libluma::tokens::accent()
                    })
                }),
                border: Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

fn view(state: &Menu, _id: iced::window::Id) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    // As duas colunas ficam no MESMO fundo ESCURO (`background`) — é onde o
    // creme tem o contraste alto que a direita já tinha. A esquerda antes
    // ficava no `surface` (mais claro), que tirava contraste do texto. A
    // separação das colunas vem da divisória vertical. A11y > duas tonalidades.
    let panel_bg = libluma::tokens::to_iced(sem.background);
    let field_bg = libluma::tokens::to_iced(sem.surface);
    let cream = libluma::tokens::to_iced(sem.text);
    let secondary = libluma::tokens::to_iced(sem.text_secondary);
    let accent = libluma::tokens::accent();

    // Backdrop escuro full-screen — clique fora fecha.
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill)).style(|_theme| {
            container::Style {
                background: Some(Background::Color(Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.5,
                })),
                ..Default::default()
            }
        }),
    )
    .on_press(Message::Close);

    // ─── Coluna esquerda: programas (placeholder) + busca no rodapé ────
    // Busca (a marca do W7) vive no rodapé da coluna esquerda. Esqueleto.
    let search = container(text("Buscar programas e arquivos…").size(13.0).color(secondary))
        .padding(Padding::from([10.0, 14.0]))
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(field_bg)),
            border: Border {
                radius: 10.0.into(),
                width: 1.5,
                color: accent,
            },
            ..Default::default()
        });

    let mut left_items: Vec<Element<Message>> = Vec::new();
    match state.pane {
        Pane::Home => {
            left_items.push(section_label("Mais usados", secondary));
            if state.favorites.is_empty() {
                left_items.push(muted_line("Nenhum app encontrado", secondary));
            } else {
                for app in &state.favorites {
                    left_items.push(app_button(app, cream));
                }
            }
            left_items.push(section_label("Recentes", secondary));
            left_items.push(muted_line("Nenhum item recente", secondary));
            left_items.push(Space::new().height(Length::Fill).into());
            left_items.push(divider(secondary));
            left_items.push(row_button(
                "file-folder",
                "Todos os programas",
                cream,
                Message::SetPane(Pane::All),
            ));
        }
        Pane::All => {
            left_items.push(row_button(
                "file-folder",
                "‹ Voltar",
                cream,
                Message::SetPane(Pane::Home),
            ));
            left_items.push(section_label(
                format!("Todos os programas ({})", state.apps.len()),
                secondary,
            ));
            let list = iced::widget::column(state.apps.iter().map(|a| app_button(a, cream)))
                .spacing(2.0);
            left_items.push(scrollable(list).height(Length::Fill).into());
        }
    }
    left_items.push(Space::new().height(Length::Fixed(6.0)).into());
    left_items.push(search.into());

    let left = container(iced::widget::column(left_items).spacing(2.0))
        .width(Length::Fixed(338.0))
        .height(Length::Fill)
        .padding(12);

    // ─── Coluna direita: lugares + sessão ──────────────────────────────
    let avatar = container(
        text(user_initial())
            .size(22.0)
            .color(panel_bg)
            .font(display_font(iced::font::Weight::Bold)),
    )
    .width(Length::Fixed(52.0))
    .height(Length::Fixed(52.0))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(accent)),
        border: Border {
            radius: 26.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });

    let header = column![
        avatar,
        text(state.greeting.clone())
            .size(15.0)
            .color(cream)
            .font(display_font(iced::font::Weight::Semibold)),
    ]
    .spacing(10.0)
    .align_x(iced::Alignment::Center)
    .width(Length::Fill);

    // Sessão (placeholder; wiring no passo 3): Desligar + reiniciar/suspender/bloquear.
    let session = row![
        row_button("desligar", "Desligar", cream, Message::Noop),
        icon_button("reiniciar", secondary),
        icon_button("suspender", secondary),
        icon_button("bloquear", secondary),
    ]
    .spacing(4.0)
    .align_y(iced::Alignment::Center);

    let right = container(
        column![
            header,
            divider(secondary),
            row_button(
                "places-computer",
                "Pasta pessoal",
                cream,
                Message::OpenPath(apps::home_dir())
            ),
            row_button(
                "file-folder",
                "Documentos",
                cream,
                Message::OpenPath(apps::documents_dir())
            ),
            row_button(
                "file-image",
                "Imagens",
                cream,
                Message::OpenPath(apps::pictures_dir())
            ),
            row_button(
                "ctx-settings",
                "Configurações do Luma",
                cream,
                Message::OpenSettings
            ),
            Space::new().height(Length::Fill),
            divider(secondary),
            session,
        ]
        .spacing(4.0),
    )
    .width(Length::Fixed(230.0))
    .height(Length::Fill)
    .padding(14);

    // Painel: duas colunas no mesmo fundo escuro, divisória vertical entre
    // elas, cantos arredondados + borda sutil pra a carta destacar do backdrop.
    let panel = mouse_area(
        container(row![left, vdivider(secondary), right].spacing(0))
            .width(Length::Fixed(580.0))
            .height(Length::Fixed(560.0))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(panel_bg)),
                border: Border {
                    radius: 16.0.into(),
                    width: 1.0,
                    color: Color { a: 0.22, ..cream },
                },
                ..Default::default()
            }),
    )
    .on_press(Message::Noop);

    // Ancora no canto SUPERIOR-esquerdo, descendo colado embaixo da luma-bar
    // (que mora no topo). A surface já nasce abaixo da zona exclusiva da
    // barra, então a margem de topo é só um respiro. Assim a sessão (no fim
    // do painel) cabe inteira, sem bater nas barras velhas de baixo.
    stack![
        backdrop,
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Top)
            .padding(Padding {
                top: 8.0,
                right: 0.0,
                bottom: 0.0,
                left: 12.0,
            }),
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
