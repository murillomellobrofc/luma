//! Theme iced Golden Hour. Mapeia as cores nomeadas da paleta
//! `luma-tokens` (a mesma que o compositor consome) pros slots do
//! `iced::theme::Palette`.

use iced::theme::Palette;
use iced::{Color, Theme};

/// Constrói o theme Golden Hour do iced a partir da paleta resolvida do
/// `luma-tokens` (default Golden Hour + overrides do `luma.toml`).
/// Nada hardcoded aqui.
///
/// Mapeamento:
/// - background ← coffee (`window_bg_dark`)
/// - text ← cream (`text_primary`)
/// - primary ← amber (`accent_amber`)
/// - success ← sage; danger ← coral
pub fn golden_hour_theme() -> Theme {
    // Respeita o modo de contraste (HC, parte 1): HC => paleta HC (fundo
    // quase preto, texto branco-quente); normal => luma.toml/Golden Hour.
    let p = luma_tokens::Palette::resolve_contrast(crate::tokens::high_contrast());
    Theme::custom(
        "Golden Hour".to_string(),
        Palette {
            background: to_color(p.coffee.components()),
            text: to_color(p.cream.components()),
            primary: to_color(p.amber.components()),
            success: to_color(p.sage.components()),
            warning: to_color(p.honey.components()),
            danger: to_color(p.coral.components()),
        },
    )
}

/// Cor da paleta (`luma-tokens`) → `Color` do iced.
fn to_color([r, g, b, a]: [f32; 4]) -> Color {
    Color::from_rgba(r, g, b, a)
}
