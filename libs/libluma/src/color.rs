//! Paleta Golden Hour pronta nos tipos do cosmic/iced. Lê os tokens
//! do `luma-tokens` (fonte compositor-free, a mesma que o `luma-theme`
//! consome) e devolve `Color`s já normalizadas pra uso direto nos widgets.
//!
//! API estável a partir do dia que o segundo app Luma consumir.
//! Crescer adicionando campos, nunca quebrando os existentes.

use iced::Color;

/// Paleta Golden Hour completa. Construir uma vez no `init()` do app
/// e segurar como field — `Copy`, dá pra passar pra helpers à vontade.
#[derive(Debug, Clone, Copy)]
pub struct GoldenHour {
    /// Coffee — bg principal escuro (`window_bg_dark`, #1F1611).
    pub coffee: Color,
    /// Coffee mais profundo, base da taskbar (`taskbar_bg_bottom`).
    pub coffee_deep: Color,
    /// Surface coffee, fundo de blocos sobre o bg (`applet_bg`).
    pub coffee_surface: Color,
    /// Coral — accent principal e destrutivo (`hover_close`, #D97757).
    pub coral: Color,
    /// Terracota (#D98E5E) — hue da seção "Dispositivos" (2f). Primitivo do
    /// `luma-tokens` (não mais derivado de coral).
    pub terracota: Color,
    /// Honey — text tint do theme (`hover_minimize`, #F5D5A8).
    pub honey: Color,
    /// Honey @ alpha 0.45, pra texto secundário ou estados inativos.
    pub honey_dim: Color,
    /// Sage — verde Golden Hour (`hover_maximize`, #6B8E7F).
    pub sage: Color,
    /// Cream — texto principal de alto contraste (`text_primary`).
    pub cream: Color,
    /// Amber — destaque secundário (`accent_amber`, #E89B3D).
    pub amber: Color,
    /// Neutro quente (cinza-café, #9C8B79) — hue da seção Sistema.
    pub warm_neutral: Color,
    /// Chrome de janela (#1B1714) — fundo da barra de título custom.
    pub chrome: Color,
}

impl GoldenHour {
    /// Carrega a paleta resolvendo os tokens do `luma-tokens` (default
    /// Golden Hour + overrides do `luma.toml`).
    pub fn load() -> Self {
        let p = luma_tokens::Palette::load();
        let honey = to_color(p.honey.components());
        let coral = to_color(p.coral.components());
        Self {
            coffee: to_color(p.coffee.components()),
            coffee_deep: to_color(p.coffee_deep.components()),
            coffee_surface: to_color(p.coffee_surface.components()),
            coral,
            terracota: to_color(p.terracota.components()),
            honey,
            honey_dim: with_alpha(honey, 0.45),
            sage: to_color(p.sage.components()),
            cream: to_color(p.cream.components()),
            amber: to_color(p.amber.components()),
            warm_neutral: to_color(p.warm_neutral.components()),
            chrome: to_color(p.chrome.components()),
        }
    }
}

fn to_color([r, g, b, a]: [f32; 4]) -> Color {
    Color::from_rgba(r, g, b, a)
}

fn with_alpha(c: Color, alpha: f32) -> Color {
    Color { a: alpha, ..c }
}
