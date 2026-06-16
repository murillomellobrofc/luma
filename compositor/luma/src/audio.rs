//! Integração com PipeWire via `wpctl` (controle) + parsing de
//! `wpctl status` (listagem). H44.volume.5+.
//!
//! Sem dependência da crate `pipewire` (assíncrona, complexa). Spawn
//! de processo é rápido (~5ms) e roda só quando popup abre / refresh.

use std::process::Command;

/// Tipo de dispositivo de áudio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKind {
    Sink,
    Source,
}

/// Um dispositivo de áudio (sink output ou source input).
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: u32,
    /// Sink ou Source. Hoje só usado pra construir as listas separadas;
    /// futuras features (filtro por tipo, etc) vão ler aqui.
    #[allow(dead_code)]
    pub kind: AudioKind,
    /// Nome legível ex: "G435 Wireless Gaming Headset Digital Stereo".
    pub description: String,
    /// 0.0..2.0 (wpctl reporta valores brutos; 1.0 = 100%).
    pub volume: f32,
    pub muted: bool,
    pub is_default: bool,
}

/// Stream de áudio de um app (sink-input no PipeWire). H44.volume.12.
#[derive(Debug, Clone)]
pub struct AudioStream {
    pub id: u32,
    /// Nome do app (ex: "Brave", "Spotify").
    pub name: String,
    pub volume: f32,
    pub muted: bool,
}

/// Lista os streams de áudio (apps tocando agora) via `wpctl status`.
/// Pega apenas as linhas top-level da seção Streams (não os routes
/// internos com `>`). H44.volume.12.
pub fn list_streams() -> Vec<AudioStream> {
    let out = match Command::new("wpctl").arg("status").output() {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let s = String::from_utf8_lossy(&out.stdout);
    let mut in_streams = false;
    let mut streams = Vec::new();
    for line in s.lines() {
        // Detecta entrada/saída da seção Streams.
        if line.contains("Streams:") {
            in_streams = true;
            continue;
        }
        if !in_streams {
            continue;
        }
        // Sai da seção quando bate Audio/Video/Settings (próximo header).
        let trimmed = line.trim();
        if trimmed.starts_with("Video")
            || trimmed.starts_with("Settings")
            || trimmed.starts_with("Audio")
        {
            in_streams = false;
            continue;
        }
        // Top-level stream: ID + Name, sem '>'. Sub-routes têm '>'.
        if line.contains('>') {
            continue;
        }
        let cleaned: String = line.chars().filter(|c| !"├─│└".contains(*c)).collect();
        let t = cleaned.trim();
        if t.is_empty() {
            continue;
        }
        // Espera: "ID. Name"
        if let Some(dot) = t.find('.') {
            if let Ok(id) = t[..dot].trim().parse::<u32>() {
                let name = t[dot + 1..].trim().to_string();
                if !name.is_empty() {
                    streams.push(AudioStream {
                        id,
                        name,
                        volume: 1.0,
                        muted: false,
                    });
                }
            }
        }
    }
    // Atualiza volume + mute por id.
    for s in &mut streams {
        if let Some((v, m)) = get_volume(s.id) {
            s.volume = v;
            s.muted = m;
        }
    }
    streams
}

/// Lê `wpctl status` e devolve sinks (`output`) + sources (`input`)
/// com metadados (id, description, volume, default).
/// Retorna `(sinks, sources)`. H44.volume.5.
pub fn list_devices() -> (Vec<AudioDevice>, Vec<AudioDevice>) {
    let out = match Command::new("wpctl").arg("status").output() {
        Ok(o) if o.status.success() => o,
        _ => return (Vec::new(), Vec::new()),
    };
    let s = String::from_utf8_lossy(&out.stdout);
    let mut sinks = Vec::new();
    let mut sources = Vec::new();
    let mut section: Option<AudioKind> = None;
    for line in s.lines() {
        // Detecta troca de seção.
        if line.contains("Sinks:") {
            section = Some(AudioKind::Sink);
            continue;
        }
        if line.contains("Sources:") {
            section = Some(AudioKind::Source);
            continue;
        }
        if line.contains("Filters:")
            || line.contains("Streams:")
            || line.contains("Devices:")
            || line.contains("Video")
            || line.contains("Settings")
        {
            section = None;
            continue;
        }
        let kind = match section {
            Some(k) => k,
            None => continue,
        };
        if let Some(dev) = parse_device_line(line, kind) {
            match kind {
                AudioKind::Sink => sinks.push(dev),
                AudioKind::Source => sources.push(dev),
            }
        }
    }
    // wpctl get-volume retorna 'muted' separadamente; nossa parsing do
    // status só pega o valor. Refresh por id pra ter mute correto.
    for d in sinks.iter_mut().chain(sources.iter_mut()) {
        if let Some((v, m)) = get_volume(d.id) {
            d.volume = v;
            d.muted = m;
        }
    }
    (sinks, sources)
}

/// Parseia uma linha de sink/source do `wpctl status`. Formato típico:
///   ` │  *   38. G435 Wireless ...  [vol: 1.15]`
///   ` │      46. Ryzen HD ...        [vol: 0.40]`
fn parse_device_line(line: &str, kind: AudioKind) -> Option<AudioDevice> {
    // Remove os chars de árvore Unicode (├─│└ etc) e espaços iniciais.
    let cleaned: String = line
        .chars()
        .filter(|c| !"├─│└".contains(*c))
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Default marker.
    let is_default = trimmed.starts_with('*');
    let after_marker = trimmed.trim_start_matches('*').trim_start();
    // Espera: "ID. Description ... [vol: VAL]"
    let dot_idx = after_marker.find('.')?;
    let id_str = &after_marker[..dot_idx];
    let id: u32 = id_str.trim().parse().ok()?;
    let rest = &after_marker[dot_idx + 1..];
    // Volume: procura "[vol: " e fim "]".
    let (description, volume) = if let Some(vol_idx) = rest.find("[vol:") {
        let desc = rest[..vol_idx].trim().to_string();
        let v_start = vol_idx + "[vol:".len();
        let v_end = rest[v_start..].find(']').map(|i| v_start + i)?;
        let v_str = rest[v_start..v_end].trim();
        let v: f32 = v_str.parse().ok().unwrap_or(0.0);
        (desc, v)
    } else {
        (rest.trim().to_string(), 0.0)
    };
    Some(AudioDevice {
        id,
        kind,
        description,
        volume,
        muted: false, // populado por get_volume()
        is_default,
    })
}

/// Lê volume + mute de um id via `wpctl get-volume <id>`. Saída
/// típica: `"Volume: 0.42"` ou `"Volume: 0.42 [MUTED]"`.
pub fn get_volume(id: u32) -> Option<(f32, bool)> {
    let out = Command::new("wpctl")
        .args(["get-volume", &id.to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let s = s.trim();
    let muted = s.contains("[MUTED]");
    let idx = s.find("Volume:")?;
    let rest = &s[idx + "Volume:".len()..];
    let token = rest.split_whitespace().next()?;
    let v: f32 = token.parse().ok()?;
    Some((v, muted))
}

/// Define volume de um id pra `pct` (0..150, %). H44.volume.6.
pub fn set_volume(id: u32, pct: u8) -> bool {
    Command::new("wpctl")
        .args(["set-volume", &id.to_string(), &format!("{}%", pct)])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Toggle mute de um id. H44.volume.7.
pub fn toggle_mute(id: u32) -> bool {
    Command::new("wpctl")
        .args(["set-mute", &id.to_string(), "toggle"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Define o id como dispositivo default (sink ou source). H44.volume.8.
pub fn set_default(id: u32) -> bool {
    Command::new("wpctl")
        .args(["set-default", &id.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
