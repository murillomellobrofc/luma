//! System tray na zona D da taskbar v5 (H33.taskbar.9-13 + SVG refator).
//!
//! Ícones carregados de `assets/branding/icons/icon-*.svg` via
//! `menu::icon_loader::load_svg_file`. Estado real do sistema lido via
//! /sys, /proc e comandos (`nmcli`, `bluetoothctl`, `wpctl`). Polling
//! a cada `poll_interval_secs` (default 3s).

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

/// Estado da conexão wifi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiState {
    /// Conectado a alguma rede (sinal forte/médio).
    Connected,
    /// Conectado mas sinal fraco (futuro — H33 trata igual a Connected).
    #[allow(dead_code)]
    Weak,
    /// Wifi ligado mas sem rede ativa.
    NoNetwork,
    /// Wifi desligado/rfkill OU não há placa wifi no sistema.
    Off,
}

impl Default for WifiState {
    fn default() -> Self {
        WifiState::Off
    }
}

/// Estado consolidado da rede (ethernet + wifi). H33.taskbar.10.eth.
///
/// Cabo de rede tem prioridade sobre wifi (`network_prefer_ethernet`):
/// se há interface ethernet com `carrier=1`, mostra `ethernet-on`
/// independente do wifi. Senão cai pro wifi normal. `ethernet_present`
/// indica se há ao menos uma placa ethernet — usada pra escolher entre
/// `ethernet-off` e `wifi-off` quando tudo está desconectado.
#[derive(Debug, Clone, Copy, Default)]
pub struct NetworkState {
    pub ethernet_up: bool,
    pub ethernet_present: bool,
    pub wifi: WifiState,
}

/// Estado do adaptador bluetooth.
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

/// Estado do volume do sistema.
#[derive(Debug, Clone, Copy, Default)]
pub struct VolumeState {
    /// 0..100 (percentual).
    pub level: u8,
    pub muted: bool,
}

/// Estado mutável do system tray. Cacheia buffers SVG por nome (key =
/// "battery-high", "wifi-full", etc) e o estado lido dos sensores.
#[derive(Debug, Default)]
pub struct TrayState {
    /// Cache de ícones SVG por nome (sem prefixo "icon-" nem extensão).
    /// Ex: "battery-high", "wifi-off", "bell-normal".
    pub icon_cache: HashMap<String, (MemoryRenderBuffer, Size<i32, Logical>)>,
    /// Cache do label "%" rasterizado por % (key = capacity).
    pub pct_label_cache: HashMap<u8, (MemoryRenderBuffer, Size<i32, Logical>)>,

    /// Timestamp do último polling. None = ainda não foi feito.
    pub last_poll: Option<std::time::Instant>,
    /// Último estado lido da battery (capacity + charging). None se
    /// sistema sem bateria.
    pub battery_info: Option<BatteryInfo>,
    /// Último estado consolidado da rede (ethernet + wifi). H33.taskbar.10.eth.
    pub network: NetworkState,
    /// Último estado lido do bluetooth.
    pub bt_state: BluetoothState,
    /// Último estado lido do volume.
    pub volume_state: VolumeState,
    /// Contador de notificações não lidas. H33 sempre 0 (H40 conecta).
    pub notif_count: usize,
}

impl TrayState {
    pub fn new() -> Self {
        Self::default()
    }
}

// ────────────────────────── Polling ──────────────────────────

/// Re-lê sensores se já passou `poll_interval_secs` desde o último poll.
/// H33.taskbar.13.
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
    state.network = read_network();
    state.bt_state = read_bluetooth();
    state.volume_state = read_volume();
    state.last_poll = Some(std::time::Instant::now());
}

/// Lê estado consolidado de rede: presença/carrier de ethernet em
/// `/sys/class/net/*/` (type=1 ARPHRD_ETHER, não-loopback) + estado
/// wifi via `read_wifi`. H33.taskbar.10.eth.
pub fn read_network() -> NetworkState {
    let (ethernet_up, ethernet_present) = read_ethernet();
    let wifi = read_wifi();
    NetworkState {
        ethernet_up,
        ethernet_present,
        wifi,
    }
}

/// Retorna `(ethernet_up, ethernet_present)`. Iter `/sys/class/net/*/`
/// e filtra type=1 (ARPHRD_ETHER) excluindo `lo`. `ethernet_up` é true
/// se ALGUMA dessas interfaces tem `carrier=1`.
fn read_ethernet() -> (bool, bool) {
    let entries = match fs::read_dir("/sys/class/net") {
        Ok(e) => e,
        Err(_) => return (false, false),
    };
    let mut present = false;
    let mut up = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name == "lo" {
            continue;
        }
        // type=1 = ARPHRD_ETHER (ethernet). wifi reporta 1 também mas
        // o nome geralmente começa com `wl` (wlan, wlp, wlo). Exclui.
        let type_str = fs::read_to_string(path.join("type"))
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if type_str != "1" {
            continue;
        }
        if name.starts_with("wl") || name.starts_with("ww") {
            continue;
        }
        present = true;
        if let Ok(c) = fs::read_to_string(path.join("carrier")) {
            if c.trim() == "1" {
                up = true;
            }
        }
    }
    (up, present)
}

// ────────────────────────── Leituras ──────────────────────────

/// Lê a battery do sistema. Tenta `LUMA_FAKE_BATTERY` env primeiro (dev),
/// depois `/sys/class/power_supply/BAT*`. None em desktops sem bateria.
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

/// Lê estado do wifi via `nmcli`. H33.taskbar.10.
pub fn read_wifi() -> WifiState {
    use std::process::Command;
    if let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE,STATE", "device"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            let mut saw_wifi_device = false;
            for line in s.lines() {
                let cols: Vec<&str> = line.split(':').collect();
                if cols.len() >= 3 && cols[1] == "wifi" {
                    saw_wifi_device = true;
                    match cols[2] {
                        "connected" => return WifiState::Connected,
                        "disconnected" => return WifiState::NoNetwork,
                        "unavailable" => return WifiState::Off,
                        _ => {}
                    }
                }
            }
            if !saw_wifi_device {
                return WifiState::Off;
            }
        }
    }
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
    WifiState::Off
}

/// Lê estado do adaptador bluetooth. H33.taskbar.11.
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

/// Lê o volume do default sink via `wpctl`. Fallback `amixer`.
/// H33.taskbar.12.
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
    if let Ok(out) = Command::new("amixer").args(["get", "Master"]).output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            let muted = s.contains("[off]");
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
    VolumeState {
        level: 0,
        muted: false,
    }
}

// ────────────────────────── Mapping de SVGs ──────────────────────────

/// Nome do SVG da bateria pra um estado dado. H33.taskbar.9.
pub fn battery_icon_name(info: BatteryInfo) -> &'static str {
    if info.charging {
        "battery-charging"
    } else if info.capacity > 50 {
        "battery-high"
    } else if info.capacity >= 20 {
        "battery-medium"
    } else {
        "battery-low"
    }
}

/// Nome do SVG da rede pra um estado dado. Cabo plugado tem prioridade
/// sobre wifi se `network_prefer_ethernet=true`. H33.taskbar.10.eth.
///
/// Prioridade:
/// 1. ethernet up                → `ethernet-on`
/// 2. wifi connected/weak        → `wifi-full` / `wifi-low`
/// 3. wifi no_network            → `wifi-none`
/// 4. tudo off, mas eth presente → `ethernet-off`
/// 5. tudo off, só wifi          → `wifi-off`
pub fn network_icon_name(state: NetworkState) -> &'static str {
    let prefer_eth = crate::theme::current()
        .taskbar
        .system_tray
        .network_prefer_ethernet;
    if prefer_eth && state.ethernet_up {
        return "ethernet-on";
    }
    match state.wifi {
        WifiState::Connected => "wifi-full",
        WifiState::Weak => "wifi-low",
        WifiState::NoNetwork => "wifi-none",
        WifiState::Off => {
            if state.ethernet_present {
                "ethernet-off"
            } else {
                "wifi-off"
            }
        }
    }
}

/// Nome do SVG do bluetooth pra um estado dado. H33.taskbar.11.
pub fn bluetooth_icon_name(state: BluetoothState) -> &'static str {
    match state {
        BluetoothState::On => "bluetooth-on",
        BluetoothState::Off => "bluetooth-off",
    }
}

/// Nome do SVG do volume pra um estado dado. H33.taskbar.12.
pub fn volume_icon_name(state: VolumeState) -> &'static str {
    if state.muted || state.level == 0 {
        "volume-muted"
    } else if state.level > 66 {
        "volume-high"
    } else if state.level > 32 {
        "volume-medium"
    } else {
        "volume-low"
    }
}

/// Nome do SVG do sino. H33 sempre "bell-normal" (H40 conecta).
pub fn bell_icon_name(_count: usize) -> &'static str {
    "bell-normal"
}

// ────────────────────────── Cache + load ──────────────────────────

/// Carrega (ou pega cacheado) o SVG `icon-<name>.svg` do `icons_path`
/// do tema, em `icon_size`. Retorna `None` se o arquivo não existe ou
/// se houve erro de parse. H33.taskbar.13.
pub fn ensure_icon(
    state: &mut TrayState,
    name: &str,
) -> Option<(MemoryRenderBuffer, Size<i32, Logical>)> {
    if let Some(entry) = state.icon_cache.get(name) {
        return Some((entry.0.clone(), entry.1));
    }
    let theme = crate::theme::current();
    let t = &theme.taskbar.system_tray;
    let path = format!("{}/icon-{}.svg", t.icons_path, name);
    let buf = crate::menu::icon_loader::load_svg_file(&path, t.icon_size as u32)?;
    let size = Size::from((t.icon_size, t.icon_size));
    state.icon_cache.insert(name.to_string(), (buf.clone(), size));
    Some((buf, size))
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
