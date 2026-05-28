mod app;
mod ipc;
mod pages;
mod theme;

fn main() -> cosmic::iced::Result {
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(800.0, 560.0))
        .theme(theme::golden_hour());
    cosmic::app::run::<app::AppModel>(settings, ())
}
