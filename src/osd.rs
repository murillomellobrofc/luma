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

    /// Nome do ícone SVG a procurar em `icons_path` (`icon-<name>.svg`).
    /// Volume troca conforme o nível; brilho é sempre `brightness`.
    pub fn icon_name(&self) -> &'static str {
        match self.kind {
            OsdKind::Brightness => "brightness",
            OsdKind::Volume => {
                if self.muted {
                    "volume-muted"
                } else if self.value < 0.33 {
                    "volume-low"
                } else if self.value < 0.66 {
                    "volume-medium"
                } else {
                    "volume-high"
                }
            }
        }
    }

    /// Percentual a exibir como label ("75%"). Brilho clampa em 100,
    /// volume mostra até 150 (boost).
    pub fn pct(&self) -> u32 {
        let max = match self.kind {
            OsdKind::Volume => 1.5,
            OsdKind::Brightness => 1.0,
        };
        (self.value.clamp(0.0, max) * 100.0).round() as u32
    }

    /// Fração `0.0..=1.0` da barra preenchida. Volume além de 100%
    /// renderiza com cor "boost" (coral), mas a barra fica cheia.
    pub fn bar_fraction(&self) -> f32 {
        match self.kind {
            OsdKind::Brightness => self.value.clamp(0.0, 1.0),
            OsdKind::Volume => self.value.clamp(0.0, 1.0),
        }
    }

    /// `true` se a barra deve usar a cor coral (volume > 100%).
    pub fn is_boost(&self) -> bool {
        matches!(self.kind, OsdKind::Volume) && self.value > 1.0 && !self.muted
    }
}

/// Posição/tamanho do OSD computados a partir do output. Centro X e Y.
#[derive(Debug, Clone, Copy)]
pub struct OsdLayout {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: i32,
    pub height: i32,
    /// Bounding box do ícone (x, y, size).
    pub icon_x: i32,
    pub icon_y: i32,
    pub icon_size: i32,
    /// Bounding box da barra de progresso (x, y, w, h).
    pub bar_x: i32,
    pub bar_y: i32,
    pub bar_w: i32,
    pub bar_h: i32,
    /// X onde a label "N%" começa (alinhada à direita do bar_x+bar_w).
    pub label_x_right: i32,
    pub label_center_y: i32,
}

impl OsdLayout {
    /// Calcula o layout centralizado no output (`output_w`, `output_h`
    /// em coordenadas lógicas). Usa só os tokens do tema, zero hardcode.
    pub fn compute(output_w: i32, output_h: i32, tokens: &crate::theme::OsdTokens) -> Self {
        let width = tokens.width;
        let height = tokens.height;
        let origin_x = (output_w - width) / 2;
        let origin_y = (output_h - height) / 2;

        let icon_size = tokens.icon_size;
        let icon_x = origin_x + tokens.icon_padding_left;
        let icon_y = origin_y + (height - icon_size) / 2;

        // Layout horizontal: [pad][icon][gap][bar][gap][label "100%"][pad].
        // Reserva ~52px à direita pra label (3 dígitos + %, 13pt). Resto
        // vira barra.
        let label_reserve = 52;
        let gap_icon_bar = 12;
        let bar_x = icon_x + icon_size + gap_icon_bar;
        let bar_h = tokens.bar_height;
        let bar_y = origin_y + (height - bar_h) / 2;
        let bar_right = origin_x + width - tokens.bar_padding_right - label_reserve;
        let bar_w = (bar_right - bar_x).max(0);

        let label_x_right = origin_x + width - tokens.bar_padding_right;
        let label_center_y = origin_y + height / 2;

        Self {
            origin_x,
            origin_y,
            width,
            height,
            icon_x,
            icon_y,
            icon_size,
            bar_x,
            bar_y,
            bar_w,
            bar_h,
            label_x_right,
            label_center_y,
        }
    }
}
