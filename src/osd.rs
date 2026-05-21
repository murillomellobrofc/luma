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

/// OSD de mídia (now playing). Componente separado do OSD básico —
/// layout maior, controles clicáveis, fade-out. H46.osd-media.
#[derive(Debug, Clone)]
pub struct MediaOsd {
    pub info: crate::media::MediaInfo,
    pub shown_at: Instant,
    pub last_poll: Instant,
}

impl MediaOsd {
    pub fn new(info: crate::media::MediaInfo) -> Self {
        let now = Instant::now();
        Self {
            info,
            shown_at: now,
            last_poll: now,
        }
    }

    /// `true` se já passou do timeout. Próximo frame remove.
    pub fn expired(&self, timeout_ms: u64) -> bool {
        self.shown_at.elapsed().as_millis() as u64 >= timeout_ms
    }

    /// Reset do timer (usado em interação com botões e em key re-trigger).
    pub fn touch(&mut self) {
        self.shown_at = Instant::now();
    }

    /// Fator de opacidade em `0.0..=1.0`. 1.0 fora da janela de fade,
    /// decai linear pra 0 nos últimos `fade_ms` antes do timeout.
    pub fn alpha(&self, timeout_ms: u64, fade_ms: u64) -> f32 {
        let el = self.shown_at.elapsed().as_millis() as u64;
        if el >= timeout_ms {
            return 0.0;
        }
        if el + fade_ms <= timeout_ms {
            return 1.0;
        }
        let remain = (timeout_ms - el) as f32;
        (remain / fade_ms as f32).clamp(0.0, 1.0)
    }

    /// Posição avançada por wall-clock. Entre polls, soma o tempo
    /// decorrido se status == Playing. Polls sobrescrevem o valor.
    pub fn current_position(&self) -> f64 {
        let base = self.info.position;
        if matches!(self.info.status, Some(crate::media::PlaybackStatus::Playing)) {
            base + self.last_poll.elapsed().as_secs_f64()
        } else {
            base
        }
    }
}

/// Botões clicáveis do MediaOsd. H46.media.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaButton {
    Previous,
    PlayPause,
    Next,
}

/// Layout do card de mídia: arte à esquerda, info à direita.
#[derive(Debug, Clone, Copy)]
pub struct MediaOsdLayout {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: i32,
    pub height: i32,
    pub art_x: i32,
    pub art_y: i32,
    pub art_size: i32,
    /// Coluna de texto + controles, à direita da arte.
    pub right_col_x: i32,
    pub right_col_w: i32,
    pub title_y: i32,
    pub artist_y: i32,
    pub controls_y: i32,
    pub prev_x: i32,
    pub play_x: i32,
    pub next_x: i32,
    pub prev_size: i32,
    pub play_size: i32,
    pub next_size: i32,
    pub progress_x: i32,
    pub progress_y: i32,
    pub progress_w: i32,
    pub progress_h: i32,
}

impl MediaOsdLayout {
    pub fn compute(
        output_w: i32,
        output_h: i32,
        tokens: &crate::theme::OsdMediaTokens,
    ) -> Self {
        let width = tokens.width;
        let height = tokens.height;
        let origin_x = (output_w - width) / 2;
        let origin_y = (output_h - height) / 2;

        let pad = tokens.padding;
        let art_size = tokens.art_size;
        let art_x = origin_x + pad;
        let art_y = origin_y + (height - art_size) / 2;

        let right_col_x = art_x + art_size + pad;
        let right_col_w = origin_x + width - pad - right_col_x;

        let title_y = origin_y + pad;
        let artist_y = title_y + (tokens.title_size as i32) + 4;

        let progress_h = tokens.progress_height;
        let progress_y = origin_y + height - pad - progress_h;
        let progress_x = right_col_x;
        let progress_w = right_col_w;

        // Linha de controles entre artista e progress.
        let play_size = tokens.play_btn_size;
        let side_size = tokens.side_btn_size;
        let controls_y = progress_y - 10 - play_size;
        // Centraliza horizontalmente os 3 botões na right_col.
        let total_w = side_size + 16 + play_size + 16 + side_size;
        let group_x = right_col_x + (right_col_w - total_w) / 2;
        let prev_x = group_x;
        let play_x = prev_x + side_size + 16;
        let next_x = play_x + play_size + 16;

        Self {
            origin_x,
            origin_y,
            width,
            height,
            art_x,
            art_y,
            art_size,
            right_col_x,
            right_col_w,
            title_y,
            artist_y,
            controls_y,
            prev_x,
            play_x,
            next_x,
            prev_size: side_size,
            play_size,
            next_size: side_size,
            progress_x,
            progress_y,
            progress_w,
            progress_h,
        }
    }

    /// Hit-test contra os 3 botões. Retorna `Some(button)` SÓ se o
    /// click cair dentro de um dos rects de controle — qualquer outro
    /// ponto (arte, texto, progress) retorna `None` e propaga o click
    /// pra baixo (zero-block fora dos botões). H46.media.6.
    pub fn hit_test_button(&self, px: f64, py: f64) -> Option<MediaButton> {
        let in_rect = |x: i32, y: i32, w: i32, h: i32| -> bool {
            let xf = x as f64;
            let yf = y as f64;
            px >= xf && px < xf + w as f64 && py >= yf && py < yf + h as f64
        };
        if in_rect(self.prev_x, self.controls_y, self.prev_size, self.prev_size) {
            return Some(MediaButton::Previous);
        }
        if in_rect(self.play_x, self.controls_y, self.play_size, self.play_size) {
            return Some(MediaButton::PlayPause);
        }
        if in_rect(self.next_x, self.controls_y, self.next_size, self.next_size) {
            return Some(MediaButton::Next);
        }
        None
    }
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
