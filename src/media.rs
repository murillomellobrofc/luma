//! Cliente MPRIS via `playerctl`. H46.osd-media.
//!
//! Mesmo padrão de degradação do [`crate::brightness`]: detecta o
//! binário uma vez (OnceLock) e, se ausente, todas as funções viram
//! no-op silencioso. O caminho de input + render continua válido —
//! basta `sudo apt install playerctl` pra ligar sem mudar código.
//!
//! Por que `playerctl` (vs `dbus-send`):
//! - saídas de uma linha, parser trivial (`wpctl`-style, alinha com
//!   `audio.rs` e `brightness.rs`)
//! - resolve player automaticamente (`-l` lista, default = primeiro
//!   ativo)
//! - expõe `mpris:artUrl` direto
//!
//! Trade-off: `playerctl` não vem padrão no Ubuntu/KDE; usuário
//! instala 1x. Documentado em `docs/DEBT.md`.

use std::process::Command;
use std::sync::OnceLock;

/// Estado de reprodução. `NoPlayer` cobre "playerctl ausente",
/// "nenhum player MPRIS conectado" e "playerctl falhou".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
    NoPlayer,
}

/// Metadados do track atual. Campos vazios quando o player não fornece.
#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    pub status: Option<PlaybackStatus>,
    pub title: String,
    pub artist: String,
    /// URL do mpris:artUrl (file:// ou https://). Vazio = sem arte.
    pub art_url: String,
    /// Posição atual em segundos.
    pub position: f64,
    /// Duração total em segundos. 0 = desconhecido.
    pub length: f64,
}

impl MediaInfo {
    /// `true` quando há um player ativo (qualquer status != NoPlayer).
    /// Usado pra decidir se o OSD aparece (sem player = não aparece).
    pub fn has_player(&self) -> bool {
        !matches!(self.status, Some(PlaybackStatus::NoPlayer) | None)
    }
}

static AVAILABLE: OnceLock<bool> = OnceLock::new();

fn available() -> bool {
    *AVAILABLE.get_or_init(|| {
        let ok = Command::new("which")
            .arg("playerctl")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            tracing::info!("media backend: playerctl");
        } else {
            tracing::info!(
                "media backend: indisponível (playerctl não instalado) — XF86 media keys serão no-op"
            );
        }
        ok
    })
}

/// `true` se podemos chamar playerctl. Usado pelo dispatch das
/// XF86 media keys pra ignorar quando ausente.
pub fn is_available() -> bool {
    available()
}

fn pctl(args: &[&str]) -> Option<String> {
    if !available() {
        return None;
    }
    let out = Command::new("playerctl").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn parse_status(s: &str) -> PlaybackStatus {
    match s.trim() {
        "Playing" => PlaybackStatus::Playing,
        "Paused" => PlaybackStatus::Paused,
        "Stopped" => PlaybackStatus::Stopped,
        _ => PlaybackStatus::NoPlayer,
    }
}

/// Snapshot completo do player ativo. Faz 1 chamada por campo —
/// gasto típico ~25-40ms (5x spawn). Aceitável pra polling a cada
/// 500ms enquanto OSD visível; mais barato que abrir um D-Bus client.
pub fn read_info() -> MediaInfo {
    if !available() {
        return MediaInfo {
            status: Some(PlaybackStatus::NoPlayer),
            ..Default::default()
        };
    }
    // Status primeiro: se NoPlayer/erro, pula o resto (rápido).
    let status_raw = pctl(&["status"]);
    let status = match status_raw {
        Some(s) if !s.is_empty() => parse_status(&s),
        _ => return MediaInfo {
            status: Some(PlaybackStatus::NoPlayer),
            ..Default::default()
        },
    };
    let title = pctl(&["metadata", "--format", "{{ title }}"]).unwrap_or_default();
    let artist = pctl(&["metadata", "--format", "{{ artist }}"]).unwrap_or_default();
    let art_url = pctl(&["metadata", "--format", "{{ mpris:artUrl }}"]).unwrap_or_default();
    let position = pctl(&["position"])
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    // mpris:length vem em microssegundos.
    let length = pctl(&["metadata", "--format", "{{ mpris:length }}"])
        .and_then(|s| s.parse::<f64>().ok())
        .map(|us| us / 1_000_000.0)
        .unwrap_or(0.0);
    MediaInfo {
        status: Some(status),
        title,
        artist,
        art_url,
        position,
        length,
    }
}
