//! Componentes visuais compartilhados dos apps Luma (iced puro).
//!
//! - [`category_tile`]: tile estilo Cinnamon (quadrado de ícone neutro +
//!   label). 2b.2: quadrado virou `surface` neutro (sem o tint embarrado).
//! - [`category_grid`]: seções rotuladas + grade responsiva de tiles, que
//!   reflui pela largura. Dirigido por dados (a taxonomia mora no app).

use iced::widget::{column, container, row, scrollable, svg, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow};

use crate::tokens;

/// Lado do quadrado de ícone (px).
const ICON_SQUARE: f32 = 56.0;
/// Tamanho do ícone dentro do quadrado (mais presença, menos espaço morto).
const ICON_GLYPH: f32 = 30.0;
/// Largura fixa do tile (>= `hit_target_min` 44; casa com a coluna).
const TILE_WIDTH: f32 = 104.0;
/// Largura mínima de coluna pro cálculo de quantas colunas cabem.
const TILE_MIN_COL: f32 = 116.0;
/// Espaçamento entre tiles (e entre linhas da grade).
const GRID_GAP: f32 = 14.0;
/// Espaçamento entre seções.
const SECTION_GAP: f32 = 22.0;
/// Opacidade do conteúdo no estado "em breve".
const DISABLED_DIM: f32 = 0.45;

// ─── CategoryTile ─────────────────────────────────────────────────────

/// Tile estilo Cinnamon: quadrado de ícone **neutro** (`surface`, flutua
/// sobre o canvas) com o **ícone do pack na hue da seção** centralizado —
/// o "color sings": o quadrado é neutro, a cor canta no glifo. + label
/// embaixo. `enabled = false` => conteúdo (ícone + label) dim + "Em breve".
///
/// - `label`: texto embaixo (sentence case).
/// - `icon`: handle de svg do ícone — vem da taxonomia do app; o `libluma`
///   não guarda os arquivos, só recebe o handle e tinge.
/// - `hue`: cor da seção — tinge o ícone via `svg::Style { color }`.
/// - `enabled`: estado.
///
/// A largura do tile é o alvo de toque (>= 44); ligar o clique é 2c.
pub fn category_tile<'a, Msg: 'a>(
    label: &'a str,
    icon: svg::Handle,
    hue: Color,
    enabled: bool,
) -> Element<'a, Msg> {
    let sem = tokens::semantic_default();
    let surface = tokens::to_iced(sem.surface);
    let text_color = tokens::to_iced(sem.text);

    let dim = if enabled { 1.0 } else { DISABLED_DIM };
    let square_color = Color {
        a: surface.a * dim,
        ..surface
    };
    let icon_color = Color {
        a: hue.a * dim,
        ..hue
    };
    let label_color = Color {
        a: text_color.a * dim,
        ..text_color
    };

    // Quadrado neutro (surface) com o ícone do pack tingido na hue da seção.
    let square = container(
        svg(icon)
            .width(Length::Fixed(ICON_GLYPH))
            .height(Length::Fixed(ICON_GLYPH))
            .style(move |_theme, _status| svg::Style {
                color: Some(icon_color),
            }),
    )
    .center_x(Length::Fixed(ICON_SQUARE))
    .center_y(Length::Fixed(ICON_SQUARE))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(square_color)),
        border: Border {
            radius: 14.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });

    let mut items: Vec<Element<'a, Msg>> = vec![
        square.into(),
        text(label)
            .size(13.0)
            .color(label_color)
            .width(Length::Fill)
            .center()
            .into(),
    ];
    if !enabled {
        items.push(
            text("Em breve")
                .size(10.0)
                .color(label_color)
                .width(Length::Fill)
                .center()
                .into(),
        );
    }

    let col = column(items)
        .spacing(6)
        .align_x(Alignment::Center)
        .width(Length::Fixed(TILE_WIDTH));

    container(col).padding(4).into()
}

// ─── CategoryGrid ─────────────────────────────────────────────────────

/// Um item (categoria) da grade. Dado dirigido pelo app.
pub struct GridItem<'a> {
    pub label: &'a str,
    /// Handle de svg do ícone (o app é dono dos arquivos; o libluma só tinge).
    pub icon: svg::Handle,
    pub enabled: bool,
}

/// Uma seção da grade: label + hue + categorias.
pub struct GridSection<'a> {
    pub label: &'a str,
    pub hue: Color,
    pub items: Vec<GridItem<'a>>,
}

/// Quantas colunas de tile cabem numa largura (min 1). O app calcula isso
/// a partir da largura do conteúdo e passa pra [`category_grid`] — assim a
/// reflow e a centralização saem do mesmo cálculo, sem `responsive`
/// aninhado.
pub fn columns_for(width: f32) -> usize {
    (((width + GRID_GAP) / (TILE_MIN_COL + GRID_GAP)).floor() as usize).max(1)
}

/// Grade estilo Cinnamon: cada seção com label (+ dot na hue) e uma grade
/// de [`category_tile`] em `cols` colunas. **Não rola por dentro** — quem
/// rola é a janela inteira (o app envolve isto num `scrollable`, com a
/// barra na borda direita). Reflui porque `cols` vem de [`columns_for`].
pub fn category_grid<'a, Msg: 'static>(
    sections: &'a [GridSection<'a>],
    cols: usize,
) -> Element<'a, Msg> {
    let cols = cols.max(1);
    let section_views: Vec<Element<'a, Msg>> =
        sections.iter().map(|s| section_view::<Msg>(s, cols)).collect();
    column(section_views).spacing(SECTION_GAP).into()
}

/// Estilo Golden Hour da barra de rolagem (pro `.style(...)` de um
/// `scrollable`): fina, trilho invisível, puxador em `warm_neutral`,
/// âmbar no hover/drag. Tokens do `libluma`, não o cinza padrão do iced.
pub fn scrollbar_style() -> impl Fn(&iced::Theme, scrollable::Status) -> scrollable::Style {
    let p = luma_tokens::Palette::GOLDEN_HOUR;
    let neutral = tokens::to_iced(p.warm_neutral);
    let amber = tokens::to_iced(p.amber);
    move |_theme, status| {
        let active = matches!(
            status,
            scrollable::Status::Hovered {
                is_vertical_scrollbar_hovered: true,
                ..
            } | scrollable::Status::Dragged {
                is_vertical_scrollbar_dragged: true,
                ..
            }
        );
        let rail = scrollable::Rail {
            background: None,
            border: Border::default(),
            scroller: scrollable::Scroller {
                background: Background::Color(if active { amber } else { neutral }),
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
            },
        };
        scrollable::Style {
            container: container::Style::default(),
            vertical_rail: rail,
            horizontal_rail: rail,
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                shadow: Shadow::default(),
                icon: neutral,
            },
        }
    }
}

fn section_view<'a, Msg: 'static>(s: &'a GridSection<'a>, cols: usize) -> Element<'a, Msg> {
    let cream = tokens::to_iced(tokens::semantic_default().text);
    let hue = s.hue;

    // Dot na hue da seção + label.
    let dot = container(text(""))
        .width(Length::Fixed(8.0))
        .height(Length::Fixed(8.0))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(hue)),
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
    let header = row(vec![dot.into(), text(s.label).size(16.0).color(cream).into()])
        .spacing(8)
        .align_y(Alignment::Center);

    // Grade: linhas de `cols` tiles.
    let tile_rows: Vec<Element<'a, Msg>> = s
        .items
        .chunks(cols)
        .map(|chunk| {
            let tiles: Vec<Element<'a, Msg>> = chunk
                .iter()
                .map(|it| category_tile::<Msg>(it.label, it.icon.clone(), hue, it.enabled))
                .collect();
            row(tiles).spacing(GRID_GAP).into()
        })
        .collect();
    let grid = column(tile_rows).spacing(GRID_GAP);

    column(vec![header.into(), grid.into()]).spacing(12).into()
}
