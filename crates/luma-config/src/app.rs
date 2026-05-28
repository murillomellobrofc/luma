use std::time::Duration;

use clap::Parser;
use cosmic::app::{Core, Task};
use cosmic::iced::widget::{self, container, row};
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::theme;
use cosmic::widget::toaster::{Toast, ToastId, Toasts, toaster};
use cosmic::widget::{button, column, dialog, icon, text, text_input};
use cosmic::{Application, Element};
use luma_ipc::{WidgetInfo, WidgetTarget};

use crate::flags::{CliArgs, LumaConfigFlags};
use crate::ipc;
use crate::pages::Page;

const APP_ID: &str = "dev.luma.Config";

const RECONNECT_DELAY: Duration = Duration::from_secs(3);

const DOT_SIZE: u16 = 10;

/// Largura fixa da sidebar custom (estilo macOS System Settings).
const SIDEBAR_WIDTH: u16 = 220;

/// Quadradinho colorido à esquerda do item de sidebar.
const SIDEBAR_ITEM_SQUARE: u16 = 26;
const SIDEBAR_ITEM_SQUARE_RADIUS: u16 = 7;
const SIDEBAR_ITEM_ICON_SIZE: u16 = 16;
const SIDEBAR_ITEM_RADIUS: u16 = 8;
const SIDEBAR_ITEM_HEIGHT: u16 = 42;

/// Header de página: quadradão de ícone à esquerda do título.
const PAGE_HEADER_SQUARE: u16 = 60;
const PAGE_HEADER_SQUARE_RADIUS: u16 = 14;
const PAGE_HEADER_ICON_SIZE: u16 = 32;

/// Card de widget: quadradinho de ícone no topo (substitui o
/// retângulo de screenshot do brief 3c.2).
const CARD_ICON_SQUARE: u16 = 40;
const CARD_ICON_SQUARE_RADIUS: u16 = 9;
const CARD_ICON_SIZE: u16 = 22;
const CARD_WIDTH: u16 = 240;

#[derive(Debug, Clone)]
pub enum ConnectionState {
    Probing,
    Connected { version: String },
    Disconnected,
}

#[derive(Debug, Clone)]
pub enum WidgetsState {
    Loading,
    Loaded(Vec<WidgetInfo>),
    Disconnected,
}

#[derive(Debug, Clone)]
struct AddDialog {
    type_name: String,
    display_name: String,
}

/// Contexto de UI (3c.4) — Panel/Desktop pulam o mini-modal; Free
/// abre. Distinto de `luma_ipc::WidgetTarget` (que só tem Panel/Desktop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppContext {
    Panel,
    Desktop,
    Free,
}

#[derive(Debug, Clone)]
pub enum Message {
    PingResult(Result<String, String>),
    ListResult(Result<Vec<WidgetInfo>, String>),
    OpenAddDialog { type_name: String, display_name: String },
    AddDialogChoose(WidgetTarget),
    AddDialogCancel,
    AddResult(Result<(String, WidgetTarget), (String, String)>),
    CloseToast(ToastId),
    /// Re-skin: clique num item da sidebar custom.
    SelectPage(Page),
    /// Re-skin: usuário digitou no campo de busca da sidebar.
    SidebarSearchChanged(String),
}

pub struct AppModel {
    core: Core,
    current_page: Page,
    connection: ConnectionState,
    widgets_state: WidgetsState,
    add_dialog: Option<AddDialog>,
    toasts: Toasts<Message>,
    app_context: AppContext,
    /// Re-skin: filtro da sidebar custom. Vazio = mostra tudo.
    sidebar_search: String,
}

impl Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = LumaConfigFlags;
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        core.set_header_title("Configurações".to_string());

        let mut model = Self {
            core,
            current_page: Page::Widgets,
            connection: ConnectionState::Probing,
            widgets_state: WidgetsState::Loading,
            add_dialog: None,
            toasts: Toasts::new(Message::CloseToast),
            app_context: AppContext::Free,
            sidebar_search: String::new(),
        };
        model.apply_cli_args(&flags.cli);

        (model, ping_task(Duration::ZERO))
    }

    fn dbus_activation(
        &mut self,
        msg: cosmic::dbus_activation::Message,
    ) -> Task<Self::Message> {
        if let cosmic::dbus_activation::Details::ActivateAction { args, .. } = msg.msg {
            let cli = CliArgs::try_parse_from(
                std::iter::once(String::from("luma-config")).chain(args),
            )
            .unwrap_or_default();
            eprintln!(
                "[luma-config] dbus_activation: tab={:?} target={:?}",
                cli.tab, cli.target
            );
            self.apply_cli_args(&cli);
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        // Re-skin: layout macOS — sidebar custom + conteúdo.
        let layout = row![self.view_sidebar(), self.view_content()].spacing(0);
        toaster(&self.toasts, layout)
    }

    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        let d = self.add_dialog.as_ref()?;
        let title = format!("Adicionar “{}” onde?", d.display_name);
        let panel_btn = button::suggested("Adicionar ao painel")
            .width(Length::Fill)
            .on_press(Message::AddDialogChoose(WidgetTarget::Panel));
        let desktop_btn = button::suggested("Adicionar ao desktop")
            .width(Length::Fill)
            .on_press(Message::AddDialogChoose(WidgetTarget::Desktop));
        let cancel_btn = button::standard("Cancelar")
            .width(Length::Fill)
            .on_press(Message::AddDialogCancel);

        Some(
            dialog()
                .title(title)
                .control(panel_btn)
                .control(desktop_btn)
                .control(cancel_btn)
                .max_width(420.0)
                .into(),
        )
    }

    fn footer(&self) -> Option<Element<'_, Self::Message>> {
        let colors = &luma_theme::current().colors;
        let dot_color = match &self.connection {
            ConnectionState::Connected { .. } => iced_color(colors.hover_maximize.components()),
            ConnectionState::Disconnected | ConnectionState::Probing => {
                let [r, g, b, _] = colors.hover_minimize.components();
                Color::from_rgba(r, g, b, 0.45)
            }
        };

        let label = match &self.connection {
            ConnectionState::Connected { version } => version.clone(),
            ConnectionState::Disconnected => "Compositor desconectado".to_string(),
            ConnectionState::Probing => "Conectando ao compositor…".to_string(),
        };

        let dot = container(widget::Space::new())
            .width(DOT_SIZE)
            .height(DOT_SIZE)
            .class(theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(dot_color)),
                    border: Border {
                        radius: (f32::from(DOT_SIZE) / 2.0).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }));

        let indicator = row![dot, text(label).size(13)]
            .spacing(8)
            .align_y(Alignment::Center);

        Some(indicator.into())
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::PingResult(Ok(version)) => {
                eprintln!("[luma-config] conectado ao compositor: {version}");
                self.connection = ConnectionState::Connected { version };
                self.widgets_state = WidgetsState::Loading;
                list_task()
            }
            Message::PingResult(Err(err)) => {
                eprintln!(
                    "[luma-config] compositor não respondeu ao Ping ({err}); retry em {}s",
                    RECONNECT_DELAY.as_secs()
                );
                self.connection = ConnectionState::Disconnected;
                self.widgets_state = WidgetsState::Disconnected;
                ping_task(RECONNECT_DELAY)
            }
            Message::ListResult(Ok(widgets)) => {
                eprintln!(
                    "[luma-config] catálogo carregado: {} widget(s)",
                    widgets.len()
                );
                self.widgets_state = WidgetsState::Loaded(widgets);
                Task::none()
            }
            Message::ListResult(Err(err)) => {
                eprintln!(
                    "[luma-config] ListAvailableWidgets falhou: {err} — voltando pro fluxo de retry"
                );
                self.connection = ConnectionState::Disconnected;
                self.widgets_state = WidgetsState::Disconnected;
                ping_task(RECONNECT_DELAY)
            }
            Message::OpenAddDialog { type_name, display_name } => {
                match context_to_target(self.app_context) {
                    Some(target) => add_widget_task(type_name, display_name, target),
                    None => {
                        self.add_dialog = Some(AddDialog { type_name, display_name });
                        Task::none()
                    }
                }
            }
            Message::AddDialogCancel => {
                self.add_dialog = None;
                Task::none()
            }
            Message::AddDialogChoose(target) => {
                let dialog = match self.add_dialog.take() {
                    Some(d) => d,
                    None => return Task::none(),
                };
                add_widget_task(dialog.type_name, dialog.display_name, target)
            }
            Message::AddResult(Ok((display_name, target))) => {
                eprintln!(
                    "[luma-config] AddWidget OK: {display_name} → {}",
                    target.as_str()
                );
                let toast_msg =
                    format!("Adicionado: {display_name} no {}", target_label(target));
                self.toasts.push(Toast::new(toast_msg)).map(cosmic::Action::App)
            }
            Message::AddResult(Err((display_name, err))) => {
                eprintln!("[luma-config] AddWidget falhou ({display_name}): {err}");
                let toast_msg = format!("Falha ao adicionar {display_name}: {err}");
                self.toasts.push(Toast::new(toast_msg)).map(cosmic::Action::App)
            }
            Message::CloseToast(id) => {
                self.toasts.remove(id);
                Task::none()
            }
            Message::SelectPage(page) => {
                self.current_page = page;
                Task::none()
            }
            Message::SidebarSearchChanged(s) => {
                self.sidebar_search = s;
                Task::none()
            }
        }
    }
}

impl AppModel {
    fn apply_cli_args(&mut self, cli: &CliArgs) {
        self.current_page = match cli.tab.as_deref() {
            Some("wallpaper") => Page::Wallpaper,
            Some("preset") => Page::Preset,
            _ => Page::Widgets,
        };
        self.app_context = match cli.target.as_deref() {
            Some("panel") => AppContext::Panel,
            Some("desktop") => AppContext::Desktop,
            _ => AppContext::Free,
        };
    }

    // ── Sidebar custom (re-skin) ────────────────────────────────────

    fn view_sidebar(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;

        let search = text_input::search_input("Buscar", &self.sidebar_search)
            .on_input(Message::SidebarSearchChanged)
            .width(Length::Fill);

        let items = [Page::Widgets, Page::Wallpaper, Page::Preset];
        let mut items_col = column::with_capacity(items.len()).spacing(spacing.space_xxxs);
        let query = self.sidebar_search.to_lowercase();
        for page in items {
            let meta = section_meta(page);
            if !query.is_empty() && !meta.label.to_lowercase().contains(&query) {
                continue;
            }
            items_col = items_col.push(sidebar_item(page, meta, page == self.current_page));
        }

        let inner = column::with_capacity(2)
            .push(search)
            .push(items_col)
            .spacing(spacing.space_s)
            .padding(spacing.space_s);

        container(inner)
            .width(SIDEBAR_WIDTH)
            .height(Length::Fill)
            .class(theme::Container::Primary)
            .into()
    }

    // ── Conteúdo principal ──────────────────────────────────────────

    fn view_content(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let header = page_header(self.current_page);
        let body = match self.current_page {
            Page::Widgets => self.view_widgets_body(),
            Page::Wallpaper => placeholder_body("Em breve: galeria de wallpapers."),
            Page::Preset => placeholder_body("Em breve: seleção e edição de presets."),
        };

        container(
            column::with_capacity(2)
                .push(header)
                .push(body)
                .spacing(spacing.space_l),
        )
        .padding(spacing.space_l)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn view_widgets_body(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        match &self.widgets_state {
            WidgetsState::Loading => text("Carregando catálogo…").size(15).into(),
            WidgetsState::Disconnected => {
                text("Conecte o compositor pra ver os widgets disponíveis.")
                    .size(15)
                    .into()
            }
            WidgetsState::Loaded(widgets) if widgets.is_empty() => {
                text("Nenhum widget disponível.").size(15).into()
            }
            WidgetsState::Loaded(widgets) => {
                let mut cards = row::with_capacity(widgets.len()).spacing(spacing.space_m);
                for info in widgets {
                    cards = cards.push(widget_card(info));
                }
                cards.into()
            }
        }
    }
}

// ── Helpers visuais ────────────────────────────────────────────────

/// Metadata visual de cada seção da sidebar (cor + ícone + label).
/// Em commit futuro pode virar membro de Page ou viver no theme.
struct SectionMeta {
    label: &'static str,
    description: &'static str,
    /// Cor base da seção (do palette luma-theme).
    color: Color,
    /// Nome do ícone freedesktop pra renderizar dentro do quadradinho.
    icon_name: &'static str,
}

fn section_meta(page: Page) -> SectionMeta {
    let colors = &luma_theme::current().colors;
    match page {
        Page::Widgets => SectionMeta {
            label: "Widgets",
            description: "Adicione widgets ao painel ou ao desktop.",
            // Coral (hover_close) — destaque principal.
            color: iced_color(colors.hover_close.components()),
            icon_name: "applications-science-symbolic",
        },
        Page::Wallpaper => SectionMeta {
            label: "Wallpaper",
            description: "Escolha a imagem de fundo do seu desktop.",
            // Terracota — coral darkened, varia da cor de Widgets.
            color: darken_color(iced_color(colors.hover_close.components()), 0.78),
            icon_name: "preferences-desktop-wallpaper-symbolic",
        },
        Page::Preset => SectionMeta {
            label: "Preset",
            description: "Aplique um preset visual completo.",
            // Honey (hover_minimize) — amarelo dourado, terceira cor da paleta.
            color: iced_color(colors.hover_minimize.components()),
            icon_name: "preferences-desktop-theme-symbolic",
        },
    }
}

fn sidebar_item<'a>(
    page: Page,
    meta: SectionMeta,
    is_active: bool,
) -> Element<'a, Message> {
    let colors = &luma_theme::current().colors;
    let coral = iced_color(colors.hover_close.components());
    let honey = iced_color(colors.hover_minimize.components());
    let coffee = iced_color(colors.window_bg_dark.components());

    // Quadradinho do ícone: cor sólida (ou invertido pro item ativo).
    let (square_bg, icon_color) = if is_active {
        // Ativo: quadradinho honey, ícone coffee escuro (alto contraste sobre o coral).
        (honey, coffee)
    } else {
        // Inativo: quadradinho na cor da seção, ícone honey claro.
        (meta.color, honey)
    };
    let icon_square = container(
        Element::<Message>::from(icon::from_name(meta.icon_name).size(SIDEBAR_ITEM_ICON_SIZE)),
    )
    .width(SIDEBAR_ITEM_SQUARE)
    .height(SIDEBAR_ITEM_SQUARE)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .class(theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(square_bg)),
            border: Border {
                radius: f32::from(SIDEBAR_ITEM_SQUARE_RADIUS).into(),
                ..Default::default()
            },
            text_color: Some(icon_color),
            icon_color: Some(icon_color),
            ..Default::default()
        }
    }));

    let label = text(meta.label).size(14);

    let content = row![icon_square, label]
        .spacing(11)
        .align_y(Alignment::Center)
        .padding([0, 10])
        .width(Length::Fill);

    // Wrapper: muda fundo do item inteiro quando ativo.
    let item_bg = if is_active { Some(coral) } else { None };
    let text_color = if is_active {
        Some(iced_color(colors.text_primary.components()))
    } else {
        None
    };
    let wrapped = container(content)
        .width(Length::Fill)
        .height(SIDEBAR_ITEM_HEIGHT)
        .class(theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: item_bg.map(Background::Color),
                border: Border {
                    radius: f32::from(SIDEBAR_ITEM_RADIUS).into(),
                    ..Default::default()
                },
                text_color,
                ..Default::default()
            }
        }));

    button::custom(wrapped)
        .padding(0)
        .class(theme::Button::Text)
        .width(Length::Fill)
        .on_press(Message::SelectPage(page))
        .into()
}

/// Header grande no topo de cada página (quadradão + título + descrição).
fn page_header<'a>(page: Page) -> Element<'a, Message> {
    let spacing = theme::active().cosmic().spacing;
    let meta = section_meta(page);
    let colors = &luma_theme::current().colors;
    let cream = iced_color(colors.text_primary.components());

    let icon_box = container(
        Element::<Message>::from(icon::from_name(meta.icon_name).size(PAGE_HEADER_ICON_SIZE)),
    )
    .width(PAGE_HEADER_SQUARE)
    .height(PAGE_HEADER_SQUARE)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .class(theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(meta.color)),
            border: Border {
                radius: f32::from(PAGE_HEADER_SQUARE_RADIUS).into(),
                ..Default::default()
            },
            text_color: Some(cream),
            icon_color: Some(cream),
            ..Default::default()
        }
    }));

    let text_col = column::with_capacity(2)
        .push(text::title3(meta.label))
        .push(text::body(meta.description))
        .spacing(spacing.space_xxxs);

    row![icon_box, text_col]
        .spacing(spacing.space_m)
        .align_y(Alignment::Center)
        .into()
}

fn widget_card(info: &WidgetInfo) -> Element<'_, Message> {
    let spacing = theme::active().cosmic().spacing;
    let colors = &luma_theme::current().colors;
    let cream = iced_color(colors.text_primary.components());

    // Cor do quadradinho deriva do tipo de widget — local hoje, dia
    // que o registry expor cor a gente lê de lá direto. Sage pra label,
    // honey pra counter (mesma paleta dos hovers da janela).
    let square_color = match info.type_name.as_str() {
        "counter" => iced_color(colors.hover_minimize.components()),
        // label e qualquer outro caem em sage (combina com "neutro").
        _ => iced_color(colors.hover_maximize.components()),
    };

    let icon_box = container(
        Element::<Message>::from(icon::from_name(info.icon_name.as_str()).size(CARD_ICON_SIZE)),
    )
    .width(CARD_ICON_SQUARE)
    .height(CARD_ICON_SQUARE)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .class(theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(square_color)),
            border: Border {
                radius: f32::from(CARD_ICON_SQUARE_RADIUS).into(),
                ..Default::default()
            },
            text_color: Some(cream),
            icon_color: Some(cream),
            ..Default::default()
        }
    }));

    let content = column::with_capacity(4)
        .push(icon_box)
        .push(text::title4(&info.display_name))
        .push(text::body(&info.description))
        .push(
            button::suggested("Adicionar")
                .width(Length::Fill)
                .on_press(Message::OpenAddDialog {
                    type_name: info.type_name.clone(),
                    display_name: info.display_name.clone(),
                }),
        )
        .spacing(spacing.space_xs)
        .align_x(Alignment::Start);

    container(content)
        .padding(spacing.space_s)
        .width(CARD_WIDTH)
        .class(theme::Container::Card)
        .into()
}

fn placeholder_body<'a>(msg: &'a str) -> Element<'a, Message> {
    text(msg).size(15).into()
}

// ── Helpers de cor ─────────────────────────────────────────────────

fn iced_color([r, g, b, a]: [f32; 4]) -> Color {
    Color::from_rgba(r, g, b, a)
}

/// Multiplica RGB por `factor` (0..1) preservando o alpha. Usado pra
/// derivar tons (terracota = coral darkened) sem hardcodar hex.
fn darken_color(c: Color, factor: f32) -> Color {
    Color { r: c.r * factor, g: c.g * factor, b: c.b * factor, a: c.a }
}

fn target_label(t: WidgetTarget) -> &'static str {
    match t {
        WidgetTarget::Panel => "painel",
        WidgetTarget::Desktop => "desktop",
    }
}

fn context_to_target(ctx: AppContext) -> Option<WidgetTarget> {
    match ctx {
        AppContext::Panel => Some(WidgetTarget::Panel),
        AppContext::Desktop => Some(WidgetTarget::Desktop),
        AppContext::Free => None,
    }
}

fn add_widget_task(
    type_name: String,
    display_name: String,
    target: WidgetTarget,
) -> Task<Message> {
    let target_str = target.as_str();
    cosmic::task::future(async move {
        match ipc::add_widget(type_name, target_str).await {
            Ok(()) => Message::AddResult(Ok((display_name, target))),
            Err(err) => Message::AddResult(Err((display_name, err.to_string()))),
        }
    })
}

fn ping_task(delay: Duration) -> Task<Message> {
    cosmic::task::future(async move {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let result = ipc::connect_and_ping()
            .await
            .map_err(|err| err.to_string());
        Message::PingResult(result)
    })
}

fn list_task() -> Task<Message> {
    cosmic::task::future(async {
        let result = ipc::fetch_available_widgets()
            .await
            .map_err(|err| err.to_string());
        Message::ListResult(result)
    })
}
