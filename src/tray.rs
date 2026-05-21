//! System tray na zona D da taskbar v5 (H33.taskbar.9-12).
//!
//! Ícones renderizados inline (paths em pixel art simples), sem deps
//! novas. State guarda info real do sistema (`/sys/class/power_supply/`
//! pra battery) e o cache dos buffers de cada ícone (chave = string
//! descrevendo o estado pra invalidar).

use std::collections::HashMap;
use std::fs;

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Size},
};

/// Informação corrente da bateria principal do sistema.
#[derive(Debug, Clone, Copy)]
pub struct BatteryInfo {
    /// 0..100.
    pub capacity: u8,
    /// True se Charging / Full / Not charging com fonte AC.
    pub charging: bool,
}

/// Estado da conexão wifi. H33.taskbar.10.fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiState {
    Connected,
    Disconnected,
}

impl Default for WifiState {
    fn default() -> Self {
        WifiState::Disconnected
    }
}

/// Estado do adaptador bluetooth. H33.taskbar.11.new.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothState {
    On,
    Off,
}

impl Default for BluetoothState {
    fn default() -> Self {
        BluetoothState::Off
    }
}

/// Estado do volume do sistema (PipeWire ou ALSA). H33.taskbar.12.fix.
#[derive(Debug, Clone, Copy, Default)]
pub struct VolumeState {
    /// 0..100 (percentual).
    pub level: u8,
    pub muted: bool,
}

/// Estado mutável do system tray. Cacheia buffers por (kind, key) onde
/// `key` distingue estados (ex.: "battery_73" / "battery_15").
#[derive(Debug, Default)]
pub struct TrayState {
    /// Cache do buffer do ícone da battery por chave estável (capacity).
    pub icon_cache: HashMap<String, (MemoryRenderBuffer, Size<i32, Logical>)>,
    /// Cache do label "%" rasterizado por % (key = "<pct>").
    pub pct_label_cache: HashMap<u8, (MemoryRenderBuffer, Size<i32, Logical>)>,
    /// Cache de wifi icons por estado (key=true conectado, false off).
    /// H33.taskbar.10.fix.
    pub wifi_cache: HashMap<bool, (MemoryRenderBuffer, Size<i32, Logical>)>,
    /// Cache de bluetooth icons por estado. H33.taskbar.11.new.
    pub bt_cache: HashMap<bool, (MemoryRenderBuffer, Size<i32, Logical>)>,
    /// Cache de volume icons por estado (mute on/off). H33.taskbar.12.fix.
    pub volume_cache: HashMap<bool, (MemoryRenderBuffer, Size<i32, Logical>)>,
    /// Buffer legacy do volume icon (vai sair em D.12.fix limpo). Mantido
    /// só pra compat de campo até a renderização migrar 100% pro cache.
    pub volume_icon: Option<(MemoryRenderBuffer, Size<i32, Logical>)>,
    /// Buffer do ícone bell (lazy, H33.taskbar.11).
    pub bell_icon: Option<(MemoryRenderBuffer, Size<i32, Logical>)>,
    /// Buffer do notification dot (lazy, H33.taskbar.11).
    pub notif_dot: Option<MemoryRenderBuffer>,
    /// Contador de notificações não lidas. Sempre 0 nesta etapa
    /// (H40 vai conectar com fonte real de notificações).
    pub notif_count: usize,
    /// Timestamp do último polling dos sensores do tray (battery/wifi/
    /// bluetooth/volume). H33.taskbar.9.fix.
    pub last_poll: Option<std::time::Instant>,
    /// Último estado lido da battery (capacity + charging). `None`
    /// significa sistema sem bateria OU ainda não foi lido.
    pub battery_info: Option<BatteryInfo>,
    /// Último estado lido do wifi. H33.taskbar.10.fix.
    pub wifi_state: WifiState,
    /// Último estado lido do bluetooth. H33.taskbar.11.new.
    pub bt_state: BluetoothState,
    /// Último estado lido do volume. H33.taskbar.12.fix.
    pub volume_state: VolumeState,
}

impl TrayState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Re-lê os sensores do tray (battery/wifi/bt/volume) se já passou o
/// `poll_interval_secs` desde o último read. Chamado uma vez por frame
/// no render; é barato quando não passou o intervalo. H33.taskbar.9.fix.
pub fn refresh_if_stale(state: &mut TrayState) {
    let interval = std::time::Duration::from_secs(3);
    let stale = state
        .last_poll
        .map(|t| t.elapsed() >= interval)
        .unwrap_or(true);
    if !stale {
        return;
    }
    state.battery_info = read_battery();
    state.wifi_state = read_wifi();
    state.bt_state = read_bluetooth();
    state.volume_state = read_volume();
    state.last_poll = Some(std::time::Instant::now());
}

/// Lê o volume do default sink via `wpctl get-volume @DEFAULT_AUDIO_SINK@`.
/// Saída típica: "Volume: 0.42" (50%) ou "Volume: 0.42 [MUTED]".
/// Fallback `amixer get Master`. H33.taskbar.12.fix.
pub fn read_volume() -> VolumeState {
    use std::process::Command;
    if let Ok(out) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            let s = s.trim();
            let muted = s.contains("[MUTED]");
            // Pega o primeiro float após "Volume:".
            if let Some(idx) = s.find("Volume:") {
                let rest = &s[idx + "Volume:".len()..];
                let token = rest.split_whitespace().next().unwrap_or("0");
                if let Ok(v) = token.parse::<f32>() {
                    let level = (v * 100.0).round().clamp(0.0, 100.0) as u8;
                    return VolumeState { level, muted };
                }
            }
        }
    }
    // Fallback amixer.
    if let Ok(out) = Command::new("amixer").args(["get", "Master"]).output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            let muted = s.contains("[off]");
            // Procura "[NN%]" no texto.
            for token in s.split_whitespace() {
                let t = token.trim_matches(|c| c == '[' || c == ']');
                if let Some(num) = t.strip_suffix('%') {
                    if let Ok(level) = num.parse::<u8>() {
                        return VolumeState { level, muted };
                    }
                }
            }
        }
    }
    VolumeState { level: 0, muted: false }
}

/// Lê o estado do adaptador bluetooth via `bluetoothctl show`. Procura
/// linha `Powered: yes/no`. Fallback `rfkill list bluetooth` (se nenhum
/// device unblocked, considera off). H33.taskbar.11.new.
pub fn read_bluetooth() -> BluetoothState {
    use std::process::Command;
    if let Ok(out) = Command::new("bluetoothctl").arg("show").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("Powered:") {
                    return if rest.trim() == "yes" {
                        BluetoothState::On
                    } else {
                        BluetoothState::Off
                    };
                }
            }
        }
    }
    // Fallback: rfkill — se Soft blocked == no E Hard blocked == no, on.
    if let Ok(out) = Command::new("rfkill")
        .args(["list", "bluetooth"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            let soft = s.contains("Soft blocked: no");
            let hard = s.contains("Hard blocked: no");
            if soft && hard {
                return BluetoothState::On;
            }
        }
    }
    BluetoothState::Off
}

/// Lê o estado da conexão wifi via `nmcli -t -f DEVICE,TYPE,STATE
/// device`. Procura primeira linha com `:wifi:connected`. Fallback:
/// `nmcli -t -f STATE general`. H33.taskbar.10.fix.
pub fn read_wifi() -> WifiState {
    use std::process::Command;
    // 1) Tenta listar devices wifi.
    if let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE,STATE", "device"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                let cols: Vec<&str> = line.split(':').collect();
                if cols.len() >= 3 && cols[1] == "wifi" {
                    if cols[2] == "connected" {
                        return WifiState::Connected;
                    }
                }
            }
        }
    }
    // 2) Fallback: estado global.
    if let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "STATE", "general"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.trim() == "connected" {
                return WifiState::Connected;
            }
        }
    }
    WifiState::Disconnected
}

/// Lê a battery do sistema. Tenta primeiro a env `LUMA_FAKE_BATTERY`
/// (dev/test em desktops sem bateria). Depois `/sys/class/power_supply/BAT*`.
/// Retorna `None` em máquinas sem bateria. H33.taskbar.9.
pub fn read_battery() -> Option<BatteryInfo> {
    if let Ok(fake) = std::env::var("LUMA_FAKE_BATTERY") {
        if let Ok(cap) = fake.parse::<u8>() {
            let charging = std::env::var("LUMA_FAKE_BATTERY_CHARGING")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            return Some(BatteryInfo {
                capacity: cap.min(100),
                charging,
            });
        }
    }

    let entries = fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_str()?.to_string();
        if !name.starts_with("BAT") {
            continue;
        }
        let capacity = fs::read_to_string(path.join("capacity"))
            .ok()?
            .trim()
            .parse::<u8>()
            .ok()?;
        let status = fs::read_to_string(path.join("status"))
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let charging = matches!(status.as_str(), "Charging" | "Full" | "Not charging");
        return Some(BatteryInfo {
            capacity: capacity.min(100),
            charging,
        });
    }
    None
}

/// Cor de fill da battery baseada no %. Sage > 50, Honey 20-50, Coral < 20.
pub fn battery_fill_color(capacity: u8) -> smithay::backend::renderer::Color32F {
    let t = &crate::theme::current().taskbar.system_tray;
    if capacity > 50 {
        t.battery_fill_high
    } else if capacity >= 20 {
        t.battery_fill_med
    } else {
        t.battery_fill_low
    }
}

/// Gera (ou pega cacheado) o ícone da battery com fill correspondente
/// ao %. Chave do cache = "battery_<pct>_<c|d>". `charging=true` desenha
/// pequeno raio âmbar overlay. H33.taskbar.9 + 9.fix.
pub fn ensure_battery_icon(
    state: &mut TrayState,
    capacity: u8,
    charging: bool,
) -> Option<(MemoryRenderBuffer, Size<i32, Logical>)> {
    let key = format!("battery_{}_{}", capacity, if charging { "c" } else { "d" });
    if let Some(entry) = state.icon_cache.get(&key) {
        return Some((entry.0.clone(), entry.1));
    }
    let theme = crate::theme::current();
    let t = &theme.taskbar.system_tray;
    let fill = battery_fill_color(capacity);
    let frame = smithay::backend::renderer::Color32F::new(
        t.icon_color.r(),
        t.icon_color.g(),
        t.icon_color.b(),
        t.icon_alpha,
    );
    let mark = if charging {
        Some(t.battery_charging_mark)
    } else {
        None
    };
    let buf = crate::gfx::make_battery_icon(t.icon_size, capacity, fill, frame, mark);
    let size = Size::from((t.icon_size, t.icon_size));
    state.icon_cache.insert(key, (buf.clone(), size));
    Some((buf, size))
}

/// Gera (ou pega cacheado) o ícone wifi 20px conforme `connected`.
/// Cor depende do estado: `wifi_color_connected` ou `wifi_color_off`,
/// ambas com `icon_alpha`. H33.taskbar.10 + .10.fix.
pub fn ensure_wifi_icon(
    state: &mut TrayState,
    connected: bool,
) -> (MemoryRenderBuffer, Size<i32, Logical>) {
    if let Some(entry) = state.wifi_cache.get(&connected) {
        return (entry.0.clone(), entry.1);
    }
    let theme = crate::theme::current();
    let t = &theme.taskbar.system_tray;
    let base = if connected {
        t.wifi_color_connected
    } else {
        t.wifi_color_off
    };
    let color = smithay::backend::renderer::Color32F::new(
        base.r(),
        base.g(),
        base.b(),
        t.icon_alpha,
    );
    let buf = crate::gfx::make_wifi_icon(t.icon_size, color);
    let size = Size::from((t.icon_size, t.icon_size));
    state.wifi_cache.insert(connected, (buf.clone(), size));
    (buf, size)
}

/// Gera (ou pega cacheado) o ícone bell 20px. H33.taskbar.11.
pub fn ensure_bell_icon(state: &mut TrayState) -> (MemoryRenderBuffer, Size<i32, Logical>) {
    if let Some(entry) = &state.bell_icon {
        return (entry.0.clone(), entry.1);
    }
    let theme = crate::theme::current();
    let t = &theme.taskbar.system_tray;
    let color = smithay::backend::renderer::Color32F::new(
        t.icon_color.r(),
        t.icon_color.g(),
        t.icon_color.b(),
        t.icon_alpha,
    );
    let buf = crate::gfx::make_bell_icon(t.icon_size, color);
    let size = Size::from((t.icon_size, t.icon_size));
    state.bell_icon = Some((buf.clone(), size));
    (buf, size)
}

/// Gera (ou pega cacheado) o notification dot coral. H33.taskbar.11.
pub fn ensure_notif_dot(state: &mut TrayState) -> MemoryRenderBuffer {
    if let Some(buf) = &state.notif_dot {
        return buf.clone();
    }
    let theme = crate::theme::current();
    let t = &theme.taskbar.system_tray;
    let buf = crate::gfx::make_dot(t.notif_dot_size, t.notif_dot_color);
    state.notif_dot = Some(buf.clone());
    buf
}

/// Gera (ou pega cacheado) o ícone bluetooth 20px conforme estado.
/// `on=true` usa `bt_color_on` (cream), false usa `bt_color_off` (gray).
/// H33.taskbar.11.new.
pub fn ensure_bluetooth_icon(
    state: &mut TrayState,
    on: bool,
) -> (MemoryRenderBuffer, Size<i32, Logical>) {
    if let Some(entry) = state.bt_cache.get(&on) {
        return (entry.0.clone(), entry.1);
    }
    let theme = crate::theme::current();
    let t = &theme.taskbar.system_tray;
    let base = if on { t.bt_color_on } else { t.bt_color_off };
    let color = smithay::backend::renderer::Color32F::new(
        base.r(),
        base.g(),
        base.b(),
        t.icon_alpha,
    );
    let buf = crate::gfx::make_bluetooth_icon(t.icon_size, color);
    let size = Size::from((t.icon_size, t.icon_size));
    state.bt_cache.insert(on, (buf.clone(), size));
    (buf, size)
}

/// Gera (ou pega cacheado) o ícone volume 20px conforme estado mute.
/// `muted=true` desenha traço diagonal sobreposto e usa cor mais sutil.
/// H33.taskbar.10 + 12.fix.
pub fn ensure_volume_icon(
    state: &mut TrayState,
    muted: bool,
) -> (MemoryRenderBuffer, Size<i32, Logical>) {
    if let Some(entry) = state.volume_cache.get(&muted) {
        return (entry.0.clone(), entry.1);
    }
    let theme = crate::theme::current();
    let t = &theme.taskbar.system_tray;
    let base = if muted { t.volume_muted_color } else { t.volume_color };
    let color = smithay::backend::renderer::Color32F::new(
        base.r(),
        base.g(),
        base.b(),
        t.icon_alpha,
    );
    let buf = crate::gfx::make_volume_icon(t.icon_size, color, muted);
    let size = Size::from((t.icon_size, t.icon_size));
    state.volume_cache.insert(muted, (buf.clone(), size));
    (buf, size)
}

/// Rasteriza (ou pega cacheado) o label "<pct>%" pra mostrar ao lado da
/// battery. H33.taskbar.9.
pub fn ensure_pct_label(
    state: &mut TrayState,
    capacity: u8,
    text: &crate::text::TextRenderer,
) -> Option<(MemoryRenderBuffer, Size<i32, Logical>)> {
    if let Some(entry) = state.pct_label_cache.get(&capacity) {
        return Some((entry.0.clone(), entry.1));
    }
    let theme = crate::theme::current();
    let t = &theme.taskbar.system_tray;
    let s = format!("{}%", capacity);
    let (pixels, w, h) = text.rasterize_colored(&s, t.battery_pct_size, t.battery_pct_color);
    if w == 0 || h == 0 {
        return None;
    }
    let buf = MemoryRenderBuffer::from_slice(
        &pixels,
        smithay::backend::allocator::Fourcc::Argb8888,
        (w as i32, h as i32),
        1,
        smithay::utils::Transform::Normal,
        None,
    );
    let size = Size::from((w as i32, h as i32));
    state.pct_label_cache.insert(capacity, (buf.clone(), size));
    Some((buf, size))
}
