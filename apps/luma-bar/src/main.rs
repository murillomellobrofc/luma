//! `luma-bar` — a barra do Luma como **cliente layer-shell** próprio (igual o
//! `luma-config`), no modelo limpo: libluma (Golden Hour) + o canal D-Bus, daí
//! acento/HC de graça. Passo 1 (mínimo): ancora na borda com zona exclusiva,
//! renderiza a moldura + relógio, e segue o acento/HC ao vivo. Sem taskbar/
//! applets ainda; coexiste com a barra velha do compositor pra comparar.

mod ipc;

use std::time::Duration;

use iced::widget::{button, column, container, mouse_area, row, svg, text, Space};
use iced::{Background, Border, Color, Element, Length, Task};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::to_layer_message;

/// Altura do conteúdo da barra (px).
const BAR_HEIGHT: u32 = 36;
/// Espessura da linha de acento na base da barra.
const ACCENT_LINE: u32 = 2;
/// Máximo de chars do título no botão (trunca com reticências).
const TITLE_MAX: usize = 22;

/// Uma janela na taskbar (passo 2): id pra ativar, label resolvido, foco.
#[derive(Debug, Clone)]
struct WinEntry {
    id: u64,
    label: String,
    focused: bool,
}

struct Bar {
    /// Hora atual "HH:MM".
    clock: String,
    /// Data curta pt-BR ("seg · 22 jun"). Atualiza junto no tick.
    date: String,
    /// Janelas abertas (poll de `list_windows` a cada 500ms).
    windows: Vec<WinEntry>,
    /// Bateria: `Some((capacity, charging))`, ou `None` se não há bateria.
    /// Lida no boot + atualizada pelo sinal (sem poll — passo 3).
    battery: Option<(u8, bool)>,
    /// Volume: `(level 0–100, muted)`. Lido no boot + sinal; mexido por
    /// scroll/clique com update otimista (passo 4).
    volume: (u8, bool),
    /// Rede: nome do ícone do estado ("ethernet-on", "wifi-full"…). Vazio
    /// até o boot ler. Lido no boot + sinal (passo 5).
    network: String,
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
    /// Hora de re-ler a lista de janelas (tick de 500ms).
    PollWindows,
    /// Lista de janelas lida do compositor.
    WindowsLoaded(Vec<(u64, String, String, bool)>),
    /// Clique num botão de janela → foca/levanta no compositor.
    ActivateWindow(u64),
    /// Bateria lida no boot / mudada pelo sinal: `(present, capacity, charging)`.
    BatteryLoaded((bool, u8, bool)),
    BatteryChanged((bool, u8, bool)),
    /// Volume lido no boot / mudado pelo sinal: `(level, muted)`.
    VolumeLoaded((u8, bool)),
    VolumeChanged((u8, bool)),
    /// Scroll no applet de volume → muda o nível (±5).
    VolumeScroll(iced::mouse::ScrollDelta),
    /// Clique no applet de volume → alterna mudo.
    ToggleMute,
    /// No-op (descarta o resultado de um set fire-and-forget).
    Noop,
    /// Rede lida no boot / mudada pelo sinal: nome do ícone.
    NetworkLoaded(String),
    NetworkChanged(String),
}

fn now_hm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Data curta pt-BR no estilo da barra velha: "seg · 22 jun".
fn now_date() -> String {
    use chrono::Datelike;
    let now = chrono::Local::now();
    format!(
        "{} · {:02} {}",
        weekday_pt_br(now.weekday().number_from_monday()),
        now.day(),
        month_pt_br(now.month())
    )
}

/// Abrev pt-BR do dia da semana (1=seg..7=dom). Espelha a barra velha.
fn weekday_pt_br(d: u32) -> &'static str {
    match d {
        1 => "seg",
        2 => "ter",
        3 => "qua",
        4 => "qui",
        5 => "sex",
        6 => "sáb",
        7 => "dom",
        _ => "?",
    }
}

/// Abrev pt-BR do mês (1=jan..12=dez). Espelha a barra velha.
fn month_pt_br(m: u32) -> &'static str {
    match m {
        1 => "jan",
        2 => "fev",
        3 => "mar",
        4 => "abr",
        5 => "mai",
        6 => "jun",
        7 => "jul",
        8 => "ago",
        9 => "set",
        10 => "out",
        11 => "nov",
        12 => "dez",
        _ => "?",
    }
}

fn boot() -> (Bar, Task<Message>) {
    // Lê acento + HC + janelas no boot pra a barra abrir já no estado certo.
    let read = Task::batch([
        Task::perform(ipc::read_accent(), Message::AccentLoaded),
        Task::perform(ipc::read_high_contrast(), Message::HighContrastLoaded),
        Task::perform(ipc::read_windows(), Message::WindowsLoaded),
        Task::perform(ipc::read_battery(), Message::BatteryLoaded),
        Task::perform(ipc::read_volume(), Message::VolumeLoaded),
        Task::perform(ipc::read_network(), Message::NetworkLoaded),
    ]);
    (
        Bar {
            clock: now_hm(),
            date: now_date(),
            windows: Vec::new(),
            battery: None,
            volume: (0, false),
            network: String::new(),
        },
        read,
    )
}

fn update(state: &mut Bar, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            state.clock = now_hm();
            state.date = now_date();
        }
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
        // Taskbar: poll → fetch; fetch → guarda; clique → ativa e re-poll na
        // hora (o destaque do foco atualiza rápido).
        Message::PollWindows => {
            return Task::perform(ipc::read_windows(), Message::WindowsLoaded);
        }
        Message::WindowsLoaded(list) => {
            state.windows = list
                .into_iter()
                .map(|(id, title, app_id, focused)| WinEntry {
                    id,
                    label: resolve_label(&title, &app_id),
                    focused,
                })
                .collect();
        }
        Message::ActivateWindow(id) => {
            return Task::perform(ipc::activate_window(id), |_| Message::PollWindows);
        }
        // Bateria (passo 3): boot e sinal aplicam igual. `present=false` =
        // sem bateria (esconde o applet).
        Message::BatteryLoaded((present, cap, chg)) | Message::BatteryChanged((present, cap, chg)) => {
            state.battery = present.then_some((cap, chg));
        }
        // Volume (passo 4): boot/sinal aplicam; scroll/clique mexem otimista +
        // mandam o set/toggle (o sinal confirma).
        Message::VolumeLoaded((level, muted)) | Message::VolumeChanged((level, muted)) => {
            state.volume = (level, muted);
        }
        Message::VolumeScroll(delta) => {
            let y = match delta {
                iced::mouse::ScrollDelta::Lines { y, .. } => y,
                iced::mouse::ScrollDelta::Pixels { y, .. } => y,
            };
            let step: i32 = if y > 0.0 {
                5
            } else if y < 0.0 {
                -5
            } else {
                0
            };
            if step != 0 {
                let new = (state.volume.0 as i32 + step).clamp(0, 100) as u8;
                state.volume.0 = new; // otimista
                return Task::perform(ipc::set_volume(new), |_| Message::Noop);
            }
        }
        Message::ToggleMute => {
            state.volume.1 = !state.volume.1; // otimista
            return Task::perform(ipc::toggle_mute(), |_| Message::Noop);
        }
        Message::Noop => {}
        // Rede (passo 5): boot/sinal aplicam o nome do ícone.
        Message::NetworkLoaded(icon) | Message::NetworkChanged(icon) => {
            state.network = icon;
        }
        _ => {}
    }
    Task::none()
}

/// Resolve o rótulo do botão: título, senão app_id, senão genérico. Trunca.
fn resolve_label(title: &str, app_id: &str) -> String {
    let raw = if !title.trim().is_empty() {
        title.trim()
    } else if !app_id.trim().is_empty() {
        app_id.trim()
    } else {
        "Janela"
    };
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() > TITLE_MAX {
        format!("{}…", chars[..TITLE_MAX - 1].iter().collect::<String>())
    } else {
        raw.to_string()
    }
}

/// Applet de bateria: corpo com fill proporcional + terminal + "%". Carregando
/// = fill sage (verde); baixa (<20% e descarregando) = coral (alerta próprio,
/// não o acento); normal = creme.
fn battery_applet<'a>(cap: u8, charging: bool) -> Element<'a, Message> {
    let pal = libluma::tokens::Palette::for_contrast(libluma::tokens::high_contrast());
    let cream = libluma::tokens::to_iced(pal.cream);
    let coral = libluma::tokens::to_iced(pal.coral);
    let sage = libluma::tokens::to_iced(pal.sage);
    let fill_color = if charging {
        sage
    } else if cap < 20 {
        coral
    } else {
        cream
    };

    let body_w = 22.0;
    let body_h = 11.0;
    let pad = 2.0;
    let fill_w = ((body_w - 2.0 * pad) * (cap as f32 / 100.0)).max(2.0);

    let fill = container(Space::new().width(Length::Fixed(fill_w)).height(Length::Fill)).style(
        move |_theme| container::Style {
            background: Some(Background::Color(fill_color)),
            border: Border {
                radius: 1.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let body = container(row![fill, Space::new().width(Length::Fill)])
        .width(Length::Fixed(body_w))
        .height(Length::Fixed(body_h))
        .padding(pad)
        .style(move |_theme| container::Style {
            border: Border {
                color: cream,
                width: 1.0,
                radius: 3.0.into(),
            },
            ..Default::default()
        });
    let nub = container(Space::new().width(Length::Fixed(2.0)).height(Length::Fixed(5.0))).style(
        move |_theme| container::Style {
            background: Some(Background::Color(cream)),
            border: Border {
                radius: 1.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let icon = row![body, nub].spacing(1).align_y(iced::Alignment::Center);

    row![icon, text(format!("{cap}%")).size(13.0).color(fill_color)]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
}

/// Handle do SVG de rede pro nome do estado. Os SVGs já existem em
/// `assets/branding/icons/` — bundlados via `include_bytes!`, mapeados por
/// nome (o compositor manda o nome via `network_icon_name`). Fallback off.
fn net_svg(icon: &str) -> svg::Handle {
    let bytes: &'static [u8] = match icon {
        "ethernet-on" => include_bytes!("../../../assets/branding/icons/icon-ethernet-on.svg"),
        "ethernet-off" => include_bytes!("../../../assets/branding/icons/icon-ethernet-off.svg"),
        "wifi-full" => include_bytes!("../../../assets/branding/icons/icon-wifi-full.svg"),
        "wifi-low" => include_bytes!("../../../assets/branding/icons/icon-wifi-low.svg"),
        "wifi-none" => include_bytes!("../../../assets/branding/icons/icon-wifi-none.svg"),
        _ => include_bytes!("../../../assets/branding/icons/icon-wifi-off.svg"),
    };
    svg::Handle::from_memory(bytes)
}

/// Applet de rede: o ícone SVG do estado (tipo + força), tingido pelo tema.
/// Nome no hover fica pra depois (o estado atual não traz SSID).
fn network_applet<'a>(icon: &str) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    svg(net_svg(icon))
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .style(move |_theme, _status| svg::Style { color: Some(cream) })
        .into()
}

/// Applet de volume: medidor horizontal + "%", **scroll** muda o nível e
/// **clique** alterna mudo. Mudo = medidor apagado + "mudo". Segue o tema.
fn volume_applet<'a>(level: u8, muted: bool) -> Element<'a, Message> {
    let pal = libluma::tokens::Palette::for_contrast(libluma::tokens::high_contrast());
    let cream = libluma::tokens::to_iced(pal.cream);
    let track = Color { a: 0.18, ..cream };
    let fill_color = if muted {
        Color { a: 0.30, ..cream }
    } else {
        cream
    };
    let bar_w = 30.0;
    let bar_h = 5.0;
    let fill_w = (bar_w * (level as f32 / 100.0)).max(1.0);

    let fill = container(Space::new().width(Length::Fixed(fill_w)).height(Length::Fill)).style(
        move |_theme| container::Style {
            background: Some(Background::Color(fill_color)),
            border: Border {
                radius: 2.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let meter = container(row![fill, Space::new().width(Length::Fill)])
        .width(Length::Fixed(bar_w))
        .height(Length::Fixed(bar_h))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(track)),
            border: Border {
                radius: 2.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let label = if muted {
        "mudo".to_string()
    } else {
        format!("{level}%")
    };
    let label_color = if muted {
        Color { a: 0.55, ..cream }
    } else {
        cream
    };

    let content = row![meter, text(label).size(12.0).color(label_color)]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    mouse_area(content)
        .on_scroll(Message::VolumeScroll)
        .on_press(Message::ToggleMute)
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

fn view(state: &Bar) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    let bar_bg = libluma::tokens::to_iced(sem.surface);
    let accent = libluma::tokens::accent();
    let secondary = libluma::tokens::to_iced(sem.text_secondary);

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
    // Botões de janela (esquerda): um por janela, título; a focada destacada
    // no acento (texto na cor do fundo). Clique → ativa/foca.
    let cream = libluma::tokens::to_iced(sem.text);
    let bg_color = libluma::tokens::to_iced(sem.background);
    let win_buttons: Vec<Element<Message>> = state
        .windows
        .iter()
        .map(|w| {
            let focused = w.focused;
            let label_color = if focused { bg_color } else { cream };
            let bg = if focused {
                accent
            } else {
                Color { a: 0.10, ..cream }
            };
            button(text(w.label.clone()).size(13.0).color(label_color))
                .on_press(Message::ActivateWindow(w.id))
                .padding(iced::Padding::from([3.0, 10.0]))
                .style(move |_theme, _status| button::Style {
                    background: Some(Background::Color(bg)),
                    text_color: label_color,
                    border: Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..button::Style::default()
                })
                .into()
        })
        .collect();
    let windows_row = row(win_buttons)
        .spacing(6)
        .align_y(iced::Alignment::Center);

    // Applet de bateria (direita), só se há bateria. Segue o tema; baixa tem
    // cor de alerta própria (coral), carregando fica sage — não o acento.
    let battery_el: Element<Message> = match state.battery {
        Some((cap, charging)) => battery_applet(cap, charging),
        None => Space::new().width(Length::Fixed(0.0)).into(),
    };
    // Rede: o ícone do estado (vazio até o boot ler).
    let network_el: Element<Message> = if state.network.is_empty() {
        Space::new().width(Length::Fixed(0.0)).into()
    } else {
        network_applet(&state.network)
    };

    let content = container(
        row![
            Space::new().width(Length::Fixed(12.0)),
            windows_row,
            Space::new().width(Length::Fill),
            network_el,
            Space::new().width(Length::Fixed(14.0)),
            volume_applet(state.volume.0, state.volume.1),
            Space::new().width(Length::Fixed(14.0)),
            battery_el,
            Space::new().width(Length::Fixed(14.0)),
            dot,
            // Data (secundária) + hora (acento), lado a lado.
            row![
                text(state.date.clone()).size(12.0).color(secondary),
                text(state.clock.clone()).size(15.0).color(accent),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
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
        // Poll da lista de janelas (≤500ms de latência — "ao vivo" o bastante
        // pro passo 2; um push-signal dá pra adicionar depois).
        iced::time::every(Duration::from_millis(500)).map(|_| Message::PollWindows),
        iced::Subscription::run(accent_subscription),
        iced::Subscription::run(high_contrast_subscription),
        iced::Subscription::run(battery_subscription),
        iced::Subscription::run(volume_subscription),
        iced::Subscription::run(network_subscription),
    ])
}

fn network_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::network_signal_stream().await {
            Ok(s) => s.map(Message::NetworkChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de rede falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn volume_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::volume_signal_stream().await {
            Ok(s) => s.map(Message::VolumeChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de volume falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn battery_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::battery_signal_stream().await {
            Ok(s) => s.map(Message::BatteryChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de bateria falhou: {e}");
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
