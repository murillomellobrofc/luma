//! Componentes visuais compartilhados dos apps Luma — agora em **iced
//! puro** (migração M1).
//!
//! [`category_tile`]: tile estilo Mint, dirigido por dados.

use iced::widget::{column, container, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::tokens;

/// Lado do quadrado de ícone tintado (px).
const ICON_SQUARE: f32 = 46.0;
/// Largura do tile — folgada e >= `hit_target_min` (44).
const TILE_WIDTH: f32 = 96.0;
/// Opacidade do conteúdo no estado "em breve".
const DISABLED_DIM: f32 = 0.45;

/// `CategoryTile` estilo Mint, em iced: quadrado de ícone tintado na hue
/// da seção + label embaixo, sobre um card `surface` que flutua no canvas.
///
/// Dirigido por dados — recebe só o que varia:
/// - `label`: texto embaixo (o chamador já manda em sentence case).
/// - `_icon_name`: reservado — o glifo é polish deferido (quadrado fica
///   só tintado por ora).
/// - `hue`: cor da seção (amber/sage/coral/honey/warm_neutral).
/// - `enabled`: `false` => conteúdo dim ~0.45 + selo "Em breve".
///
/// `surface`/`text` vêm dos tokens (default Golden Hour). A largura do
/// tile é o alvo de toque (>= 44); ligar o clique é 2c.
pub fn category_tile<'a, Msg: 'a>(
    label: &'a str,
    _icon_name: &'a str,
    hue: Color,
    enabled: bool,
) -> Element<'a, Msg> {
    let sem = tokens::semantic_default();
    let surface = tokens::to_iced(sem.surface);
    let text_color = tokens::to_iced(sem.text);

    let dim = if enabled { 1.0 } else { DISABLED_DIM };
    let hue_bg = Color {
        a: 0.18 * dim,
        ..hue
    };
    let label_color = Color {
        a: text_color.a * dim,
        ..text_color
    };

    // Quadrado de ícone tintado (glifo deferido — vazio por ora).
    let square = container(text(""))
        .width(Length::Fixed(ICON_SQUARE))
        .height(Length::Fixed(ICON_SQUARE))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(hue_bg)),
            border: Border {
                radius: 12.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let mut items: Vec<Element<'a, Msg>> = vec![
        square.into(),
        text(label).size(13.0).color(label_color).into(),
    ];
    if !enabled {
        items.push(text("Em breve").size(10.0).color(label_color).into());
    }

    let col = column(items)
        .spacing(4)
        .align_x(Alignment::Center);

    container(col)
        .padding(8)
        .center_x(Length::Fixed(TILE_WIDTH))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                radius: 14.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
