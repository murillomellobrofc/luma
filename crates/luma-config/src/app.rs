use cosmic::app::{Core, Task};
use cosmic::iced::widget;
use cosmic::widget::nav_bar;
use cosmic::{Application, Element};

use crate::ipc;
use crate::pages::Page;

const APP_ID: &str = "dev.luma.Config";

/// Estado da conexão D-Bus com o compositor (3b).
#[derive(Debug, Clone)]
#[allow(dead_code)]
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
    /// Resultado do `Ping` inicial. `Ok(version)` em sucesso; `Err(msg)`
    /// com a descrição do erro em falha (zbus::Error não é Clone).
    PingResult(Result<String, String>),
}

pub struct AppModel {
    core: Core,
    nav: nav_bar::Model,
    current_page: Page,
    #[allow(dead_code)] // 3b sub-iii usa pra renderizar indicador.
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

        let task = cosmic::task::future(async {
            let result = ipc::connect_and_ping()
                .await
                .map_err(|err| err.to_string());
            Message::PingResult(result)
        });

        (model, task)
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

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::PingResult(Ok(version)) => {
                eprintln!("[luma-config] conectado ao compositor: {version}");
                self.connection = ConnectionState::Connected { version };
            }
            Message::PingResult(Err(err)) => {
                eprintln!("[luma-config] compositor não respondeu ao Ping: {err}");
                self.connection = ConnectionState::Disconnected;
            }
        }
        Task::none()
    }
}
