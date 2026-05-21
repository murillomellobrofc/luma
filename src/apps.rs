use std::{fs, path::PathBuf};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::element::memory::MemoryRenderBuffer,
    },
    utils::{Logical, Size, Transform},
};

/// Ícone decodificado de um app: o buffer pronto pra render + tamanho
/// lógico original (necessário pra calcular `src` rect ao escalar).
pub type AppIcon = (MemoryRenderBuffer, Size<i32, Logical>);
use tracing::{info, warn};

/// Aplicativo discovered de um arquivo `.desktop`.
#[derive(Debug, Clone)]
pub struct DesktopApp {
    pub name: String,
    /// Comando do campo `Exec`, sem placeholders `%u`/`%f`/etc.
    pub exec: String,
    /// Nome (ou caminho) do ícone do campo `Icon=`. `None` se ausente.
    pub icon: Option<String>,
}

/// Carrega apps de `/usr/share/applications/*.desktop`. Filtra apps com
/// `NoDisplay=true` ou `Hidden=true`, mantém só `Type=Application`,
/// ordena alfabeticamente pelo Name. Pra MVP, limit em 16 (cabe na grid).
pub fn load(max: usize) -> Vec<DesktopApp> {
    let dirs = [
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];

    let mut apps: Vec<DesktopApp> = Vec::new();
    for dir in &dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            match parse(&path) {
                Ok(Some(app)) => apps.push(app),
                Ok(None) => {}
                Err(err) => warn!("erro parseando {:?}: {err}", path),
            }
        }
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.name == b.name);
    apps.truncate(max);
    info!("{} apps carregados de /usr/share/applications", apps.len());
    apps
}

/// Reordena `apps` colocando favoritos curados primeiro. Os primeiros N
/// apps da Vec viram favoritos da coluna esquerda do menu. H32.7.fix.3.
///
/// Cada grupo é um conjunto de aliases (case-insensitive) testado contra
/// `Name`, `Icon` e o primeiro segmento do `Exec` de cada app. O primeiro
/// match de cada grupo é promovido. O resto fica em ordem alfabética.
pub fn promote_favorites(apps: &mut Vec<DesktopApp>) {
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

    let mut chosen: Vec<usize> = Vec::new();
    for group in groups {
        for (i, app) in apps.iter().enumerate() {
            if chosen.contains(&i) {
                continue;
            }
            if matches_any(app, group) {
                chosen.push(i);
                break;
            }
        }
    }

    if chosen.is_empty() {
        return;
    }

    let chosen_set: std::collections::HashSet<usize> = chosen.iter().copied().collect();
    let mut reordered: Vec<DesktopApp> = Vec::with_capacity(apps.len());
    for &i in &chosen {
        reordered.push(apps[i].clone());
    }
    for (i, app) in apps.iter().enumerate() {
        if !chosen_set.contains(&i) {
            reordered.push(app.clone());
        }
    }
    *apps = reordered;
    info!("{} favoritos curados promovidos pro topo", chosen.len());
}

fn matches_any(app: &DesktopApp, patterns: &[&str]) -> bool {
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

fn parse(path: &std::path::Path) -> Result<Option<DesktopApp>, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let mut in_entry = false;
    let mut type_field: Option<String> = None;
    let mut name: Option<String> = None;
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
                "Exec" if exec.is_none() => exec = Some(strip_field_codes(val)),
                "Icon" if icon.is_none() => icon = Some(val.to_string()),
                "NoDisplay" => no_display = val.eq_ignore_ascii_case("true"),
                "Hidden" => hidden = val.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
    }

    if no_display || hidden {
        return Ok(None);
    }
    if type_field.as_deref() != Some("Application") {
        return Ok(None);
    }
    let (Some(name), Some(exec)) = (name, exec) else {
        return Ok(None);
    };
    if exec.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(DesktopApp { name, exec, icon }))
}

/// Diretórios-base do XDG Icon Theme spec, em ordem de prioridade:
/// per-user primeiro (override), depois sistema, depois pixmaps legacy.
/// H33.icons.1.
fn theme_base_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        out.push(PathBuf::from(home).join(".local/share/icons"));
    }
    out.push(PathBuf::from("/usr/share/icons"));
    out
}

/// Ordem de prioridade dos temas: Breeze (KDE) → Adwaita (GTK) →
/// hicolor (universal). Apps KDE têm ícone só no Breeze; apps GTK
/// puros no Adwaita/hicolor. hicolor SEMPRE é o último fallback.
/// H33.icons.1.
const THEME_PRIORITY: &[&str] = &[
    "breeze",
    "breeze-dark",
    "Adwaita",
    "AdwaitaLegacy",
    "hicolor",
];

/// Sizes em ordem de preferência. 48 primeiro (canônico Breeze + GTK
/// medium). 32 pra apps de baixa-res. H33.icons.3.
const SIZE_PREF: &[&str] = &["48", "64", "32", "128", "96", "256", "192", "16"];

/// Categorias de ícone. `apps` é o foco aqui; `categories` cobre alguns
/// legacy.
const CATEGORIES: &[&str] = &["apps", "categories"];

/// Resolve o `Icon=` de um `.desktop` em um caminho PNG ou SVG no
/// sistema, varrendo Breeze (KDE), Adwaita (GTK) e hicolor (universal).
/// Retorna `None` se nenhum tema/tamanho/categoria contém o ícone.
/// H33.icons.1-4.
fn resolve_icon_path(icon: &str) -> Option<PathBuf> {
    // 1. Path absoluto: usa direto se for formato suportado.
    let p = PathBuf::from(icon);
    if p.is_absolute() && p.exists() && is_supported_ext(&p) {
        return Some(p);
    }

    // 2. pixmaps legacy (geralmente PNG simples por app).
    for ext in &["png", "svg", "jpg", "jpeg"] {
        let candidate = PathBuf::from(format!("/usr/share/pixmaps/{}.{}", icon, ext));
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // 3. Temas. Varre cada base × theme × size × category × style.
    for base in theme_base_dirs() {
        for theme in THEME_PRIORITY {
            // 3a. Pasta scalable (SVG vetorial) — preferida quando há.
            for cat in CATEGORIES {
                // hicolor/Adwaita layout: theme/scalable/cat/<icon>.svg
                let svg = base.join(theme).join("scalable").join(cat).join(format!("{}.svg", icon));
                if svg.exists() {
                    return Some(svg);
                }
                // Breeze layout: theme/cat/scalable/<icon>.svg (rare)
                let svg2 = base.join(theme).join(cat).join("scalable").join(format!("{}.svg", icon));
                if svg2.exists() {
                    return Some(svg2);
                }
            }
            // 3b. Tamanhos fixos. PNG > SVG na mesma resolução.
            for size in SIZE_PREF {
                for cat in CATEGORIES {
                    // hicolor/Adwaita layout: theme/<size>x<size>/cat/<icon>.<ext>
                    let hi_dir = base.join(theme).join(format!("{}x{}", size, size)).join(cat);
                    for ext in &["png", "svg"] {
                        let candidate = hi_dir.join(format!("{}.{}", icon, ext));
                        if candidate.exists() {
                            return Some(candidate);
                        }
                    }
                    // Breeze layout: theme/cat/<size>/<icon>.<ext>  (size sem 'x')
                    let bz_dir = base.join(theme).join(cat).join(size);
                    for ext in &["svg", "png"] {
                        let candidate = bz_dir.join(format!("{}.{}", icon, ext));
                        if candidate.exists() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }
    None
}

fn is_supported_ext(p: &std::path::Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("png") | Some("jpg") | Some("jpeg") | Some("svg")
    )
}

/// Pra cada app, tenta carregar o ícone. Retorna Vec do mesmo tamanho
/// de `apps`, com `None` pros que não tem ícone disponível.
pub fn load_icons(apps: &[DesktopApp]) -> Vec<Option<AppIcon>> {
    let mut loaded = 0usize;
    let icons: Vec<_> = apps
        .iter()
        .map(|a| {
            let icon_name = a.icon.as_deref()?;
            let path = resolve_icon_path(icon_name)?;
            let icon = decode_icon(&path)?;
            loaded += 1;
            Some(icon)
        })
        .collect();
    info!("{}/{} ícones de apps carregados", loaded, apps.len());
    icons
}

fn decode_icon(path: &std::path::Path) -> Option<AppIcon> {
    // H33.icons.2: aceita SVG via icon_loader (usvg + resvg).
    if path.extension().and_then(|e| e.to_str()) == Some("svg") {
        // Rasteriza em 128×128 — alto o suficiente pra ficar nítido
        // quando escalado pra 32 (taskbar) ou 64 (menu fullscreen).
        let buf = crate::menu::icon_loader::load_svg_file(
            &path.to_string_lossy(),
            128,
        )?;
        return Some((buf, Size::from((128, 128))));
    }
    let img = image::open(path).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    let mut pixels = rgba.into_raw();
    // RGBA → BGRA premultiplied (mesmo padrão do wallpaper).
    for chunk in pixels.chunks_exact_mut(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];
        chunk[0] = ((b as u16 * a as u16) / 255) as u8;
        chunk[1] = ((g as u16 * a as u16) / 255) as u8;
        chunk[2] = ((r as u16 * a as u16) / 255) as u8;
        chunk[3] = a;
    }
    let buffer = MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (w, h),
        1,
        Transform::Normal,
        None,
    );
    Some((buffer, Size::from((w, h))))
}

/// Remove field codes do Exec (`%u`, `%U`, `%f`, `%F`, `%i`, `%c`, `%k`).
/// Mantém o resto intacto.
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(&next) = chars.peek() {
                if matches!(next, 'u' | 'U' | 'f' | 'F' | 'i' | 'c' | 'k' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm') {
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

/// Tenta resolver o ícone do app correspondente a um `app_id` Wayland.
/// Match heurístico: case-insensitive contra `Icon`, `Name` e o último
/// segmento do `Exec` de cada `.desktop` carregado. App_id pode ser
/// estilo "firefox", "org.mozilla.firefox", "footclient", etc.
pub fn find_icon_for_app_id<'a>(
    apps: &[DesktopApp],
    icons: &'a [Option<AppIcon>],
    app_id: &str,
) -> Option<&'a AppIcon> {
    let needle = app_id.to_lowercase();
    // Pega o último segmento depois de '.' pra IDs estilo "org.kde.dolphin"
    let needle_tail = needle.rsplit('.').next().unwrap_or(&needle);
    for (i, app) in apps.iter().enumerate() {
        let name_lower = app.name.to_lowercase();
        let icon_lower = app.icon.as_deref().unwrap_or("").to_lowercase();
        let exec_first = app
            .exec
            .split_whitespace()
            .next()
            .map(|s| s.rsplit('/').next().unwrap_or(s).to_lowercase())
            .unwrap_or_default();
        let hit = icon_lower == needle
            || icon_lower == needle_tail
            || name_lower == needle
            || name_lower == needle_tail
            || exec_first == needle
            || exec_first == needle_tail;
        if hit {
            if let Some(Some(icon)) = icons.get(i) {
                return Some(icon);
            }
        }
    }
    None
}

/// Lança o app via `sh -c <exec>` em background (não bloqueia o
/// compositor). Usa sh porque o Exec pode ter aspas, args, redirects.
pub fn launch(exec: &str) {
    info!("lançando: {exec}");
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(exec)
        .spawn()
        .map_err(|e| warn!("erro lançando {exec}: {e}"));
}
