//! Barra de título Golden Hour custom (M2) — a identidade de janela
//! única dos apps Luma, reusável. Substitui a decoração padrão do iced.
//!
//! Estilo todo via tokens do `libluma` (chrome, hues dos dots, cream, a
//! fonte de display). Emite [`TitleBarMessage`]; o app mapeia pros
//! window actions do iced.

use iced::widget::{container, mouse_area, row, text};
use iced::{Alignment, Background, Border, Color, Element, Font, Length};

use crate::tokens;

/// Altura confortável da barra.
const BAR_HEIGHT: f32 = 40.0;
/// Diâmetro dos dots de controle.
const DOT: f32 = 13.0;

/// Mensagens da barra de título. O app traduz pros window actions.
#[derive(Debug, Clone, Copy)]
pub enum TitleBarMessage {
    /// dot coral.
    Close,
    /// dot honey.
    Minimize,
    /// dot sage.
    Maximize,
    /// arrastar a área da barra (fora dos dots).
    Drag,
}

/// Barra de título Golden Hour: três dots estilo macOS à esquerda
/// (coral = fechar, honey = minimizar, sage = maximizar) + o título na
/// fonte de display. A área da barra fora dos dots arrasta a janela.
pub fn title_bar<'a>(title: &'a str) -> Element<'a, TitleBarMessage> {
    let p = luma_tokens::Palette::GOLDEN_HOUR;
    let coral = tokens::to_iced(p.coral);
    let honey = tokens::to_iced(p.honey);
    let sage = tokens::to_iced(p.sage);
    let cream = tokens::to_iced(p.cream);
    let chrome = tokens::to_iced(p.chrome);

    let dots = row(vec![
        dot(coral, TitleBarMessage::Close),
        dot(honey, TitleBarMessage::Minimize),
        dot(sage, TitleBarMessage::Maximize),
    ])
    .spacing(8)
    .align_y(Alignment::Center);

    let title_label = text(title)
        .size(15.0)
        .font(Font::with_name(tokens::FONT_DISPLAY))
        .color(cream);

    // O container abaixo preenche a largura com o chrome (bg + área de
    // drag); o conteúdo (dots + título) fica à esquerda.
    let bar = row(vec![dots.into(), title_label.into()])
        .spacing(12)
        .align_y(Alignment::Center)
        .padding(8);

    mouse_area(
        container(bar)
            .width(Length::Fill)
            .center_y(Length::Fixed(BAR_HEIGHT))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(chrome)),
                ..Default::default()
            }),
    )
    .on_press(TitleBarMessage::Drag)
    .into()
}

/// Um dot circular clicável (controle de janela). `container` tintado +
/// `mouse_area` pro clique — o clique no dot é capturado aqui, então o
/// `mouse_area` da barra (drag) não dispara junto.
fn dot<'a>(color: Color, msg: TitleBarMessage) -> Element<'a, TitleBarMessage> {
    mouse_area(
        container(text(""))
            .width(Length::Fixed(DOT))
            .height(Length::Fixed(DOT))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: (DOT / 2.0).into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_press(msg)
    .into()
}
