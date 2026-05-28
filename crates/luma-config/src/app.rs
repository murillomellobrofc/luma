use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::iced::widget::{self, container, row};
use cosmic::iced::{Alignment, Background, Border, Color};
use cosmic::theme;
use cosmic::widget::nav_bar;
use cosmic::{Application, Element};

use crate::ipc;
use crate::pages::Page;

const APP_ID: &str = "dev.luma.Config";

/// Atraso entre tentativas de reconexão quando o compositor não tá no
/// ar. 3s — suficiente pra dev abrir o app antes do compositor e ver o
/// indicador virar sage assim que o `Ping` aceitar.
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// Diâmetro do ponto indicador (px). Hit target irrelevante — é
/// puramente visual.
const DOT_SIZE: u16 = 10;

/// Estado da conexão D-Bus com o compositor (3b).
#[derive(Debug, Clone)]
pub enum ConnectionState {
    /// Estado inicial — Ping disparado, aguardando resposta.
    Probing,
    /// Conectado; guardamos a string de versão devolvida pelo Ping.
    Connected { version: String },
    /// Compositor não respondeu ao Ping (não tá no ar, ou bus error).
    Disconnected,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Resultado do `Ping`. `Ok(version)` em sucesso; `Err(msg)` com a
    /// descrição do erro em falha (zbus::Error não é Clone).
    PingResult(Result<String, String>),
}

pub struct AppModel {
    core: Core,
    nav: nav_bar::Model,
    current_page: Page,
    connection: ConnectionState,
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
            .icon(cosmic::widget::icon::from_name("applications-science-symbolic"))
            .data::<Page>(Page::Widgets)
            .activate();
        nav.insert()
            .text("Wallpaper")
            .icon(cosmic::widget::icon::from_name("preferences-desktop-wallpaper-symbolic"))
            .data::<Page>(Page::Wallpaper);
        nav.insert()
            .text("Preset")
            .icon(cosmic::widget::icon::from_name("preferences-desktop-theme-symbolic"))
            .data::<Page>(Page::Preset);

        let model = Self {
            core,
            nav,
            current_page: Page::Widgets,
            connection: ConnectionState::Probing,
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
        let content = match self.current_page {
            Page::Widgets => "Widgets — em construção",
            Page::Wallpaper => "Wallpaper — em breve",
            Page::Preset => "Preset — em breve",
        };
        widget::text(content)
            .size(18)
            .into()
    }

    /// Rodapé: indicador discreto de status da conexão com o compositor.
    /// Ponto colorido (sage = conectado, honey-dim = desconectado /
    /// probing) + label textual com a versão ou estado.
    fn footer(&self) -> Option<Element<'_, Self::Message>> {
        let colors = &luma_theme::current().colors;
        let dot_color = match &self.connection {
            ConnectionState::Connected { .. } => iced_color(colors.hover_maximize.components()), // sage
            ConnectionState::Disconnected | ConnectionState::Probing => {
                let [r, g, b, _] = colors.hover_minimize.components(); // honey-dim
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

        let indicator = row![dot, widget::text(label).size(13)]
            .spacing(8)
            .align_y(Alignment::Center);

        Some(indicator.into())
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::PingResult(Ok(version)) => {
                eprintln!("[luma-config] conectado ao compositor: {version}");
                self.connection = ConnectionState::Connected { version };
                Task::none()
            }
            Message::PingResult(Err(err)) => {
                eprintln!(
                    "[luma-config] compositor não respondeu ao Ping ({err}); retry em {}s",
                    RECONNECT_DELAY.as_secs()
                );
                self.connection = ConnectionState::Disconnected;
                ping_task(RECONNECT_DELAY)
            }
        }
    }
}

/// Cria uma `Task` que, após `delay`, chama `connect_and_ping` e
/// devolve o resultado como `Message::PingResult`. Com `delay == 0`,
/// dispara imediatamente — usado no startup.
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

fn iced_color([r, g, b, a]: [f32; 4]) -> Color {
    Color::from_rgba(r, g, b, a)
}
