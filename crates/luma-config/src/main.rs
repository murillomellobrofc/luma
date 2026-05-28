mod app;
mod flags;
mod ipc;
mod pages;
mod theme;

use clap::Parser;

fn main() -> cosmic::iced::Result {
    let cli = flags::CliArgs::parse();
    let app_flags = flags::LumaConfigFlags::from_cli(cli);
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(800.0, 560.0))
        .theme(theme::golden_hour());
    // 3c.4: single-instance via D-Bus (libcosmic). Já tem outra
    // janela aberta? args são entregues à instância existente pelo
    // `org.freedesktop.DbusActivation::ActivateAction`; o
    // `AppModel::dbus_activation` re-aplica aba + contexto sem
    // duplicar janela.
    cosmic::app::run_single_instance::<app::AppModel>(settings, app_flags)
}
