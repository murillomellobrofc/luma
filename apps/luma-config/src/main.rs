//! luma-config — app de configuração do Luma, em **iced puro**
//! (migração M1).
//!
//! M1 é o esqueleto: abre janela iced, prova os tokens e o
//! `CategoryTile`. A sidebar e as páginas antigas do cosmic estão
//! arquivadas em `legacy/` — a grade nova (2b.2) substitui tudo. O
//! cliente D-Bus (`ipc`) sobrevive intacto e é religado à GUI na 2c.

use clap::Parser;

mod app;
mod flags;
// Camada D-Bus preservada (cliente do compositor). Ainda não religada à
// GUI iced — isso acontece na 2c.
#[allow(dead_code)]
mod ipc;
// Enum de página preservado (a navegação volta com a grade 2b.2 / 2c).
#[allow(dead_code)]
mod pages;

fn main() -> iced::Result {
    // Parseia --tab/--target no boot. Em M1 não fazem nada (a navegação
    // volta na 2c); parsear cedo só garante que args válidos não quebrem.
    let _cli = flags::CliArgs::parse();

    iced::application(app::State::new, app::update, app::view)
        .title("Configurações do Luma")
        .theme(app::theme)
        // M2: decoração padrão do iced DESLIGADA — a barra de título é a
        // moldura Golden Hour custom (libluma::title_bar). Cantos retos
        // por ora (transparência/cantos no Wayland fica pra depois; o
        // próprio compositor do Luma pode arredondar as janelas dos apps).
        .decorations(false)
        .window_size(iced::Size::new(800.0, 560.0))
        .run()
}
