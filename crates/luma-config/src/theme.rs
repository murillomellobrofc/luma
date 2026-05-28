use std::sync::Arc;

use cosmic::cosmic_theme::ThemeBuilder;
use cosmic::cosmic_theme::palette::{Srgb, Srgba};

/// Constrói o theme Golden Hour pra aplicar no `Settings`. Lê as 4 cores
/// nomeadas direto da `ColorTokens` do luma-theme — nada hardcoded aqui.
pub fn golden_hour() -> cosmic::Theme {
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

fn to_srgb([r, g, b, _]: [f32; 4]) -> Srgb {
    Srgb::new(r, g, b)
}
