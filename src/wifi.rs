//! Cliente Wi-Fi via spawn de `nmcli` (NetworkManager CLI).
//! H57.2. Mesmo padrão de outras integrações externas (wpctl pra
//! PipeWire, kdialog pra wallpaper). Sem dep nova — nmcli é
//! padrão em qualquer distro com NetworkManager.
//!
//! Funções síncronas (read_state pinta um quadro do estado atual,
//! ~50ms). Conectar dispara o nmcli em thread separada pra não
//! travar o main loop.

use std::process::Command;

/// Rede Wi-Fi visível.
#[derive(Debug, Clone)]
pub struct WifiNetwork {
    pub ssid: String,
    /// 0..100.
    pub signal: u8,
    /// "" (open) ou "WPA2", "WPA3", etc.
    pub security: String,
    /// `true` quando é a rede atualmente conectada.
    pub in_use: bool,
}

impl WifiNetwork {
    pub fn is_protected(&self) -> bool {
        !self.security.is_empty() && self.security != "--"
    }
}

/// Snapshot do estado do wifi.
#[derive(Debug, Clone, Default)]
pub struct WifiState {
    /// `true` se o rádio está ligado. False = desligado por software
    /// (rfkill ou nmcli radio off) OU sem placa wifi.
    pub enabled: bool,
    /// `true` se o host tem alguma placa wifi reconhecida pelo NM.
    /// Quando false E enabled false, o applet deveria ESCONDER o
    /// ícone do tray inteiro (per spec H57).
    pub has_hardware: bool,
    pub networks: Vec<WifiNetwork>,
}

/// Lê estado completo via nmcli. Retorno é snapshot — chamador
/// armazena e re-chama em refresh.
pub fn read_state() -> WifiState {
    let enabled = matches!(
        Command::new("nmcli")
            .args(["radio", "wifi"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .as_deref()
            .map(str::trim),
        Some("enabled")
    );
    // Detecta placa via `nmcli -t -f DEVICE,TYPE device`.
    let has_hardware = Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE", "device"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().any(|l| l.split(':').nth(1) == Some("wifi")))
        .unwrap_or(false);
    let networks = if enabled && has_hardware {
        list_networks()
    } else {
        Vec::new()
    };
    WifiState {
        enabled,
        has_hardware,
        networks,
    }
}

/// Lista redes via `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY device wifi list`.
/// Formato `-t`: campos por `:`, valores com `:` escapados como `\:`.
fn list_networks() -> Vec<WifiNetwork> {
    let out = Command::new("nmcli")
        .args(["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list"])
        .output();
    let out = match out {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let text = match String::from_utf8(out.stdout) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut nets: Vec<WifiNetwork> = Vec::new();
    for line in text.lines() {
        let fields = split_nmcli_t(line);
        if fields.len() < 4 {
            continue;
        }
        let in_use = fields[0] == "*";
        let ssid = fields[1].clone();
        if ssid.is_empty() {
            continue; // rede oculta — fora de escopo (DEBT)
        }
        let signal: u8 = fields[2].parse().unwrap_or(0);
        let security = fields[3].clone();
        nets.push(WifiNetwork {
            ssid,
            signal,
            security,
            in_use,
        });
    }
    // Dedup por SSID: nmcli pode listar múltiplos APs com mesmo
    // SSID; ficamos com o mais forte (e que esteja IN-USE se
    // existir).
    nets.sort_by(|a, b| {
        b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal))
    });
    let mut seen: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    nets.retain(|n| seen.insert(n.ssid.clone()));
    nets
}

/// Split de uma linha do nmcli `-t`: campos por `:` com `\:` literal.
fn split_nmcli_t(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&nxt) = chars.peek() {
                cur.push(nxt);
                chars.next();
                continue;
            }
        }
        if c == ':' {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

/// Liga/desliga o rádio wifi via `nmcli radio wifi on|off`.
/// Spawn fire-and-forget.
pub fn set_enabled(on: bool) {
    let arg = if on { "on" } else { "off" };
    let _ = Command::new("nmcli")
        .args(["radio", "wifi", arg])
        .spawn();
    tracing::info!("wifi: radio {arg}");
}

/// Dispara um rescan (sem aguardar resultado).
pub fn request_scan() {
    let _ = Command::new("nmcli")
        .args(["device", "wifi", "rescan"])
        .spawn();
}

/// Outcome de uma tentativa de conexão (notificada pela thread
/// quando termina). Usado pra mostrar erro inline no popup.
#[derive(Debug, Clone)]
pub enum ConnectResult {
    Success { ssid: String },
    Failure { ssid: String, message: String },
}

/// Conecta a uma rede em uma thread separada. O outcome chega no
/// `outbox` quando o nmcli termina (pode levar 10s+). Main loop
/// observa o slot a cada frame.
///
/// password=None pra rede aberta OU pra rede já com perfil salvo.
/// password=Some(p) pra rede protegida nova.
pub fn connect_async(
    outbox: std::sync::Arc<std::sync::Mutex<Option<ConnectResult>>>,
    ssid: String,
    password: Option<String>,
) {
    std::thread::Builder::new()
        .name("luma-wifi-connect".into())
        .spawn(move || {
            let mut cmd = Command::new("nmcli");
            cmd.args(["device", "wifi", "connect", &ssid]);
            if let Some(pw) = password.as_ref() {
                cmd.args(["password", pw]);
            }
            let result = cmd.output();
            let outcome = match result {
                Ok(o) if o.status.success() => ConnectResult::Success {
                    ssid: ssid.clone(),
                },
                Ok(o) => ConnectResult::Failure {
                    ssid: ssid.clone(),
                    message: String::from_utf8_lossy(&o.stderr)
                        .lines()
                        .next()
                        .unwrap_or("falha")
                        .to_string(),
                },
                Err(e) => ConnectResult::Failure {
                    ssid: ssid.clone(),
                    message: format!("{e}"),
                },
            };
            tracing::info!("wifi connect outcome: {:?}", outcome);
            if let Ok(mut g) = outbox.lock() {
                *g = Some(outcome);
            }
        })
        .ok();
}
