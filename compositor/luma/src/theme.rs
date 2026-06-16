//! Re-export do crate `luma-theme`. O Theme foi extraído pra um crate
//! separado pra permitir que `luma-widget` (e widgets terceiros) o
//! consumam sem depender do binário luma. Esse stub mantém o path
//! `crate::theme::xxx` antigo funcionando em todo o resto do código.
//!
//! Ver `crates/luma-theme/src/lib.rs` pra a implementação real.

pub use luma_theme::*;
