//! Componentes visuais compartilhados dos apps Luma (iced puro).
//!
//! - [`category_tile`]: tile estilo Cinnamon (quadrado de ícone neutro +
//!   label). 2b.2: quadrado virou `surface` neutro (sem o tint embarrado).
//! - [`category_grid`]: seções rotuladas + grade responsiva de tiles, que
//!   reflui pela largura. Dirigido por dados (a taxonomia mora no app).

use iced::widget::{column, container, mouse_area, row, scrollable, slider, svg, text, toggler};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Padding, Shadow};

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
/// Largura da borda dos cards/tiles. A cor (`semantic.border`) é
/// transparente no Golden Hour (borda invisível, normal inalterado) e forte
/// no alto contraste — é ela que separa a forma quando surface ~= fundo.
/// HC (parte 1).
const BORDER_WIDTH: f32 = 1.5;

/// Escala uma dimensão INTERATIVA (área tocável) pelo alvo de toque atual
/// (Alvos de toque parte 1). No default (`hit_scale()` == 1.0) devolve o
/// próprio baseline → idêntico ao de hoje; com o alvo maior, cresce
/// proporcional. Só pra área clicável/tocável — não pra texto nem padding
/// decorativo.
fn scaled(base: f32) -> f32 {
    base * tokens::hit_scale()
}

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
/// - `on_press`: mensagem ao clicar. `Some` + `enabled` => tile clicável
///   (cursor de mão); `None` ou "em breve" => não interage.
pub fn category_tile<'a, Msg: Clone + 'a>(
    label: &'a str,
    icon: svg::Handle,
    hue: Color,
    enabled: bool,
    on_press: Option<Msg>,
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
    let border_base = tokens::to_iced(sem.border);
    let border_color = Color {
        a: border_base.a * dim,
        ..border_base
    };

    // Quadrado neutro (surface) com o ícone do pack tingido na hue da seção.
    // Borda forte no HC (transparente no normal).
    let square = container(
        svg(icon)
            .width(Length::Fixed(scaled(ICON_GLYPH)))
            .height(Length::Fixed(scaled(ICON_GLYPH)))
            .style(move |_theme, _status| svg::Style {
                color: Some(icon_color),
            }),
    )
    .center_x(Length::Fixed(scaled(ICON_SQUARE)))
    .center_y(Length::Fixed(scaled(ICON_SQUARE)))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(square_color)),
        border: Border {
            radius: 14.0.into(),
            width: BORDER_WIDTH,
            color: border_color,
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
        .width(Length::Fixed(scaled(TILE_WIDTH)));

    let tile = container(col).padding(4);

    // Tile habilitado com mensagem vira **focável + clicável** (Teclado
    // passo 2): entra na cadeia de Tab, mostra o anel ao focar, e ativa por
    // Enter/Espaço/clique. "Em breve" (ou sem mensagem) fica só visual — sem
    // foco, então é pulado na ordem de Tab.
    match (enabled, on_press) {
        (true, Some(msg)) => crate::focusable::focusable(tile, msg).into(),
        _ => tile.into(),
    }
}

// ─── CategoryGrid ─────────────────────────────────────────────────────

/// Um item (categoria) da grade. Dado dirigido pelo app.
pub struct GridItem<'a, Msg> {
    pub label: &'a str,
    /// Handle de svg do ícone (o app é dono dos arquivos; o libluma só tinge).
    pub icon: svg::Handle,
    pub enabled: bool,
    /// Mensagem ao clicar (abrir a categoria). `None` = não navega.
    pub on_press: Option<Msg>,
}

/// Uma seção da grade: label + hue + categorias.
pub struct GridSection<'a, Msg> {
    pub label: &'a str,
    pub hue: Color,
    pub items: Vec<GridItem<'a, Msg>>,
}

/// Quantas colunas de tile cabem numa largura (min 1). O app calcula isso
/// a partir da largura do conteúdo e passa pra [`category_grid`] — assim a
/// reflow e a centralização saem do mesmo cálculo, sem `responsive`
/// aninhado.
pub fn columns_for(width: f32) -> usize {
    // Min-col escala com o alvo de toque (parte 1): tiles maiores => menos
    // colunas => reflow limpo.
    let min_col = scaled(TILE_MIN_COL);
    (((width + GRID_GAP) / (min_col + GRID_GAP)).floor() as usize).max(1)
}

/// Grade estilo Cinnamon: cada seção com label (+ dot na hue) e uma grade
/// de [`category_tile`] em `cols` colunas. **Não rola por dentro** — quem
/// rola é a janela inteira (o app envolve isto num `scrollable`, com a
/// barra na borda direita). Reflui porque `cols` vem de [`columns_for`].
pub fn category_grid<'a, Msg: Clone + 'static>(
    sections: &'a [GridSection<'a, Msg>],
    cols: usize,
) -> Element<'a, Msg> {
    let cols = cols.max(1);
    let section_views: Vec<Element<'a, Msg>> =
        sections.iter().map(|s| section_view::<Msg>(s, cols)).collect();
    column(section_views).spacing(SECTION_GAP).into()
}

/// Como [`category_grid`], mas filtrando os tiles por um predicado sobre o
/// label (a busca da landing, 2e). Itens cujo label não casa somem; seções
/// que ficam sem nenhum item somem inteiras (label junto). Retorna `None`
/// se **nada** casou — aí o app mostra o estado vazio. O predicado decide
/// o matching (o app aplica o fold case/accent-insensitive de seu lado).
pub fn category_grid_filtered<'a, Msg: Clone + 'static>(
    sections: &'a [GridSection<'a, Msg>],
    cols: usize,
    matches: impl Fn(&str) -> bool,
) -> Option<Element<'a, Msg>> {
    let cols = cols.max(1);
    let section_views: Vec<Element<'a, Msg>> = sections
        .iter()
        .filter_map(|s| {
            let items: Vec<&'a GridItem<'a, Msg>> =
                s.items.iter().filter(|it| matches(it.label)).collect();
            if items.is_empty() {
                None
            } else {
                Some(section_chrome::<Msg>(s.label, s.hue, items, cols))
            }
        })
        .collect();
    if section_views.is_empty() {
        None
    } else {
        Some(column(section_views).spacing(SECTION_GAP).into())
    }
}

/// Estilo Golden Hour da barra de rolagem (pro `.style(...)` de um
/// `scrollable`): fina, trilho invisível, puxador em `warm_neutral`,
/// âmbar no hover/drag. Tokens do `libluma`, não o cinza padrão do iced.
pub fn scrollbar_style() -> impl Fn(&iced::Theme, scrollable::Status) -> scrollable::Style {
    let p = luma_tokens::Palette::for_contrast(tokens::high_contrast());
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

fn section_view<'a, Msg: Clone + 'static>(
    s: &'a GridSection<'a, Msg>,
    cols: usize,
) -> Element<'a, Msg> {
    section_chrome::<Msg>(s.label, s.hue, s.items.iter().collect(), cols)
}

/// Monta a seção (label + dot na hue + grade de tiles) a partir de uma
/// lista de itens já escolhida. Compartilhado pela grade cheia
/// ([`category_grid`]) e pela filtrada ([`category_grid_filtered`]), pra o
/// chrome da seção sair de um lugar só.
fn section_chrome<'a, Msg: Clone + 'static>(
    label: &'a str,
    hue: Color,
    items: Vec<&'a GridItem<'a, Msg>>,
    cols: usize,
) -> Element<'a, Msg> {
    let cream = tokens::to_iced(tokens::semantic_default().text);

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
    let header = row(vec![dot.into(), text(label).size(16.0).color(cream).into()])
        .spacing(8)
        .align_y(Alignment::Center);

    // Grade: linhas de `cols` tiles.
    let tile_rows: Vec<Element<'a, Msg>> = items
        .chunks(cols)
        .map(|chunk| {
            let tiles: Vec<Element<'a, Msg>> = chunk
                .iter()
                .map(|it| {
                    category_tile::<Msg>(it.label, it.icon.clone(), hue, it.enabled, it.on_press.clone())
                })
                .collect();
            row(tiles).spacing(GRID_GAP).into()
        })
        .collect();
    let grid = column(tile_rows).spacing(GRID_GAP);

    column(vec![header.into(), grid.into()]).spacing(12).into()
}

// ─── PageHeader ───────────────────────────────────────────────────────

/// Cabeçalho da página de categoria (drill-in): quadradinho `surface` com a
/// seta de voltar âmbar (clicável => `on_back`), "Todas as configurações" em
/// texto secundário, e o título da categoria no display (Fraunces).
/// Reusável por qualquer app Luma.
pub fn page_header<'a, Msg: Clone + 'a>(title: &'a str, on_back: Msg) -> Element<'a, Msg> {
    let p = luma_tokens::Palette::for_contrast(tokens::high_contrast());
    let amber = tokens::to_iced(p.amber);
    let surface = tokens::to_iced(p.surface);
    let cream = tokens::to_iced(p.cream);
    let secondary = tokens::to_iced(p.cream_dim);
    let border = tokens::to_iced(p.border);

    let back = mouse_area(
        container(text("←").size(22.0).color(amber))
            .center_x(Length::Fixed(scaled(40.0)))
            .center_y(Length::Fixed(scaled(40.0)))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(surface)),
                border: Border {
                    radius: 12.0.into(),
                    width: BORDER_WIDTH,
                    color: border,
                },
                ..Default::default()
            }),
    )
    .on_press(on_back)
    .interaction(iced::mouse::Interaction::Pointer);

    let labels = column(vec![
        text("Todas as configurações")
            .size(12.0)
            .color(secondary)
            .into(),
        text(title)
            .size(26.0)
            // Fraunces SÓ tem a face SemiBold (peso 600) embarcada; o
            // cosmic-text não faz fallback de peso dentro da família, então
            // pedir Weight::Normal cai no sans. Casa o peso da face.
            .font(Font {
                family: iced::font::Family::Name(tokens::FONT_DISPLAY),
                weight: iced::font::Weight::Semibold,
                ..Font::DEFAULT
            })
            .color(cream)
            .into(),
    ])
    .spacing(2);

    row(vec![back.into(), labels.into()])
        .spacing(14)
        .align_y(Alignment::Center)
        .into()
}

// ─── Página: grupos e linhas de ajuste (2d) ───────────────────────────

/// Altura mínima de uma linha de ajuste — alvo de toque PCD (>= 44px).
const ROW_MIN_H: f32 = 52.0;
/// Padding horizontal interno das linhas (alinha label, controle, divider).
const ROW_PAD_H: f32 = 18.0;
/// Raio da caixa de grupo (`surface` arredondada).
const GROUP_RADIUS: f32 = 16.0;

/// Casca comum de uma linha de ajuste: largura cheia, altura >= alvo PCD,
/// conteúdo centralizado na vertical e recuado na horizontal (alinha com o
/// divider). Quem chama monta o `row` de label + controle.
fn row_shell<'a, Msg: 'a>(inner: impl Into<Element<'a, Msg>>) -> iced::widget::Container<'a, Msg> {
    container(inner)
        .width(Length::Fill)
        .center_y(Length::Fixed(scaled(ROW_MIN_H)))
        .padding(Padding::from([0.0, ROW_PAD_H]))
}

/// Caixa `surface` arredondada que agrupa linhas de ajuste, com um label de
/// grupo **opcional** em cima (ex: "Toque e alvos"). Intercala divisores
/// finos entre as linhas. Bate com as caixas do mockup. As linhas vêm
/// prontas das funções [`toggle_row`] / [`slider_row`] / [`nav_row`].
pub fn settings_group<'a, Msg: 'a>(
    label: Option<&'a str>,
    rows: Vec<Element<'a, Msg>>,
) -> Element<'a, Msg> {
    let sem = tokens::semantic_default();
    let surface = tokens::to_iced(sem.surface);
    let secondary = tokens::to_iced(luma_tokens::Palette::for_contrast(tokens::high_contrast()).cream_dim);
    let divider = Color {
        a: 0.08,
        ..tokens::to_iced(sem.text)
    };

    // Linhas + divisores intercalados (sem divisor após a última).
    let n = rows.len();
    let mut stacked: Vec<Element<'a, Msg>> = Vec::with_capacity(n.saturating_mul(2));
    for (i, r) in rows.into_iter().enumerate() {
        stacked.push(r);
        if i + 1 < n {
            // 1px recuado na horizontal pra alinhar com o conteúdo das linhas.
            let line = container(text(""))
                .width(Length::Fill)
                .height(Length::Fixed(1.0))
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(divider)),
                    ..Default::default()
                });
            stacked.push(
                container(line)
                    .width(Length::Fill)
                    .padding(Padding::from([0.0, ROW_PAD_H]))
                    .into(),
            );
        }
    }

    let border = tokens::to_iced(sem.border);
    let box_ = container(column(stacked).width(Length::Fill))
        .width(Length::Fill)
        .padding(Padding::from([4.0, 0.0]))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                radius: GROUP_RADIUS.into(),
                width: BORDER_WIDTH,
                color: border,
            },
            ..Default::default()
        });

    // Com label: empilha o label sobre a caixa. Sem label: só a caixa.
    match label {
        Some(l) => column(vec![
            text(l).size(13.0).color(secondary).into(),
            box_.into(),
        ])
        .spacing(8)
        .width(Length::Fill)
        .into(),
        None => box_.into(),
    }
}

/// Linha de ajuste com toggle à direita (ligado = âmbar). `value` é o estado
/// atual; `on_toggle` recebe o novo valor ao clicar. Estado mora no app.
pub fn toggle_row<'a, Msg: Clone + 'a>(
    label: &'a str,
    value: bool,
    on_toggle: impl Fn(bool) -> Msg + 'a,
) -> Element<'a, Msg> {
    let cream = tokens::to_iced(tokens::semantic_default().text);
    let amber = tokens::to_iced(luma_tokens::Palette::for_contrast(tokens::high_contrast()).amber);
    let off = Color { a: 0.22, ..cream };

    let tog = toggler(value)
        .on_toggle(on_toggle)
        .size(scaled(26.0))
        .style(move |_theme, status| {
            let on = matches!(
                status,
                toggler::Status::Active { is_toggled: true }
                    | toggler::Status::Hovered { is_toggled: true }
            );
            toggler::Style {
                background: Background::Color(if on { amber } else { off }),
                background_border_width: 0.0,
                background_border_color: Color::TRANSPARENT,
                foreground: Background::Color(cream),
                foreground_border_width: 0.0,
                foreground_border_color: Color::TRANSPARENT,
                text_color: None,
                border_radius: None,
                padding_ratio: 0.2,
            }
        });

    row_shell(
        row(vec![
            text(label)
                .size(14.0)
                .color(cream)
                .width(Length::Fill)
                .into(),
            tog.into(),
        ])
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .into()
}

/// Linha de ajuste com slider + valor (ex: "44 px"). `range`/`step` definem
/// o intervalo; `on_change` recebe o novo valor **a cada tick** enquanto
/// arrasta (atualiza a UI ao vivo). `on_release` (opcional) dispara **uma
/// vez ao soltar** — é onde o app deve commitar (D-Bus/disco), pra o
/// arraste não martelar o backend. O valor é formatado como inteiro +
/// `unit`. Estado mora no app.
pub fn slider_row<'a, Msg: Clone + 'a>(
    label: &'a str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    unit: &'a str,
    on_change: impl Fn(f32) -> Msg + 'a,
    on_release: Option<Msg>,
) -> Element<'a, Msg> {
    let cream = tokens::to_iced(tokens::semantic_default().text);
    let secondary = tokens::to_iced(luma_tokens::Palette::for_contrast(tokens::high_contrast()).cream_dim);
    let amber = tokens::to_iced(luma_tokens::Palette::for_contrast(tokens::high_contrast()).amber);
    let track = Color { a: 0.18, ..cream };

    let sld = slider(range, value, on_change)
        .step(step)
        .width(Length::Fill)
        .style(move |_theme, _status| slider::Style {
            rail: slider::Rail {
                backgrounds: (Background::Color(amber), Background::Color(track)),
                width: 5.0,
                border: Border {
                    radius: 3.0.into(),
                    ..Default::default()
                },
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle { radius: scaled(9.0) },
                background: Background::Color(cream),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
        });
    // Commit no soltar: só registra se o app passou a mensagem.
    let sld = match on_release {
        Some(msg) => sld.on_release(msg),
        None => sld,
    };

    let val = text(format!("{value:.0} {unit}"))
        .size(13.0)
        .color(secondary)
        .width(Length::Fixed(54.0))
        .align_x(iced::alignment::Horizontal::Right);

    row_shell(
        row(vec![
            text(label).size(14.0).color(cream).into(),
            sld.into(),
            val.into(),
        ])
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .into()
}

/// Linha de navegação: label + chevron à direita. A linha inteira é clicável
/// (cursor de mão) e emite `on_press` — pra abrir um sub-detalhe depois.
pub fn nav_row<'a, Msg: Clone + 'a>(label: &'a str, on_press: Msg) -> Element<'a, Msg> {
    let cream = tokens::to_iced(tokens::semantic_default().text);
    let secondary = tokens::to_iced(luma_tokens::Palette::for_contrast(tokens::high_contrast()).cream_dim);

    let inner = row(vec![
        text(label)
            .size(14.0)
            .color(cream)
            .width(Length::Fill)
            .into(),
        text("›").size(20.0).color(secondary).into(),
    ])
    .spacing(12)
    .align_y(Alignment::Center);

    mouse_area(row_shell(inner))
        .on_press(on_press)
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

/// Linha **"em breve"**: label dim + selo "Em breve" à direita, SEM controle
/// e SEM clique. Pra um ajuste que ainda não funciona — não pode aparentar
/// que funciona (limpeza honesta). Mesma altura/ritmo das outras linhas.
pub fn coming_soon_row<'a, Msg: 'a>(label: &'a str) -> Element<'a, Msg> {
    let sem = tokens::semantic_default();
    let cream = tokens::to_iced(sem.text);
    let secondary = tokens::to_iced(sem.text_secondary);
    let dim = Color {
        a: cream.a * DISABLED_DIM,
        ..cream
    };
    row_shell(
        row(vec![
            text(label)
                .size(14.0)
                .color(dim)
                .width(Length::Fill)
                .into(),
            text("Em breve").size(12.0).color(secondary).into(),
        ])
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .into()
}
