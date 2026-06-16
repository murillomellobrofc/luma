//! Camada de tokens semânticos pros apps Luma.
//!
//! Re-exporta os tipos compositor-free do `luma-tokens` (camada
//! semântica accent/background/surface/text) e adiciona o que é
//! específico do lado-app: a conversão pros tipos de cor do iced e as
//! constantes de spacing, tipografia e alvo de toque PCD que os
//! componentes consomem.
//!
//! As cores não têm hex aqui — nascem no `luma-tokens` (default Golden
//! Hour + override do `luma.toml`). Spacing/tipografia/PCD são valores
//! de layout, então vivem em código (constantes).

use iced::Color;

pub use luma_tokens::{Color as TokenColor, Palette, Semantic};

/// Paleta semântica resolvida do `luma.toml` (default Golden Hour +
/// overrides). Construir uma vez no `init()` do app e segurar.
pub fn semantic() -> Semantic {
    Semantic::load()
}

/// Semântica do default Golden Hour, SEM ler o `luma.toml` — pros
/// componentes pegarem `surface`/`text` sem custo de IO por frame.
pub fn semantic_default() -> Semantic {
    Semantic::from_palette(&Palette::GOLDEN_HOUR)
}

/// Token de cor do `luma-tokens` → `Color` do iced. É assim que os
/// componentes consomem um token de cor.
pub fn to_iced(c: TokenColor) -> Color {
    let [r, g, b, a] = c.components();
    Color::from_rgba(r, g, b, a)
}

// ─── Alvo de toque (PCD) ──────────────────────────────────────────────

/// Tamanho mínimo de hit target — WCAG AAA = 44px. Mesmo valor de
/// `accessibility.hit_target_min` do `luma-theme` (os quadrados 44×44
/// da taskbar são a referência). Não muda o valor; só dá um lar
/// app-side ao número.
pub const HIT_TARGET_MIN: u16 = 44;

// ─── Spacing (escala 4px) ─────────────────────────────────────────────

pub const SPACE_XS: u16 = 4;
pub const SPACE_SM: u16 = 8;
pub const SPACE_MD: u16 = 12;
pub const SPACE_LG: u16 = 16;
pub const SPACE_XL: u16 = 24;

// ─── Tipografia ───────────────────────────────────────────────────────

/// Família de display (títulos grandes) — espelha o uso do compositor
/// (clock/calendário em Fraunces).
pub const FONT_DISPLAY: &str = "Fraunces";
/// Família de UI (títulos/labels) — espelha `title_font_family` (Manrope).
pub const FONT_UI: &str = "Manrope";

/// Escala de tamanhos de texto (px), espelhando os tamanhos já em uso.
pub const TEXT_SM: u16 = 12;
pub const TEXT_BASE: u16 = 14;
pub const TEXT_LG: u16 = 18;
pub const TEXT_XL: u16 = 24;
