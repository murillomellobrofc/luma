mod app;
mod pages;

fn main() -> cosmic::iced::Result {
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(800.0, 560.0));
    cosmic::app::run::<app::AppModel>(settings, ())
}
