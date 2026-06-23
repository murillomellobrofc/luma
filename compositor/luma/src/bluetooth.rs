//! Cliente Bluetooth via spawn de `bluetoothctl` (BlueZ).
//! H57.3. Mesmo padrão do wifi (nmcli wrapper).
//!
//! Just Works pairing apenas — devices que pedem PIN/passkey
//! precisam de um agent BlueZ registrado, que é peça à parte
//! (anotada em docs/DEBT.md).

use std::process::Command;

/// Categoria de ícone derivada da propriedade Icon do BlueZ. Usado
/// pra escolher o glyph na render row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtIconKind {
    Audio,
    Keyboard,
    Mouse,
    Phone,
    Generic,
}

impl BtIconKind {
    /// Glyph unicode genérico (fallback até pack de SVG-specific por
    /// tipo entrar). Mantém o popup funcional sem dep visual nova.
    pub fn glyph(self) -> &'static str {
        match self {
            BtIconKind::Audio => "🎧",
            BtIconKind::Keyboard => "⌨",
            BtIconKind::Mouse => "🖱",
            BtIconKind::Phone => "📱",
            BtIconKind::Generic => "▣",
        }
    }
    fn from_bluez(icon: &str) -> Self {
        match icon {
            "audio-headset" | "audio-headphones" | "audio-card" => BtIconKind::Audio,
            "input-keyboard" => BtIconKind::Keyboard,
            "input-mouse" | "input-tablet" => BtIconKind::Mouse,
            "phone" => BtIconKind::Phone,
            _ => BtIconKind::Generic,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BtDevice {
    pub mac: String,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
    pub icon_kind: BtIconKind,
}

#[derive(Debug, Clone, Default)]
pub struct BluetoothState {
    /// `true` se o controller está Powered:yes.
    pub enabled: bool,
    /// `true` se há um controller (independente de powered).
    pub has_hardware: bool,
    /// Devices pareados (sempre). Quando scanning=true, inclui
    /// também os descobertos próximos (não-pareados).
    pub devices: Vec<BtDevice>,
}

/// Nome do ícone do estado, pro applet da barra (luma-bar). Leve: só roda
/// `bluetoothctl show` (sem enumerar devices). Mapeia pro SVG de assets:
/// sem controller → "" (esconde); Powered:yes → "bluetooth-on"; senão
/// "bluetooth-off". (Só há on/off nos assets — "conectado" usa on.)
pub fn icon_name() -> &'static str {
    match Command::new("bluetoothctl").arg("show").output() {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            if !text.contains("Controller ") {
                return "";
            }
            if text.lines().any(|l| l.trim().starts_with("Powered: yes")) {
                "bluetooth-on"
            } else {
                "bluetooth-off"
            }
        }
        Err(_) => "",
    }
}

/// Lê estado do controller default + lista de devices. Se
/// `include_unpaired=true`, lista TODOS conhecidos (paired +
/// descobertos via scan). Se false, só os pareados.
pub fn read_state(include_unpaired: bool) -> BluetoothState {
    let show_out = Command::new("bluetoothctl").arg("show").output();
    let (has_hardware, enabled) = match show_out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            let has_hw = text.contains("Controller ");
            let on = text
                .lines()
                .any(|l| l.trim().starts_with("Powered: yes"));
            (has_hw, on)
        }
        Err(_) => (false, false),
    };
    let devices = if enabled && has_hardware {
        list_devices(include_unpaired)
    } else {
        Vec::new()
    };
    BluetoothState {
        enabled,
        has_hardware,
        devices,
    }
}

/// Lista devices via `bluetoothctl devices [Paired]`. Pra cada um,
/// roda `bluetoothctl info MAC` pra checar Connected + Icon.
/// Caro (1+N spawns por refresh) mas refresh é raro (popup aberto).
fn list_devices(include_unpaired: bool) -> Vec<BtDevice> {
    let mut args: Vec<&str> = vec!["devices"];
    if !include_unpaired {
        args.push("Paired");
    }
    let out = match Command::new("bluetoothctl").args(&args).output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let text = match String::from_utf8(out.stdout) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut devices: Vec<BtDevice> = Vec::new();
    for line in text.lines() {
        // Formato: "Device XX:XX:XX:XX:XX:XX Nome com espaços"
        let rest = match line.strip_prefix("Device ") {
            Some(r) => r,
            None => continue,
        };
        let mut parts = rest.splitn(2, ' ');
        let mac = match parts.next() {
            Some(m) => m.to_string(),
            None => continue,
        };
        let name = parts.next().unwrap_or("").to_string();
        // Per-device info: connected + icon.
        let (connected, paired, icon_kind) = read_device_info(&mac);
        devices.push(BtDevice {
            mac,
            name,
            connected,
            paired,
            icon_kind,
        });
    }
    // Pareados primeiro; conectados no topo.
    devices.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    devices
}

fn read_device_info(mac: &str) -> (bool, bool, BtIconKind) {
    let out = match Command::new("bluetoothctl")
        .args(["info", mac])
        .output()
    {
        Ok(o) => o,
        Err(_) => return (false, false, BtIconKind::Generic),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut connected = false;
    let mut paired = false;
    let mut icon_kind = BtIconKind::Generic;
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("Connected: ") {
            connected = v == "yes";
        } else if let Some(v) = l.strip_prefix("Paired: ") {
            paired = v == "yes";
        } else if let Some(v) = l.strip_prefix("Icon: ") {
            icon_kind = BtIconKind::from_bluez(v);
        }
    }
    (connected, paired, icon_kind)
}

/// Conecta um dispositivo pela MAC (fire-and-forget num thread que reapeia o
/// processo, sem zumbi). Pode levar segundos; o popup relê depois.
pub fn connect(mac: &str) {
    let mac = mac.to_string();
    std::thread::spawn(move || {
        let _ = Command::new("bluetoothctl").args(["connect", &mac]).status();
    });
}

/// Desconecta um dispositivo pela MAC (idem).
pub fn disconnect(mac: &str) {
    let mac = mac.to_string();
    std::thread::spawn(move || {
        let _ = Command::new("bluetoothctl").args(["disconnect", &mac]).status();
    });
}

/// Path D-Bus do device pela MAC: `<adapter>/dev_AA_BB_..`. None se sem adapter.
fn device_path(mac: &str) -> Option<String> {
    let adapter = adapter_path()?;
    Some(format!("{}/dev_{}", adapter, mac.replace(':', "_")))
}

/// Pareia um device via `Device1.Pair()` (usa o nosso agent default pro código)
/// + trust + connect no sucesso. Roda num thread (Pair bloqueia até a conversa
/// com o agent terminar). Atualiza a fase no `slot` pro popup acompanhar.
pub fn pair(mac: &str, slot: crate::bt_agent::PairingSlot) {
    let mac = mac.to_string();
    std::thread::Builder::new()
        .name("luma-bt-pair".into())
        .spawn(move || {
            slot.lock().unwrap().phase = crate::bt_agent::Phase::Pairing;
            let Some(path) = device_path(&mac) else {
                slot.lock().unwrap().phase =
                    crate::bt_agent::Phase::Failed("sem adapter".into());
                return;
            };
            let conn = match zbus::blocking::Connection::system() {
                Ok(c) => c,
                Err(e) => {
                    slot.lock().unwrap().phase = crate::bt_agent::Phase::Failed(format!("{e}"));
                    return;
                }
            };
            let dev = match zbus::blocking::Proxy::new(&conn, "org.bluez", path, "org.bluez.Device1")
            {
                Ok(p) => p,
                Err(e) => {
                    slot.lock().unwrap().phase = crate::bt_agent::Phase::Failed(format!("{e}"));
                    return;
                }
            };
            match dev.call::<_, _, ()>("Pair", &()) {
                Ok(()) => {
                    let _ = dev.set_property("Trusted", true);
                    let _ = dev.call::<_, _, ()>("Connect", &());
                    slot.lock().unwrap().phase = crate::bt_agent::Phase::Done;
                    tracing::info!("bt pair ok: {mac}");
                }
                Err(e) => {
                    let msg = e.to_string();
                    tracing::warn!("bt pair falhou {mac}: {msg}");
                    slot.lock().unwrap().phase = crate::bt_agent::Phase::Failed(msg);
                }
            }
        })
        .ok();
}

/// Liga/desliga o controller.
pub fn set_enabled(on: bool) {
    let arg = if on { "on" } else { "off" };
    let _ = Command::new("bluetoothctl")
        .args(["power", arg])
        .spawn();
    tracing::info!("bluetooth: power {arg}");
}

/// Liga/desliga o discovery via Adapter1, na **conexão persistente** passada
/// (o do agent). BlueZ amarra Start/StopDiscovery à conexão que pediu — por
/// isso precisa ser a MESMA, não uma nova a cada chamada. Async (roda na
/// runtime do D-Bus, sem blocking).
pub async fn set_discovery(conn: &zbus::Connection, on: bool) -> zbus::Result<()> {
    use zbus::zvariant::OwnedObjectPath;
    let om = zbus::Proxy::new(conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager").await?;
    let managed: std::collections::HashMap<
        OwnedObjectPath,
        std::collections::HashMap<
            String,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        >,
    > = om.call("GetManagedObjects", &()).await?;
    let Some(adapter) = managed
        .iter()
        .find(|(_, ifaces)| ifaces.contains_key("org.bluez.Adapter1"))
        .map(|(p, _)| p.clone())
    else {
        return Ok(());
    };
    let ad = zbus::Proxy::new(conn, "org.bluez", &adapter, "org.bluez.Adapter1").await?;
    let method = if on { "StartDiscovery" } else { "StopDiscovery" };
    ad.call::<_, _, ()>(method, &()).await?;
    tracing::info!("bluetooth: {method} ok");
    Ok(())
}

/// Liga/desliga discovery (versão blocking pro popup legado do compositor).
/// O novo popup da luma-bar usa `set_discovery` na conexão persistente.
pub fn scan_on() {
    if let Some(path) = adapter_path() {
        let _ = blocking_adapter_call(&path, "StartDiscovery");
    }
}

pub fn scan_off() {
    if let Some(path) = adapter_path() {
        let _ = blocking_adapter_call(&path, "StopDiscovery");
    }
}

fn blocking_adapter_call(path: &str, method: &str) -> Result<(), zbus::Error> {
    let conn = zbus::blocking::Connection::system()?;
    let proxy = zbus::blocking::Proxy::new(&conn, "org.bluez", path, "org.bluez.Adapter1")?;
    proxy.call::<_, _, ()>(method, &())?;
    Ok(())
}

/// Encontra o path do adapter default via ObjectManager. Usualmente
/// /org/bluez/hci0. Retorna None se BlueZ não responde.
fn adapter_path() -> Option<String> {
    use zbus::zvariant::OwnedObjectPath;
    let conn = zbus::blocking::Connection::system().ok()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )
    .ok()?;
    // GetManagedObjects retorna a{oa{sa{sv}}}.
    let result: std::collections::HashMap<
        OwnedObjectPath,
        std::collections::HashMap<
            String,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        >,
    > = proxy.call("GetManagedObjects", &()).ok()?;
    for (path, ifaces) in result.iter() {
        if ifaces.contains_key("org.bluez.Adapter1") {
            return Some(path.as_str().to_string());
        }
    }
    None
}

/// Outcome do connect/pair quando o command termina (pode levar
/// alguns segundos).
#[derive(Debug, Clone)]
pub enum BtResult {
    Success { mac: String, op: BtOp },
    Failure { mac: String, op: BtOp, message: String },
}

#[derive(Debug, Clone, Copy)]
pub enum BtOp {
    Connect,
    Disconnect,
    Pair,
}

pub fn connect_async(
    outbox: std::sync::Arc<std::sync::Mutex<Option<BtResult>>>,
    mac: String,
) {
    run_op(outbox, mac, BtOp::Connect, &["connect"])
}

pub fn disconnect_async(
    outbox: std::sync::Arc<std::sync::Mutex<Option<BtResult>>>,
    mac: String,
) {
    run_op(outbox, mac, BtOp::Disconnect, &["disconnect"])
}

/// Just Works pairing: bluetoothctl pair MAC + auto-trust + connect.
pub fn pair_async(
    outbox: std::sync::Arc<std::sync::Mutex<Option<BtResult>>>,
    mac: String,
) {
    let outbox_p = outbox.clone();
    std::thread::Builder::new()
        .name("luma-bt-pair".into())
        .spawn(move || {
            let pair_result = Command::new("bluetoothctl")
                .args(["pair", &mac])
                .output();
            // Trust + connect só se pair sucedeu.
            let outcome = match pair_result {
                Ok(o) if o.status.success() => {
                    let _ = Command::new("bluetoothctl")
                        .args(["trust", &mac])
                        .output();
                    let _ = Command::new("bluetoothctl")
                        .args(["connect", &mac])
                        .output();
                    BtResult::Success {
                        mac: mac.clone(),
                        op: BtOp::Pair,
                    }
                }
                Ok(o) => BtResult::Failure {
                    mac: mac.clone(),
                    op: BtOp::Pair,
                    message: String::from_utf8_lossy(&o.stderr)
                        .lines()
                        .next()
                        .unwrap_or("falha ao parear")
                        .to_string(),
                },
                Err(e) => BtResult::Failure {
                    mac: mac.clone(),
                    op: BtOp::Pair,
                    message: format!("{e}"),
                },
            };
            tracing::info!("bt pair outcome: {:?}", outcome);
            if let Ok(mut g) = outbox_p.lock() {
                *g = Some(outcome);
            }
        })
        .ok();
}

fn run_op(
    outbox: std::sync::Arc<std::sync::Mutex<Option<BtResult>>>,
    mac: String,
    op: BtOp,
    cmd_args: &[&str],
) {
    let mac_clone = mac.clone();
    let args: Vec<String> = cmd_args.iter().map(|s| s.to_string()).collect();
    std::thread::Builder::new()
        .name("luma-bt-op".into())
        .spawn(move || {
            let mut cmd = Command::new("bluetoothctl");
            for a in args.iter() {
                cmd.arg(a);
            }
            cmd.arg(&mac_clone);
            let result = cmd.output();
            let outcome = match result {
                Ok(o) if o.status.success() => BtResult::Success {
                    mac: mac_clone.clone(),
                    op,
                },
                Ok(o) => BtResult::Failure {
                    mac: mac_clone.clone(),
                    op,
                    message: String::from_utf8_lossy(&o.stderr)
                        .lines()
                        .next()
                        .unwrap_or("falha")
                        .to_string(),
                },
                Err(e) => BtResult::Failure {
                    mac: mac_clone.clone(),
                    op,
                    message: format!("{e}"),
                },
            };
            tracing::info!("bt op outcome: {:?}", outcome);
            if let Ok(mut g) = outbox.lock() {
                *g = Some(outcome);
            }
        })
        .ok();
}
