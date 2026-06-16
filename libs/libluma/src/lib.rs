//! libluma — design system Golden Hour dos apps Luma.
//!
//! Lê os tokens de `luma-tokens` (fundação compositor-free, a mesma
//! fonte que o `luma-theme`/compositor consome) e expõe pros apps em
//! **iced puro** (migração M1) — SEM depender do `luma-theme`, logo sem
//! smithay/GL no grafo de build do app:
//!
//! - [`tokens`] — camada semântica (accent/background/surface/text),
//!   conversão pros tipos do iced (`to_iced`) e constantes de spacing,
//!   tipografia e alvo de toque PCD.
//! - O theme `iced::Theme` Golden Hour pronto pro `.theme(...)` do app.
//! - Helpers de cor (struct `GoldenHour`) com a paleta em `iced::Color`.
//! - [`category_tile`] e (futuro) demais componentes visuais.
//!
//! O compositor Smithay NÃO consome libluma — ele lê o `luma-tokens`
//! via `luma-theme` direto.

mod color;
pub mod components;
mod theme;
pub mod tokens;

pub use color::GoldenHour;
pub use components::{
    GridItem, GridSection, category_grid, category_tile, columns_for, scrollbar_style,
};
pub use theme::golden_hour_theme;
pub use tokens::Semantic;
