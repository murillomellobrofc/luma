use cosmic::app::{Core, Task};
use cosmic::iced::widget;
use cosmic::widget::nav_bar;
use cosmic::{Application, Element};

use crate::pages::Page;

const APP_ID: &str = "dev.luma.Config";

#[derive(Debug, Clone)]
pub enum Message {
    #[allow(dead_code)]
    // Placeholder até as mensagens reais (seleção de página, toggles, etc.) entrarem.
    None,
}

pub struct AppModel {
    core: Core,
    nav: nav_bar::Model,
    current_page: Page,
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
        };
        (model, Task::none())
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
            Message::None => {}
        }
        Task::none()
    }
}
