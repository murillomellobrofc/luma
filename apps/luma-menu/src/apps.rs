//! Descoberta de apps `.desktop` + lançamento, pro luma-menu. Porta da
//! lógica do compositor (`compositor/luma/src/apps.rs`), mas desacoplada do
//! smithay: ícone vira um **path** (o iced rende SVG/PNG direto), não buffer.
//! Varre as pastas XDG, lê Name (prefere pt-BR)/Icon/Exec, pula
//! NoDisplay/Hidden, tira os field codes do Exec.

use std::{collections::HashMap, fs, path::PathBuf};

/// Tipo de ícone resolvido, pro view escolher o widget (svg vs imagem).
#[derive(Debug, Clone)]
pub enum IconKind {
    Svg(PathBuf),
    Raster(PathBuf),
}

/// App descoberto de um `.desktop`.
#[derive(Debug, Clone)]
pub struct App {
    pub name: String,
    /// Comando do `Exec`, sem placeholders `%u`/`%f`/etc.
    pub exec: String,
    /// Nome cru do `Icon=` (mantido pra casar favoritos).
    pub icon: Option<String>,
    /// Ícone resolvido pra um arquivo (best-effort), ou `None`.
    pub icon_path: Option<IconKind>,
}

/// Carrega todos os apps visíveis, em ordem alfabética, com ícone resolvido.
pub fn load() -> Vec<App> {
    let mut apps: Vec<App> = Vec::new();
    for dir in app_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            if let Some(app) = parse(&path) {
                apps.push(app);
            }
        }
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.name == b.name);
    // Resolve o ícone de cada um (uma vez; o índice é lazy + O(1) no lookup).
    for app in &mut apps {
        app.icon_path = app
            .icon
            .as_deref()
            .and_then(resolve_icon_path)
            .map(classify_icon);
    }
    apps
}

/// Um punhado pra "Mais usados": os favoritos curados que existem no sistema,
/// na ordem dos grupos. Contagem de uso real fica pra refino depois.
pub fn favorites(apps: &[App], max: usize) -> Vec<App> {
    let groups: &[&[&str]] = &[
        &["firefox"],
        &["brave", "brave-browser"],
        &["nautilus", "files", "dolphin", "thunar", "nemo"],
        &[
            "gnome-terminal",
            "konsole",
            "kgx",
            "kitty",
            "foot",
            "alacritty",
            "wezterm",
            "tilix",
        ],
        &["code", "vscode", "vscodium", "code-oss"],
        &["gnome-control-center", "systemsettings", "settings"],
        &["gnome-calculator", "calculator", "kcalc"],
        &["gedit", "kate", "text-editor", "geany"],
    ];

    let mut out: Vec<App> = Vec::new();
    for group in groups {
        if out.len() >= max {
            break;
        }
        if let Some(app) = apps.iter().find(|a| matches_any(a, group)) {
            if !out.iter().any(|o| o.name == app.name) {
                out.push(app.clone());
            }
        }
    }
    // Completa com os primeiros alfabéticos se faltou favorito conhecido.
    for app in apps {
        if out.len() >= max {
            break;
        }
        if !out.iter().any(|o| o.name == app.name) {
            out.push(app.clone());
        }
    }
    out
}

fn matches_any(app: &App, patterns: &[&str]) -> bool {
    let name = app.name.to_lowercase();
    let icon = app.icon.as_deref().unwrap_or("").to_lowercase();
    let exec_first = app
        .exec
        .split_whitespace()
        .next()
        .map(|s| s.rsplit('/').next().unwrap_or(s).to_lowercase())
        .unwrap_or_default();
    patterns
        .iter()
        .any(|p| name == *p || icon == *p || exec_first == *p)
}

/// Pastas XDG de apps, em ordem de prioridade (user → sistema).
fn app_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let push = |d: PathBuf, dirs: &mut Vec<PathBuf>| {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    };
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        push(PathBuf::from(data_home).join("applications"), &mut dirs);
    } else if let Some(home) = std::env::var_os("HOME") {
        push(
            PathBuf::from(home).join(".local/share/applications"),
            &mut dirs,
        );
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for d in data_dirs.split(':') {
        if d.is_empty() {
            continue;
        }
        push(PathBuf::from(d).join("applications"), &mut dirs);
    }
    dirs
}

fn parse(path: &std::path::Path) -> Option<App> {
    let content = fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut type_field: Option<String> = None;
    let mut name: Option<String> = None;
    let mut name_pt: Option<String> = None;
    let mut name_ptbr: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut icon: Option<String> = None;
    let mut no_display = false;
    let mut hidden = false;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "Type" => type_field = Some(val.to_string()),
                "Name" if name.is_none() => name = Some(val.to_string()),
                "Name[pt_BR]" => name_ptbr = Some(val.to_string()),
                "Name[pt]" if name_pt.is_none() => name_pt = Some(val.to_string()),
                "Exec" if exec.is_none() => exec = Some(strip_field_codes(val)),
                "Icon" if icon.is_none() => icon = Some(val.to_string()),
                "NoDisplay" => no_display = val.eq_ignore_ascii_case("true"),
                "Hidden" => hidden = val.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
    }

    if no_display || hidden {
        return None;
    }
    if type_field.as_deref() != Some("Application") {
        return None;
    }
    // Prefere pt-BR > pt > Name padrão.
    let name = name_ptbr.or(name_pt).or(name)?;
    let exec = exec?;
    if exec.trim().is_empty() {
        return None;
    }
    Some(App {
        name,
        exec,
        icon,
        icon_path: None,
    })
}

/// Remove field codes do Exec (`%u`, `%U`, `%f`, `%F`, `%i`, `%c`, `%k`…).
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(&next) = chars.peek() {
                if matches!(
                    next,
                    'u' | 'U' | 'f' | 'F' | 'i' | 'c' | 'k' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm'
                ) {
                    chars.next();
                    continue;
                }
                if next == '%' {
                    chars.next();
                    out.push('%');
                    continue;
                }
            }
        }
        out.push(c);
    }
    out.trim().to_string()
}

fn classify_icon(path: PathBuf) -> IconKind {
    if path.extension().and_then(|e| e.to_str()) == Some("svg") {
        IconKind::Svg(path)
    } else {
        IconKind::Raster(path)
    }
}

// ─── Resolução de ícone (índice global, scan único lazy) ───────────────

struct IconIndex {
    entries: HashMap<String, PathBuf>,
}

static ICON_INDEX: std::sync::OnceLock<IconIndex> = std::sync::OnceLock::new();

/// Resolve o `Icon=` consultando o índice (path absoluto vence; senão lookup
/// O(1) por stem). best-effort: `None` se não achar.
pub fn resolve_icon_path(icon: &str) -> Option<PathBuf> {
    let p = PathBuf::from(icon);
    if p.is_absolute() && p.exists() && is_supported_ext(&p) {
        return Some(p);
    }
    ICON_INDEX
        .get_or_init(IconIndex::build)
        .entries
        .get(icon)
        .cloned()
}

fn is_supported_ext(p: &std::path::Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("png") | Some("jpg") | Some("jpeg") | Some("svg")
    )
}

impl IconIndex {
    fn build() -> IconIndex {
        let mut entries: HashMap<String, PathBuf> = HashMap::new();
        for base in theme_base_dirs() {
            for theme in THEME_PRIORITY {
                let theme_dir = base.join(theme);
                if !theme_dir.is_dir() {
                    continue;
                }
                index_two_levels(&theme_dir, &mut entries);
            }
        }
        // pixmaps (último fallback).
        if let Ok(it) = fs::read_dir("/usr/share/pixmaps") {
            for entry in it.flatten() {
                let p = entry.path();
                if is_supported_ext(&p) {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        entries.entry(stem.to_string()).or_insert(p.clone());
                    }
                }
            }
        }
        IconIndex { entries }
    }
}

/// Indexa arquivos a 2 níveis de profundidade (layout theme/<l1>/<l2>/file).
fn index_two_levels(theme_dir: &std::path::Path, entries: &mut HashMap<String, PathBuf>) {
    let Ok(level1) = fs::read_dir(theme_dir) else {
        return;
    };
    for l1 in level1.flatten() {
        let l1p = l1.path();
        if !l1p.is_dir() {
            continue;
        }
        let Ok(level2) = fs::read_dir(&l1p) else {
            continue;
        };
        for l2 in level2.flatten() {
            let l2p = l2.path();
            if !l2p.is_dir() {
                continue;
            }
            let Ok(files) = fs::read_dir(&l2p) else {
                continue;
            };
            for f in files.flatten() {
                let p = f.path();
                if is_supported_ext(&p) {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        entries.entry(stem.to_string()).or_insert(p.clone());
                    }
                }
            }
        }
    }
}

fn theme_base_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        out.push(PathBuf::from(home).join(".local/share/icons"));
    }
    out.push(PathBuf::from("/usr/share/icons"));
    out
}

const THEME_PRIORITY: &[&str] = &["breeze", "breeze-dark", "Adwaita", "AdwaitaLegacy", "hicolor"];

// ─── Lançamento (desanexado) ───────────────────────────────────────────

/// Roda o `exec` desanexado (setsid + background) — sobrevive o menu fechar.
pub fn launch(exec: &str) {
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("setsid {exec} >/dev/null 2>&1 </dev/null &"))
        .spawn();
}

/// Abre um caminho no gerenciador de arquivos (via xdg-open por ora; quando o
/// FM do Luma existir, troca só o comando). Desanexado.
pub fn open_path(path: &str) {
    let safe = path.replace('\'', "");
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("setsid xdg-open '{safe}' >/dev/null 2>&1 </dev/null &"))
        .spawn();
}

/// Abre o luma-config. Em dev acha o binário irmão (target/debug); senão PATH.
pub fn launch_config() {
    launch(&config_cmd());
}

/// Caminho do binário luma-config (irmão em dev, senão PATH).
fn config_cmd() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("luma-config")))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "luma-config".to_string())
}

/// Abre o luma-config com argumentos (ex: --tab widgets --target desktop).
pub fn launch_config_args(args: &[&str]) {
    let quoted: String = args
        .iter()
        .map(|a| format!(" '{}'", a.replace('\'', "")))
        .collect();
    launch(&format!("{}{}", config_cmd(), quoted));
}

/// Abre um terminal em $HOME. Mesma ordem do compositor: $TERMINAL →
/// x-terminal-emulator → konsole → gnome-terminal → foot → alacritty → kitty.
pub fn open_terminal() {
    let candidates = [
        std::env::var("TERMINAL").ok(),
        Some("x-terminal-emulator".into()),
        Some("konsole".into()),
        Some("gnome-terminal".into()),
        Some("foot".into()),
        Some("alacritty".into()),
        Some("kitty".into()),
        Some("xterm".into()),
    ];
    for cand in candidates.into_iter().flatten() {
        if cand.is_empty() {
            continue;
        }
        // which: só lança o primeiro que existe no PATH.
        let bin = cand.split_whitespace().next().unwrap_or(&cand);
        if which(bin) {
            launch(&cand);
            return;
        }
    }
}

/// `true` se o binário está no PATH.
fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join(bin).is_file())
        })
        .unwrap_or(false)
}

// ─── Lugares (destinos da coluna direita) ──────────────────────────────

/// Caminho da pasta pessoal.
pub fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
}

/// Caminho de Documentos (XDG_DOCUMENTS_DIR → ~/Documentos → ~/Documents → ~).
pub fn documents_dir() -> String {
    user_dir("XDG_DOCUMENTS_DIR", &["Documentos", "Documents"])
}

/// Caminho de Imagens (XDG_PICTURES_DIR → ~/Imagens → ~/Pictures → ~).
pub fn pictures_dir() -> String {
    user_dir("XDG_PICTURES_DIR", &["Imagens", "Pictures"])
}

fn user_dir(env_key: &str, names: &[&str]) -> String {
    if let Ok(v) = std::env::var(env_key) {
        if !v.is_empty() && PathBuf::from(&v).is_dir() {
            return v;
        }
    }
    let home = home_dir();
    for n in names {
        let p = PathBuf::from(&home).join(n);
        if p.is_dir() {
            return p.to_string_lossy().to_string();
        }
    }
    home
}
