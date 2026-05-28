use std::time::Duration;

use clap::Parser;
use cosmic::app::{Core, Task};
use cosmic::iced::widget::{self, container, row};
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::theme;
use cosmic::widget::toaster::{Toast, ToastId, Toasts, toaster};
use cosmic::font;
use cosmic::widget::{button, column, dialog, icon, text, text_input};
use cosmic::{Application, Element};
use libluma::GoldenHour;
use luma_ipc::{WidgetInfo, WidgetTarget};

use crate::flags::{CliArgs, LumaConfigFlags};
use crate::ipc;
use crate::pages::Page;

const APP_ID: &str = "dev.luma.Config";

const RECONNECT_DELAY: Duration = Duration::from_secs(3);

const DOT_SIZE: u16 = 10;

const SIDEBAR_WIDTH: u16 = 220;
const SIDEBAR_ITEM_SQUARE: u16 = 26;
const SIDEBAR_ITEM_SQUARE_RADIUS: u16 = 7;
const SIDEBAR_ITEM_ICON_SIZE: u16 = 16;
const SIDEBAR_ITEM_RADIUS: u16 = 8;
const SIDEBAR_ITEM_HEIGHT: u16 = 42;

const PAGE_HEADER_SQUARE: u16 = 60;
const PAGE_HEADER_SQUARE_RADIUS: u16 = 14;
const PAGE_HEADER_ICON_SIZE: u16 = 32;

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
    SelectPage(Page),
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
    sidebar_search: String,
    /// Paleta Golden Hour pronta nos tipos do iced. Carregada uma vez
    /// no `init` via `libluma::GoldenHour::load()` e segurada como
    /// Copy pro view/helpers pegarem por valor.
    gh: GoldenHour,
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
            gh: GoldenHour::load(),
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
        let dot_color = match &self.connection {
            ConnectionState::Connected { .. } => self.gh.sage,
            ConnectionState::Disconnected | ConnectionState::Probing => self.gh.honey_dim,
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

    // ── Sidebar custom ──────────────────────────────────────────────

    fn view_sidebar(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;

        let search = text_input::search_input("Buscar", &self.sidebar_search)
            .on_input(Message::SidebarSearchChanged)
            .width(Length::Fill);

        let items = [Page::Widgets, Page::Wallpaper, Page::Preset];
        let mut items_col = column::with_capacity(items.len()).spacing(spacing.space_xxxs);
        let query = self.sidebar_search.to_lowercase();
        for page in items {
            let meta = self.section_meta(page);
            if !query.is_empty() && !meta.label.to_lowercase().contains(&query) {
                continue;
            }
            items_col = items_col.push(self.sidebar_item(page, meta, page == self.current_page));
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

    fn sidebar_item(
        &self,
        page: Page,
        meta: SectionMeta,
        is_active: bool,
    ) -> Element<'_, Message> {
        let gh = self.gh;
        let (square_bg, icon_color) = if is_active {
            // Ativo: quadradinho honey + ícone coffee escuro (alto
            // contraste sobre o fundo coral).
            (gh.honey, gh.coffee)
        } else {
            // Inativo: quadradinho na cor da seção + ícone honey.
            (meta.color, gh.honey)
        };
        let icon_square = container(
            Element::<Message>::from(
                icon::from_name(meta.icon_name).size(SIDEBAR_ITEM_ICON_SIZE),
            ),
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

        // Label 14px (brief). Ativo ganha peso semibold (≈ "peso 500"
        // — Weight::Medium não existe na fonte default; Semibold é o
        // mais próximo). A cor cream do ativo vem do wrapping
        // container abaixo via text_color override.
        let label = if is_active {
            text(meta.label).size(14).font(font::semibold())
        } else {
            text(meta.label).size(14)
        };

        let content = row![icon_square, label]
            .spacing(11)
            .align_y(Alignment::Center)
            .padding([0, 10])
            .width(Length::Fill);

        let item_bg = if is_active { Some(gh.coral) } else { None };
        let text_color = if is_active { Some(gh.cream) } else { None };
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

    // ── Conteúdo principal ──────────────────────────────────────────

    fn view_content(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let header = self.view_page_header();
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

    fn view_page_header(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let meta = self.section_meta(self.current_page);
        let cream = self.gh.cream;
        let bg = meta.color;

        let icon_box = container(
            Element::<Message>::from(
                icon::from_name(meta.icon_name).size(PAGE_HEADER_ICON_SIZE),
            ),
        )
        .width(PAGE_HEADER_SQUARE)
        .height(PAGE_HEADER_SQUARE)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .class(theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: f32::from(PAGE_HEADER_SQUARE_RADIUS).into(),
                    ..Default::default()
                },
                text_color: Some(cream),
                icon_color: Some(cream),
                ..Default::default()
            }
        }));

        // Brief: título 22px peso 500 honey; descrição 13px honey_dim.
        let text_col = column::with_capacity(2)
            .push(text(meta.label).size(22).font(font::semibold()))
            .push(dim_text(meta.description, 13, self.gh.honey_dim))
            .spacing(spacing.space_xxxs);

        row![icon_box, text_col]
            .spacing(spacing.space_m)
            .align_y(Alignment::Center)
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
                    cards = cards.push(self.widget_card(info));
                }
                cards.into()
            }
        }
    }

    fn widget_card<'a>(&self, info: &'a WidgetInfo) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let cream = self.gh.cream;

        // Cor do quadradinho deriva do tipo de widget — sage pra label,
        // honey pra counter. Coerência com a cor que o widget terá no
        // painel (registry futuro pode passar isso direto).
        let square_color = match info.type_name.as_str() {
            "counter" => self.gh.honey,
            _ => self.gh.sage,
        };

        let icon_box = container(
            Element::<Message>::from(
                icon::from_name(info.icon_name.as_str()).size(CARD_ICON_SIZE),
            ),
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

        // Brief: nome 14px peso 500 honey; descrição 12px honey_dim.
        let content = column::with_capacity(4)
            .push(icon_box)
            .push(text(info.display_name.as_str()).size(14).font(font::semibold()))
            .push(dim_text(&info.description, 12, self.gh.honey_dim))
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

    fn section_meta(&self, page: Page) -> SectionMeta {
        match page {
            Page::Widgets => SectionMeta {
                label: "Widgets",
                description: "Adicione widgets ao painel ou ao desktop.",
                color: self.gh.coral,
                icon_name: "applications-science-symbolic",
            },
            Page::Wallpaper => SectionMeta {
                label: "Wallpaper",
                description: "Escolha a imagem de fundo do seu desktop.",
                color: self.gh.terracota,
                icon_name: "preferences-desktop-wallpaper-symbolic",
            },
            Page::Preset => SectionMeta {
                label: "Preset",
                description: "Aplique um preset visual completo.",
                color: self.gh.honey,
                icon_name: "preferences-desktop-theme-symbolic",
            },
        }
    }
}

/// Metadata visual de cada seção (cor + ícone + label). Vive no
/// luma-config até a libluma ganhar um padrão de "section" próprio.
#[derive(Debug, Clone, Copy)]
struct SectionMeta {
    label: &'static str,
    description: &'static str,
    color: Color,
    icon_name: &'static str,
}

fn placeholder_body<'a>(msg: &'a str) -> Element<'a, Message> {
    text(msg).size(15).into()
}

/// Renderiza `s` em `size`px com cor honey_dim (text_color via
/// container wrapping — iced text `.color()` não funciona sob o
/// cosmic Theme). Usado em descrições de página e card.
fn dim_text<'a>(s: &'a str, size: u16, dim_color: Color) -> Element<'a, Message> {
    container(text(s).size(size))
        .class(theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                text_color: Some(dim_color),
                ..Default::default()
            }
        }))
        .into()
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
