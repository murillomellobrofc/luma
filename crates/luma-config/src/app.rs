use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::iced::widget::{self, container, row};
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::theme;
use cosmic::widget::{button, column, icon, nav_bar, text};
use cosmic::{Application, Element};
use luma_ipc::WidgetInfo;

use crate::ipc;
use crate::pages::Page;

const APP_ID: &str = "dev.luma.Config";

/// Atraso entre tentativas de reconexão quando o compositor não tá no
/// ar. 3s — suficiente pra dev abrir o app antes do compositor e ver o
/// indicador virar sage assim que o `Ping` aceitar.
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// Diâmetro do ponto indicador (px) no rodapé. Hit target irrelevante.
const DOT_SIZE: u16 = 10;

/// Tamanho do ícone do card de widget.
const CARD_ICON_SIZE: u16 = 48;

/// Largura fixa do card. Linha simples por enquanto (3c.2); grid de
/// verdade vira polish.
const CARD_WIDTH: u16 = 240;

/// Estado da conexão D-Bus com o compositor (3b).
#[derive(Debug, Clone)]
pub enum ConnectionState {
    Probing,
    Connected { version: String },
    Disconnected,
}

/// Estado da página Widgets (3c.2). Deriva de `ConnectionState` na
/// transição, mas vive como campo próprio porque `Loaded` carrega o
/// catálogo e `Disconnected` precisa renderizar diferente do Loading.
#[derive(Debug, Clone)]
pub enum WidgetsState {
    /// Conectado mas catálogo ainda não chegou (ou catálogo vai chegar
    /// depois do próximo Connected).
    Loading,
    Loaded(Vec<WidgetInfo>),
    /// Sem compositor — UI mostra a mensagem de "conecte o compositor"
    /// no lugar dos cards.
    Disconnected,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Resultado do `Ping`. `Ok(version)` em sucesso; `Err(msg)` em
    /// falha (zbus::Error não é Clone).
    PingResult(Result<String, String>),
    /// Resultado do `ListAvailableWidgets` disparado depois do Ping Ok.
    ListResult(Result<Vec<WidgetInfo>, String>),
    /// Botão Adicionar clicado num card. Em 3c.2 só loga; em 3c.3 abre
    /// o mini-modal de destino.
    AddRequested(String),
}

pub struct AppModel {
    core: Core,
    nav: nav_bar::Model,
    current_page: Page,
    connection: ConnectionState,
    widgets_state: WidgetsState,
}

impl Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        core.set_header_title("Configurações".to_string());

        let mut nav = nav_bar::Model::default();
        nav.insert()
            .text("Widgets")
            .icon(icon::from_name("applications-science-symbolic"))
            .data::<Page>(Page::Widgets)
            .activate();
        nav.insert()
            .text("Wallpaper")
            .icon(icon::from_name("preferences-desktop-wallpaper-symbolic"))
            .data::<Page>(Page::Wallpaper);
        nav.insert()
            .text("Preset")
            .icon(icon::from_name("preferences-desktop-theme-symbolic"))
            .data::<Page>(Page::Preset);

        let model = Self {
            core,
            nav,
            current_page: Page::Widgets,
            connection: ConnectionState::Probing,
            widgets_state: WidgetsState::Loading,
        };

        (model, ping_task(Duration::ZERO))
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Self::Message> {
        self.nav.activate(id);
        if let Some(page) = self.nav.data::<Page>(id).cloned() {
            self.current_page = page;
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self.current_page {
            Page::Widgets => self.view_widgets(),
            Page::Wallpaper => text("Wallpaper — em breve").size(18).into(),
            Page::Preset => text("Preset — em breve").size(18).into(),
        }
    }

    /// Rodapé: indicador discreto de status da conexão com o compositor.
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
                // Encadeia: catálogo de widgets logo após o ping aceitar.
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
                // Trata como desconexão; o próximo Ping retry refaz o
                // ciclo (Ping → List → Loaded).
                self.connection = ConnectionState::Disconnected;
                self.widgets_state = WidgetsState::Disconnected;
                ping_task(RECONNECT_DELAY)
            }
            Message::AddRequested(type_name) => {
                // 3c.2: ainda inerte. 3c.3 abre o mini-modal de destino.
                eprintln!("[luma-config] AddRequested({type_name}) — modal entra em 3c.3");
                Task::none()
            }
        }
    }
}

impl AppModel {
    fn view_widgets(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;

        match &self.widgets_state {
            WidgetsState::Loading => container(text("Carregando catálogo…").size(15))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
            WidgetsState::Disconnected => container(
                text("Conecte o compositor pra ver os widgets disponíveis.")
                    .size(15),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
            WidgetsState::Loaded(widgets) if widgets.is_empty() => {
                container(text("Nenhum widget disponível.").size(15))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .into()
            }
            WidgetsState::Loaded(widgets) => {
                let mut cards = row::with_capacity(widgets.len()).spacing(spacing.space_m);
                for info in widgets {
                    cards = cards.push(card(info));
                }
                container(cards)
                    .padding(spacing.space_m)
                    .width(Length::Fill)
                    .into()
            }
        }
    }
}

fn card(info: &WidgetInfo) -> Element<'_, Message> {
    let spacing = theme::active().cosmic().spacing;
    let type_name = info.type_name.clone();

    let icon_widget = icon::from_name(info.icon_name.as_str()).size(CARD_ICON_SIZE);

    let content = column::with_capacity(4)
        .push(icon_widget)
        .push(text::title4(&info.display_name))
        .push(text::body(&info.description))
        .push(
            button::suggested("Adicionar")
                .width(Length::Fill)
                .on_press(Message::AddRequested(type_name)),
        )
        .spacing(spacing.space_xs)
        .align_x(Alignment::Start);

    container(content)
        .padding(spacing.space_s)
        .width(CARD_WIDTH)
        .class(theme::Container::Card)
        .into()
}

/// Cria uma `Task` que, após `delay`, chama `connect_and_ping` e
/// devolve o resultado como `Message::PingResult`.
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

/// Cria uma `Task` que busca o catálogo de widgets e devolve via
/// `Message::ListResult`.
fn list_task() -> Task<Message> {
    cosmic::task::future(async {
        let result = ipc::fetch_available_widgets()
            .await
            .map_err(|err| err.to_string());
        Message::ListResult(result)
    })
}

fn iced_color([r, g, b, a]: [f32; 4]) -> Color {
    Color::from_rgba(r, g, b, a)
}
