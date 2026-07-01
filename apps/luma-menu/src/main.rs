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

/// Ação de item do context menu do desktop (só as diretas no núcleo; as que
/// tocam o compositor — organizar/colar/wallpaper/criar — vêm no commit 2).
#[derive(Debug, Clone, Copy)]
enum CtxAction {
    Terminal,
    AddWidget,
    Customize,
    Settings,
}

/// Context menu do desktop aberto: a surface + o ponto do clique + o tamanho
/// do output (pra proteção de borda).
#[derive(Debug, Clone, Copy)]
struct CtxMenu {
    id: IcedId,
    x: i32,
    y: i32,
    out_w: i32,
    out_h: i32,
}

/// Ação irreversível esperando confirmação (desligar/reiniciar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confirm {
    PowerOff,
    Reboot,
}

impl Confirm {
    fn title(self) -> &'static str {
        match self {
            Confirm::PowerOff => "Desligar agora?",
            Confirm::Reboot => "Reiniciar agora?",
        }
    }
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
    /// Texto digitado na busca. Vazio = mostra Início/Todos; senão filtra.
    search: String,
    /// Confirmação pendente (desligar/reiniciar), ou None.
    confirm: Option<Confirm>,
    /// Context menu do desktop aberto (surface própria), ou None.
    ctx: Option<CtxMenu>,
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
    /// Tecla pressionada na busca (teclado roteado pra layer surface).
    Key(iced::keyboard::Key),
    /// Sessão: suspende / bloqueia (reversíveis, direto).
    Suspend,
    Lock,
    /// Pede confirmação de uma ação irreversível (desligar/reiniciar).
    AskConfirm(Confirm),
    /// Confirma / cancela a ação pendente.
    DoConfirm,
    CancelConfirm,
    /// Sinal do compositor: abre o context menu do desktop no ponto.
    DesktopMenu(i32, i32, i32, i32),
    /// Fecha o context menu do desktop.
    CloseCtx,
    /// Clique num item do context menu → ação + fecha.
    CtxRun(CtxAction),
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
        // Exclusive: o menu pega o foco de teclado ao abrir (busca + Esc);
        // o compositor devolve pra janela ativa quando fecha. Gotcha #4.
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
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
            search: String::new(),
            confirm: None,
            ctx: None,
        },
        read,
    )
}

/// Settings da surface do context menu: full-screen overlay (backdrop pra o
/// clique-fora fechar); o card é posicionado no ponto do clique no view.
fn ctx_settings() -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: None,
        layer: Layer::Overlay,
        anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
        exclusive_zone: Some(0),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::LastOutput,
        events_transparent: false,
        namespace: Some("luma-ctxmenu".to_string()),
    }
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
            state.search.clear(); // busca limpa a cada abertura
            state.confirm = None; // sem confirmação pendente
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
        Message::Suspend => {
            return Task::batch([Task::perform(ipc::suspend(), |_| Message::Noop), close(state)]);
        }
        Message::Lock => {
            return Task::batch([Task::perform(ipc::lock(), |_| Message::Noop), close(state)]);
        }
        Message::AskConfirm(c) => state.confirm = Some(c),
        Message::CancelConfirm => state.confirm = None,
        Message::DesktopMenu(x, y, out_w, out_h) => {
            // Fecha um ctx anterior (se aberto) e abre no novo ponto.
            let close = if let Some(c) = state.ctx.take() {
                Task::done(Message::RemoveWindow(c.id))
            } else {
                Task::none()
            };
            let (id, open) = Message::layershell_open(ctx_settings());
            state.ctx = Some(CtxMenu {
                id,
                x,
                y,
                out_w,
                out_h,
            });
            return Task::batch([close, open]);
        }
        Message::CloseCtx => {
            if let Some(c) = state.ctx.take() {
                return Task::batch([
                    Task::perform(ipc::desktop_menu_closed(), |_| Message::Noop),
                    Task::done(Message::RemoveWindow(c.id)),
                ]);
            }
        }
        Message::CtxRun(action) => {
            match action {
                CtxAction::Terminal => apps::open_terminal(),
                CtxAction::AddWidget => {
                    apps::launch_config_args(&["--tab", "widgets", "--target", "desktop"])
                }
                CtxAction::Customize => apps::launch_config_args(&["--tab", "widgets"]),
                CtxAction::Settings => apps::launch_config(),
            }
            if let Some(c) = state.ctx.take() {
                return Task::batch([
                    Task::perform(ipc::desktop_menu_closed(), |_| Message::Noop),
                    Task::done(Message::RemoveWindow(c.id)),
                ]);
            }
        }
        Message::DoConfirm => {
            let action = match state.confirm.take() {
                Some(Confirm::PowerOff) => Task::perform(ipc::power_off(), |_| Message::Noop),
                Some(Confirm::Reboot) => Task::perform(ipc::reboot(), |_| Message::Noop),
                None => return Task::none(),
            };
            return Task::batch([action, close(state)]);
        }
        Message::Key(key) => {
            use iced::keyboard::key::Named;
            use iced::keyboard::Key;
            match key {
                Key::Named(Named::Escape) => {
                    if state.confirm.is_some() {
                        state.confirm = None; // Esc cancela a confirmação
                    } else {
                        return close(state);
                    }
                }
                Key::Named(Named::Backspace) => {
                    state.search.pop();
                }
                Key::Named(Named::Enter) => {
                    // Enter lança o primeiro resultado da busca.
                    if !state.search.is_empty() {
                        if let Some(app) = filtered_apps(state).into_iter().next() {
                            let exec = app.exec.clone();
                            apps::launch(&exec);
                            return close(state);
                        }
                    }
                }
                Key::Named(Named::Space) => state.search.push(' '),
                Key::Character(s) => state.search.push_str(s.as_str()),
                _ => {}
            }
        }
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
        "ctx-new" => include_bytes!("../../../assets/branding/icons/icon-ctx-new.svg"),
        "ctx-paste" => include_bytes!("../../../assets/branding/icons/icon-ctx-paste.svg"),
        "ctx-arrange" => include_bytes!("../../../assets/branding/icons/icon-ctx-arrange.svg"),
        "ctx-terminal" => include_bytes!("../../../assets/branding/icons/icon-ctx-terminal.svg"),
        "ctx-wallpaper" => include_bytes!("../../../assets/branding/icons/icon-ctx-wallpaper.svg"),
        "ctx-display" => include_bytes!("../../../assets/branding/icons/icon-ctx-display.svg"),
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

/// Ícone SVG SEM tint (cores originais) — pros tiles coloridos de sessão
/// (desligar coral, reiniciar âmbar, suspender sage, bloquear mel), que são
/// desenhados com cor própria e viram um quadrado liso se tingidos.
fn svg_icon_raw<'a>(name: &str, size: f32) -> Element<'a, Message> {
    svg(icon_handle(name))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
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

/// Vista de confirmação (desligar/reiniciar): card central com título +
/// Confirmar/Cancelar. Backdrop e Esc cancelam. Protege os irreversíveis.
fn confirm_view<'a>(confirm: Confirm) -> Element<'a, Message> {
    let sem = libluma::tokens::semantic_default();
    let surface = libluma::tokens::to_iced(sem.surface);
    let cream = libluma::tokens::to_iced(sem.text);
    let bg = libluma::tokens::to_iced(sem.background);
    let accent = libluma::tokens::accent();

    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill)).style(|_theme| {
            container::Style {
                background: None,
                ..Default::default()
            }
        }),
    )
    .on_press(Message::CancelConfirm);

    // Botões: Confirmar (no acento) + Cancelar (neutro).
    let confirm_btn = mouse_area(
        container(text("Confirmar").size(14.0).color(bg))
            .padding(Padding::from([9.0, 18.0]))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(accent)),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_press(Message::DoConfirm)
    .interaction(iced::mouse::Interaction::Pointer);
    let cancel_btn = mouse_area(
        container(text("Cancelar").size(14.0).color(cream))
            .padding(Padding::from([9.0, 18.0]))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(Color { a: 0.10, ..cream })),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_press(Message::CancelConfirm)
    .interaction(iced::mouse::Interaction::Pointer);

    let card = mouse_area(
        container(
            column![
                text(confirm.title())
                    .size(20.0)
                    .color(cream)
                    .font(display_font(iced::font::Weight::Semibold)),
                row![cancel_btn, confirm_btn].spacing(12.0),
            ]
            .spacing(22.0)
            .align_x(iced::Alignment::Center),
        )
        .padding(28)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                radius: 18.0.into(),
                width: 1.0,
                color: Color { a: 0.22, ..cream },
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

/// Botão só-ícone de sessão (reiniciar/suspender/bloquear): tile colorido SEM
/// tint. `_color` não é usado (o tile tem cor própria), mantido pra simetria.
fn icon_button<'a>(name: &str, _color: Color, msg: Message) -> Element<'a, Message> {
    button(container(svg_icon_raw(name, 22.0)).center_x(Length::Fill).center_y(Length::Fill))
        .on_press(msg)
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

/// Um item do context menu: ícone + label + (atalho/seta). `action` None =
/// inerte/desabilitado (apagado, sem clique) — os submenus e o que toca o
/// compositor entram no commit 2.
fn ctx_item<'a>(
    icon: &str,
    label: &'a str,
    trailing: &'a str,
    action: Option<CtxAction>,
) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    let secondary =
        libluma::tokens::to_iced(libluma::tokens::semantic_default().text_secondary);
    let color = if action.is_some() { cream } else { secondary };
    let content = row![
        svg_icon(icon, 16.0, color),
        text(label).size(13.0).color(color),
        Space::new().width(Length::Fill),
        text(trailing).size(11.0).color(secondary),
    ]
    .spacing(10.0)
    .align_y(iced::Alignment::Center);
    let inner = container(content)
        .width(Length::Fill)
        .padding(Padding::from([6.0, 12.0]));
    match action {
        Some(a) => mouse_area(inner)
            .on_press(Message::CtxRun(a))
            .interaction(iced::mouse::Interaction::Pointer)
            .into(),
        None => inner.into(),
    }
}

/// Divisória do context menu.
fn ctx_sep<'a>() -> Element<'a, Message> {
    let secondary =
        libluma::tokens::to_iced(libluma::tokens::semantic_default().text_secondary);
    container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
        .padding(Padding::from([3.0, 10.0]))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(Color { a: 0.18, ..secondary })),
            ..Default::default()
        })
        .into()
}

/// Context menu do desktop, no ponto do clique, com proteção de borda. Cara
/// de sempre: card surface + linha de acento no topo + itens + separadores.
fn ctx_view(c: &CtxMenu) -> Element<'static, Message> {
    let sem = libluma::tokens::semantic_default();
    let surface = libluma::tokens::to_iced(sem.surface);
    let cream = libluma::tokens::to_iced(sem.text);
    let accent = libluma::tokens::accent();

    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill)).style(|_theme| {
            container::Style {
                background: None,
                ..Default::default()
            }
        }),
    )
    .on_press(Message::CloseCtx);

    let accent_line = container(Space::new().width(Length::Fill).height(Length::Fixed(2.0)))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(accent)),
            ..Default::default()
        });

    let items = column![
        ctx_item("ctx-new", "Criar novo", "▸", None),
        ctx_item("ctx-paste", "Colar", "Ctrl+V", None),
        ctx_item("ctx-arrange", "Organizar ícones", "▸", None),
        ctx_sep(),
        ctx_item("ctx-terminal", "Abrir terminal aqui", "", Some(CtxAction::Terminal)),
        ctx_item("ctx-wallpaper", "Mudar papel de parede", "▸", None),
        ctx_sep(),
        ctx_item("ctx-new", "Adicionar widget ao desktop", "", Some(CtxAction::AddWidget)),
        ctx_item("ctx-settings", "Personalizar desktop", "", Some(CtxAction::Customize)),
        ctx_sep(),
        ctx_item("ctx-display", "Configurar tela", "", None),
        ctx_item("ctx-settings", "Configurações do Luma", "", Some(CtxAction::Settings)),
    ]
    .spacing(1.0);

    let card = mouse_area(
        container(column![accent_line, items].spacing(6.0))
            .width(Length::Fixed(260.0))
            .padding(Padding::from([6.0, 0.0]))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(surface)),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: Color { a: 0.22, ..cream },
                },
                ..Default::default()
            }),
    )
    .on_press(Message::Noop);

    // Proteção de borda: abre pra cima/esquerda se não couber.
    let menu_w = 260;
    let menu_h = 345; // estimativa (9 itens + 3 separadores + acento + pad)
    let px = if c.x + menu_w > c.out_w {
        (c.x - menu_w).max(0)
    } else {
        c.x
    };
    let py = if c.y + menu_h > c.out_h {
        (c.y - menu_h).max(0)
    } else {
        c.y
    };

    stack![
        backdrop,
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Top)
            .padding(Padding {
                top: py as f32,
                right: 0.0,
                bottom: 0.0,
                left: px as f32,
            }),
    ]
    .into()
}

fn view(state: &Menu, id: iced::window::Id) -> Element<'_, Message> {
    // Context menu do desktop tem surface própria (abre no ponto do clique).
    if let Some(c) = state.ctx {
        if c.id == id {
            return ctx_view(&c);
        }
    }
    // Confirmação pendente (desligar/reiniciar) toma a tela — modal.
    if let Some(confirm) = state.confirm {
        return confirm_view(confirm);
    }
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

    // Backdrop TRANSPARENTE full-screen — captura o clique-fora (fecha) sem
    // escurecer o fundo.
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill)).style(|_theme| {
            container::Style {
                background: None,
                ..Default::default()
            }
        }),
    )
    .on_press(Message::Close);

    // ─── Coluna esquerda: programas + busca no rodapé ──────────────────
    // Busca (a marca do W7) no rodapé. Mostra o texto digitado (teclado
    // roteado, gotcha #4) ou o placeholder.
    let (search_text, search_color) = if state.search.is_empty() {
        ("Buscar programas e arquivos…".to_string(), secondary)
    } else {
        (state.search.clone(), cream)
    };
    let search = container(text(search_text).size(13.0).color(search_color))
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
    if !state.search.is_empty() {
        // Busca ativa: resultados filtrados ao vivo.
        let results = filtered_apps(state);
        left_items.push(section_label(
            format!("Resultados ({})", results.len()),
            secondary,
        ));
        if results.is_empty() {
            left_items.push(muted_line("Nada encontrado", secondary));
        } else {
            let list = iced::widget::column(results.into_iter().map(|a| app_button(a, cream)))
                .spacing(2.0);
            left_items.push(scrollable(list).height(Length::Fill).into());
        }
    } else {
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
    // Sessão em duas linhas (a coluna é estreita): Desligar em cima (largura
    // cheia), reiniciar/suspender/bloquear embaixo.
    let session = column![
        clickable_row(
            row![
                svg_icon_raw("desligar", 22.0),
                text("Desligar").size(14.0).color(cream)
            ]
            .spacing(10.0)
            .align_y(iced::Alignment::Center)
            .into(),
            Message::AskConfirm(Confirm::PowerOff),
        ),
        row![
            icon_button("reiniciar", secondary, Message::AskConfirm(Confirm::Reboot)),
            icon_button("suspender", secondary, Message::Suspend),
            icon_button("bloquear", secondary, Message::Lock),
        ]
        .spacing(6.0)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(6.0);

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
        iced::Subscription::run(desktop_menu_subscription),
        iced::Subscription::run(accent_subscription),
        iced::Subscription::run(high_contrast_subscription),
        // Teclado (gotcha #4): quando o overlay tem foco, as teclas chegam.
        iced::event::listen_with(|event, _status, _window| {
            if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event {
                Some(Message::Key(key))
            } else {
                None
            }
        }),
    ])
}

/// Apps que casam com a busca (case-insensitive no nome).
fn filtered_apps(state: &Menu) -> Vec<&apps::App> {
    let q = state.search.to_lowercase();
    state
        .apps
        .iter()
        .filter(|a| a.name.to_lowercase().contains(&q))
        .collect()
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

fn desktop_menu_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::desktop_menu_stream().await {
            Ok(s) => s.map(|(x, y, w, h)| Message::DesktopMenu(x, y, w, h)).boxed(),
            Err(e) => {
                eprintln!("luma-menu: subscription de desktop_menu falhou: {e}");
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
        // Surface transparente: o "clear color" padrão (fundo escuro do tema)
        // pintava a tela toda. Assim só o card pinta; o resto deixa o
        // wallpaper passar (sem dim).
        .style(|_state: &Menu, theme: &iced::Theme| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        })
        .subscription(subscription)
        .run()
}
