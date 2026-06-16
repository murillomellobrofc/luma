//! libluma — design system Golden Hour dos apps Luma.
//!
//! Lê os tokens de `luma-tokens` (fundação compositor-free, a mesma
//! fonte que o `luma-theme`/compositor consome) e expõe pros apps
//! libcosmic — SEM depender do `luma-theme`, logo sem smithay/GL no
//! grafo de build do app:
//!
//! - [`tokens`] — camada semântica (accent/background/surface/text),
//!   conversão pros tipos do cosmic e constantes de spacing, tipografia
//!   e alvo de toque PCD.
//! - O theme cosmic Golden Hour pronto pra `Settings::theme()`.
//! - Helpers de cor (struct `GoldenHour`) com a paleta nos tipos
//!   `Srgba`/`Color` esperados pelos widgets.
//! - (Futuro) Componentes visuais compartilhados — section header,
//!   icon box, card, sidebar item, mini-modal, toast. Cada um migra
//!   do `luma-config` quando um segundo app validar o padrão.
//!
//! O compositor Smithay NÃO consome libluma — ele lê o `luma-tokens`
//! via `luma-theme` direto.

mod color;
mod theme;
pub mod tokens;

pub use color::GoldenHour;
pub use theme::golden_hour_theme;
pub use tokens::Semantic;
