//! Componentes visuais compartilhados dos apps Luma (libcosmic).
//!
//! 2b.1: o [`category_tile`] — tile estilo Mint, dirigido por dados.

use cosmic::iced::widget::container;
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::widget::{column, icon, text};
use cosmic::{Element, theme};

use crate::tokens;

/// Lado do quadrado de ícone tintado (px).
const ICON_SQUARE: f32 = 46.0;
/// Tamanho do glifo dentro do quadrado.
const ICON_GLYPH: u16 = 24;
/// Largura do tile — folgada e >= `hit_target_min` (44).
const TILE_WIDTH: f32 = 96.0;
/// Opacidade do conteúdo no estado "em breve".
const DISABLED_DIM: f32 = 0.45;

/// `CategoryTile` estilo Mint: quadrado de ícone tintado na hue da
/// seção + label embaixo, sobre um card `surface` que flutua no canvas.
///
/// Dirigido por dados — recebe só o que varia:
/// - `label`: texto embaixo (o chamador já manda em sentence case).
/// - `icon_name`: named icon do tema do sistema.
/// - `hue`: cor da seção (amber/sage/coral/honey/warm_neutral).
/// - `enabled`: `false` => conteúdo dim ~0.45 + selo "Em breve".
///
/// `surface`/`text` vêm dos tokens (default Golden Hour). A área do tile
/// é o alvo de toque (>= 44); ligar o clique é 2c.
pub fn category_tile<'a, Msg: 'static>(
    label: &'a str,
    icon_name: &'a str,
    hue: Color,
    enabled: bool,
) -> Element<'a, Msg> {
    let sem = tokens::semantic_default();
    let surface = tokens::to_cosmic(sem.surface);
    let text_color = tokens::to_cosmic(sem.text);

    let dim = if enabled { 1.0 } else { DISABLED_DIM };
    // hue cheia pro ícone (dimada se "em breve"); hue alpha baixo pro
    // fundo do quadrado; texto idem.
    let icon_hue = Color { a: hue.a * dim, ..hue };
    let hue_bg = Color { a: 0.18 * dim, ..hue };
    let label_color = Color { a: text_color.a * dim, ..text_color };

    let icon_square = container(Element::<'a, Msg>::from(
        icon::from_name(icon_name).size(ICON_GLYPH),
    ))
    .width(Length::Fixed(ICON_SQUARE))
    .height(Length::Fixed(ICON_SQUARE))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .class(theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(hue_bg)),
            border: Border {
                radius: 12.0.into(),
                ..Default::default()
            },
            icon_color: Some(icon_hue),
            text_color: Some(icon_hue),
            ..Default::default()
        }
    }));

    let mut col = column::with_capacity(3)
        .push(icon_square)
        .push(text(label).size(tokens::TEXT_SM))
        .spacing(tokens::SPACE_XS)
        .align_x(Alignment::Center);

    if !enabled {
        col = col.push(text("Em breve").size(10));
    }

    container(col)
        .padding(tokens::SPACE_SM)
        .width(Length::Fixed(TILE_WIDTH))
        .center_x(Length::Fill)
        .class(theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(surface)),
                border: Border {
                    radius: 14.0.into(),
                    ..Default::default()
                },
                text_color: Some(label_color),
                ..Default::default()
            }
        }))
        .into()
}
