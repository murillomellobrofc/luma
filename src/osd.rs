//! OSD centralizado de volume/brilho. H45.osd.
//!
//! Componente compartilhado: o mesmo pill 260×64 vale pra volume e
//! brilho — muda só o ícone e a fonte de dados. Aparece no centro
//! da tela (X e Y), faz auto-dismiss em 1.5s, **não rouba foco** e
//! **não bloqueia cliques** (overlay puro de render, sem hit-test).
//!
//! Estado vive em `LumaState::osd: Option<Osd>`. Cada tecla XF86 que
//! ajusta volume/brilho substitui o `Osd` ali e reseta `shown_at`.
//! No tick de frame, se `now - shown_at > dismiss_ms`, limpa.

use std::time::Instant;

/// Tipo de OSD ativo. Define o ícone e a fonte de leitura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsdKind {
    Volume,
    Brightness,
}

/// OSD ativo. `value` é normalizado `0.0..=1.5` (volume vai além de
/// 100% no modo boost; brilho fica em 0..=1.0).
#[derive(Debug, Clone)]
pub struct Osd {
    pub kind: OsdKind,
    pub value: f32,
    /// Só relevante quando `kind == Volume`. Brilho não muta.
    pub muted: bool,
    pub shown_at: Instant,
}

impl Osd {
    pub fn volume(value: f32, muted: bool) -> Self {
        Self {
            kind: OsdKind::Volume,
            value,
            muted,
            shown_at: Instant::now(),
        }
    }

    pub fn brightness(value: f32) -> Self {
        Self {
            kind: OsdKind::Brightness,
            value,
            muted: false,
            shown_at: Instant::now(),
        }
    }

    /// `true` se já passou do tempo de dismiss. Usado pelo tick do
    /// winit pra limpar o overlay.
    pub fn expired(&self, dismiss_ms: u64) -> bool {
        self.shown_at.elapsed().as_millis() as u64 >= dismiss_ms
    }
}
