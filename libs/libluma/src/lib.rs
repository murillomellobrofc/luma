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
//! - [`icons`] — resolução de ícone freedesktop (`Icon=` → arquivo, e
//!   `app_id` Wayland → ícone do app). Compartilhado pelo menu e pela barra.
//! - [`category_tile`] e (futuro) demais componentes visuais.
//!
//! O compositor Smithay NÃO consome libluma — ele lê o `luma-tokens`
//! via `luma-theme` direto.

mod color;
pub mod components;
pub mod focusable;
pub mod icons;
mod theme;
pub mod tokens;

pub use color::GoldenHour;
pub use icons::{IconKind, classify_icon, icon_for_app_id, resolve_icon_path};
pub use components::{
    GridItem, GridSection, accent_picker, accent_swatch, category_grid, category_grid_filtered,
    category_tile, coming_soon_row, columns_for, nav_row, page_header, scrollbar_style,
    settings_group, slider_row, toggle_row,
};
pub use focusable::{focusable, focusable_stepper};
pub use theme::golden_hour_theme;
pub use tokens::Semantic;
