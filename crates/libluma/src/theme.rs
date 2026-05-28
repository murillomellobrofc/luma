//! Theme cosmic Golden Hour. Mapeia as cores nomeadas do `luma-theme`
//! (paleta única, compartilhada com o compositor) pros slots do
//! `cosmic_theme::ThemeBuilder` partindo do dark.
//!
//! Movido do `luma-config` durante a criação da libluma (a função
//! existe igualzinha desde o sub-passo 3 da 3a, já validada).

use std::sync::Arc;

use cosmic::cosmic_theme::ThemeBuilder;
use cosmic::cosmic_theme::palette::{Srgb, Srgba};

/// Constrói o theme Golden Hour do cosmic. Lê as 4 cores nomeadas
/// direto da `ColorTokens` do luma-theme — nada hardcoded.
///
/// Mapeamento (decidido na 3a):
/// - accent / destructive ← coral (`hover_close`)
/// - bg_color + neutral_tint ← coffee (`window_bg_dark`)
/// - text_tint ← honey (`hover_minimize`)
pub fn golden_hour_theme() -> cosmic::Theme {
    luma_theme::init();
    let colors = &luma_theme::current().colors;

    let coffee = to_srgb(colors.window_bg_dark.components());
    let coral = to_srgb(colors.hover_close.components());
    let honey = to_srgb(colors.hover_minimize.components());

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

/// Helper: `Color32F` do luma-theme/smithay vira `Srgb` do palette.
/// Acessa `.components() -> [f32; 4]` em vez de nomear `Color32F`
/// pra evitar declarar smithay como dep direta da libluma.
fn to_srgb([r, g, b, _]: [f32; 4]) -> Srgb {
    Srgb::new(r, g, b)
}
