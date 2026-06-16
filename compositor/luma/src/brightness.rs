//! Controle de brilho com backend degradado. H45.osd.
//!
//! Estratégia: detectar o melhor backend disponível **uma vez** (lazy
//! via OnceLock) e reusar em todas as chamadas. Se nada estiver
//! disponível, vira no-op silencioso — o OSD de brilho fica inativo
//! mas o caminho de input continua funcionando.
//!
//! Ordem de prioridade:
//! 1. `/sys/class/backlight/<dev>/brightness` (+ `max_brightness`)
//!    — caminho direto, mas escrita exige udev rule pra usuário sem
//!    root. Tipicamente disponível em laptops com painel interno.
//! 2. `brightnessctl set N%` se instalado — wrapper userland que
//!    cuida da permissão. Cobre o mesmo hardware que (1).
//! 3. `ddcutil setvcp 10 N` — controle DDC/CI pra monitores externos
//!    via I²C. Útil pra desktops.
//! 4. Nenhum → no-op. Logado uma vez.
//!
//! Na máquina atual de dev (desktop sem backlight, sem ferramentas
//! instaladas) o backend resolve pra Noop e o OSD de brilho fica
//! pronto pra ligar sozinho quando ddcutil/brightnessctl forem
//! instalados.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
enum Backend {
    Sysfs {
        brightness: PathBuf,
        max: u32,
    },
    Brightnessctl,
    Ddcutil,
    Noop,
}

static BACKEND: OnceLock<Backend> = OnceLock::new();

fn backend() -> &'static Backend {
    BACKEND.get_or_init(|| {
        let b = probe();
        match &b {
            Backend::Sysfs { brightness, max } => {
                tracing::info!(
                    "brightness backend: sysfs ({}, max={})",
                    brightness.display(),
                    max
                );
            }
            Backend::Brightnessctl => tracing::info!("brightness backend: brightnessctl"),
            Backend::Ddcutil => tracing::info!("brightness backend: ddcutil"),
            Backend::Noop => tracing::info!(
                "brightness backend: noop (sem /sys/class/backlight, sem brightnessctl/ddcutil)"
            ),
        }
        b
    })
}

fn probe() -> Backend {
    // 1. Sysfs: primeiro device em /sys/class/backlight/.
    if let Ok(rd) = std::fs::read_dir("/sys/class/backlight") {
        for entry in rd.flatten() {
            let p = entry.path();
            let max_path = p.join("max_brightness");
            let bright_path = p.join("brightness");
            if let (Ok(max_str), true) = (std::fs::read_to_string(&max_path), bright_path.exists())
            {
                if let Ok(max) = max_str.trim().parse::<u32>() {
                    if max > 0 {
                        return Backend::Sysfs {
                            brightness: bright_path,
                            max,
                        };
                    }
                }
            }
        }
    }
    // 2. brightnessctl.
    if which("brightnessctl") {
        return Backend::Brightnessctl;
    }
    // 3. ddcutil (controle DDC/CI de monitores externos).
    if which("ddcutil") {
        return Backend::Ddcutil;
    }
    Backend::Noop
}

fn which(bin: &str) -> bool {
    Command::new("which")
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Retorna o valor atual de brilho em `0.0..=1.0`. `None` se o backend
/// for noop ou se a leitura falhar.
pub fn current() -> Option<f32> {
    match backend() {
        Backend::Sysfs { brightness, max } => {
            let s = std::fs::read_to_string(brightness).ok()?;
            let v: u32 = s.trim().parse().ok()?;
            Some((v as f32 / *max as f32).clamp(0.0, 1.0))
        }
        Backend::Brightnessctl => {
            let out = Command::new("brightnessctl").arg("get").output().ok()?;
            let cur: f32 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
            let max = Command::new("brightnessctl")
                .arg("max")
                .output()
                .ok()
                .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f32>().ok())?;
            if max <= 0.0 {
                return None;
            }
            Some((cur / max).clamp(0.0, 1.0))
        }
        Backend::Ddcutil => {
            // ddcutil getvcp 10 retorna algo como "VCP code 0x10 (Brightness): current value = 75, max value = 100".
            let out = Command::new("ddcutil").args(["getvcp", "10"]).output().ok()?;
            let s = String::from_utf8_lossy(&out.stdout);
            let cur_idx = s.find("current value =")?;
            let rest = &s[cur_idx + "current value =".len()..];
            let cur: f32 = rest.split(|c: char| !c.is_ascii_digit()).find(|t| !t.is_empty())?.parse().ok()?;
            let max_idx = s.find("max value =")?;
            let rest = &s[max_idx + "max value =".len()..];
            let max: f32 = rest.split(|c: char| !c.is_ascii_digit()).find(|t| !t.is_empty())?.parse().ok()?;
            if max <= 0.0 {
                return None;
            }
            Some((cur / max).clamp(0.0, 1.0))
        }
        Backend::Noop => None,
    }
}

/// Define o brilho em `0.0..=1.0`. Retorna `false` se o backend for
/// noop ou se a escrita falhar.
pub fn set(value: f32) -> bool {
    let v = value.clamp(0.0, 1.0);
    match backend() {
        Backend::Sysfs { brightness, max } => {
            let n = (v * *max as f32).round() as u32;
            std::fs::write(brightness, n.to_string()).is_ok()
        }
        Backend::Brightnessctl => {
            let pct = (v * 100.0).round() as u32;
            Command::new("brightnessctl")
                .args(["set", &format!("{}%", pct)])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
        Backend::Ddcutil => {
            let pct = (v * 100.0).round() as u32;
            Command::new("ddcutil")
                .args(["setvcp", "10", &pct.to_string()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
        Backend::Noop => false,
    }
}

/// Soma `delta` (clamp `0.0..=1.0`) ao valor atual e aplica. Retorna o
/// novo valor, ou `None` se o backend for noop.
pub fn step(delta: f32) -> Option<f32> {
    let cur = current()?;
    let next = (cur + delta).clamp(0.0, 1.0);
    if set(next) {
        Some(next)
    } else {
        None
    }
}

/// `true` se há algum backend ativo. Usado pra decidir se a tecla
/// XF86_MonBrightness* faz algo ou só ignora.
pub fn available() -> bool {
    !matches!(backend(), Backend::Noop)
}
