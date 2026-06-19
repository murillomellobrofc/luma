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

    // boot (3a) devolve (estado, Task) — o Task estabelece a conexão D-Bus
    // com o compositor em background, sem travar a subida da janela.
    iced::application(app::boot, app::update, app::view)
        // O título é lido pelo compositor pra desenhar o header oficial.
        .title("Configurações do Luma")
        .theme(app::theme)
        // HC parte 2: escuta o sinal de mudança de aparência do compositor
        // e re-resolve a paleta ao vivo.
        .subscription(app::subscription)
        // App frameless: SEM decoração própria. O Luma decora as janelas
        // dos apps dele server-side (header oficial Golden Hour). Deixamos
        // a decoração ligada pra o compositor desenhar o header.
        // Largura default > que o cap de 860 do corpo, pra o conteúdo
        // centralizar com margem dos dois lados (em vez de encher e
        // sobrar deserto de um lado só). Altura onde a taxonomia já passa
        // um pouco, pra a barra de rolagem aparecer no uso normal.
        .window_size(iced::Size::new(1080.0, 640.0))
        // Fonte de display Golden Hour (Fraunces) pro título das páginas.
        .font(libluma::tokens::DISPLAY_FONT_BYTES)
        .run()
}
