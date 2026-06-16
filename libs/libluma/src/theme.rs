//! Theme cosmic Golden Hour. Mapeia as cores nomeadas da paleta
//! `luma-tokens` (a mesma que o compositor consome via luma-theme)
//! pros slots do `cosmic_theme::ThemeBuilder` partindo do dark.
//!
//! Movido do `luma-config` durante a criação da libluma (a função
//! existe igualzinha desde o sub-passo 3 da 3a, já validada).

use std::sync::Arc;

use cosmic::cosmic_theme::ThemeBuilder;
use cosmic::cosmic_theme::palette::{Srgb, Srgba};

/// Constrói o theme Golden Hour do cosmic. Lê 3 cores nomeadas direto
/// da paleta do `luma-tokens` — nada hardcoded.
///
/// Mapeamento (decidido na 3a):
/// - accent / destructive ← coral (`hover_close`)
/// - bg_color + neutral_tint ← coffee (`window_bg_dark`)
/// - text_tint ← honey (`hover_minimize`)
pub fn golden_hour_theme() -> cosmic::Theme {
    let p = luma_tokens::Palette::load();

    let coffee = to_srgb(p.coffee.components());
    let coral = to_srgb(p.coral.components());
    let honey = to_srgb(p.honey.components());

    let coffee_bg = Srgba::new(coffee.red, coffee.green, coffee.blue, 1.0);

    let cosmic_theme = ThemeBuilder::dark()
        .accent(coral)
        .bg_color(coffee_bg)
        .neutral_tint(coffee)
        .text_tint(honey)
        .destructive(coral)
        .build();

    cosmic::Theme::custom(Arc::new(cosmic_theme))
}

/// Helper: cor da paleta (`luma-tokens`) vira `Srgb` do palette do
/// cosmic. Usa `.components() -> [f32; 4]`.
fn to_srgb([r, g, b, _]: [f32; 4]) -> Srgb {
    Srgb::new(r, g, b)
}
