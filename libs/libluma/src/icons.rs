//! Resolução de ícone freedesktop pros apps Luma (iced puro).
//!
//! Portado do `apps/luma-menu/src/apps.rs`, que por sua vez era um porte do
//! `compositor/luma/src/apps.rs` — eram duas cópias quase idênticas e a
//! taskbar (zona C) ia virar a terceira. Mora aqui pra ser uma só.
//!
//! Dois índices, ambos lazy atrás de `OnceLock` (um scan no primeiro uso,
//! lookup O(1) depois):
//!
//! - **índice de ícones** — `stem do arquivo → caminho`, varrendo os temas em
//!   ordem de prioridade. É o que resolve um `Icon=` num arquivo de verdade.
//! - **índice de `.desktop`** — `chave → valor do Icon=`, pra casar um
//!   `app_id` Wayland com o app que o abriu (ver [`icon_for_app_id`]).
//!
//! Não é o algoritmo canônico da spec de ícones do freedesktop: não lê
//! `index.theme` nem casa por tamanho, indexa por *stem* a dois níveis de
//! profundidade. É deliberado — o custo é um scan e o acerto na prática é
//! alto. Trocar pelo canônico é possível sem mexer nos chamadores.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Tipo de ícone resolvido, pro view escolher o widget (`svg` vs `image`).
#[derive(Debug, Clone)]
pub enum IconKind {
    Svg(PathBuf),
    Raster(PathBuf),
}

impl IconKind {
    /// O caminho, seja svg ou raster.
    pub fn path(&self) -> &Path {
        match self {
            IconKind::Svg(p) | IconKind::Raster(p) => p,
        }
    }
}

/// Classifica um caminho já resolvido pelo widget que o rende.
pub fn classify_icon(path: PathBuf) -> IconKind {
    if path.extension().and_then(|e| e.to_str()) == Some("svg") {
        IconKind::Svg(path)
    } else {
        IconKind::Raster(path)
    }
}

fn is_supported_ext(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("png") | Some("jpg") | Some("jpeg") | Some("svg")
    )
}

// ─── Índice de ícones (stem → caminho) ─────────────────────────────────

struct IconIndex {
    entries: HashMap<String, PathBuf>,
}

static ICON_INDEX: OnceLock<IconIndex> = OnceLock::new();

const THEME_PRIORITY: &[&str] = &["breeze", "breeze-dark", "Adwaita", "AdwaitaLegacy", "hicolor"];

/// Resolve o valor de um `Icon=` num arquivo. Caminho absoluto vence; senão
/// lookup O(1) por stem no índice. best-effort: `None` se não achar.
pub fn resolve_icon_path(icon: &str) -> Option<PathBuf> {
    if icon.is_empty() {
        return None;
    }
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

/// Indexa arquivos a 2 níveis de profundidade (layout `theme/<l1>/<l2>/file`).
fn index_two_levels(theme_dir: &Path, entries: &mut HashMap<String, PathBuf>) {
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

// ─── Índice de `.desktop` (chave → Icon=) ──────────────────────────────

struct DesktopIndex {
    /// Chave normalizada (lowercase) → valor cru do `Icon=`.
    entries: HashMap<String, String>,
}

static DESKTOP_INDEX: OnceLock<DesktopIndex> = OnceLock::new();

/// Pastas de `.desktop` na ordem XDG (usuário antes de sistema).
fn app_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |d: PathBuf| {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    };
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        push(PathBuf::from(data_home).join("applications"));
    } else if let Some(home) = std::env::var_os("HOME") {
        push(PathBuf::from(home).join(".local/share/applications"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for d in data_dirs.split(':') {
        if d.is_empty() {
            continue;
        }
        push(PathBuf::from(d).join("applications"));
    }
    dirs
}

impl DesktopIndex {
    fn build() -> DesktopIndex {
        let mut entries: HashMap<String, String> = HashMap::new();
        for dir in app_dirs() {
            let Ok(files) = fs::read_dir(&dir) else {
                continue;
            };
            for f in files.flatten() {
                let path = f.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                index_desktop_file(&path, &mut entries);
            }
        }
        DesktopIndex { entries }
    }
}

/// Lê um `.desktop` e registra todas as chaves por onde um `app_id` pode
/// chegar. Diferente do `load()` do menu, **não** pula `NoDisplay`/`Hidden`:
/// um app escondido do launcher ainda abre janela e precisa de ícone.
fn index_desktop_file(path: &Path, entries: &mut HashMap<String, String>) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    let mut in_entry = false;
    let mut icon: Option<String> = None;
    let mut name: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut wm_class: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            // Só a seção principal — as "Desktop Action" têm Icon/Exec próprios.
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let (key, val) = (key.trim(), val.trim());
        if val.is_empty() {
            continue;
        }
        match key {
            "Icon" if icon.is_none() => icon = Some(val.to_string()),
            "Name" if name.is_none() => name = Some(val.to_string()),
            "Exec" if exec.is_none() => exec = Some(val.to_string()),
            "StartupWMClass" if wm_class.is_none() => wm_class = Some(val.to_string()),
            _ => {}
        }
    }

    // Sem Icon= não há o que indexar.
    let Some(icon) = icon else {
        return;
    };

    let mut add = |k: Option<String>| {
        if let Some(k) = k {
            let k = k.trim().to_lowercase();
            if !k.is_empty() {
                // O primeiro que registra a chave vence — e `app_dirs()` vem
                // em ordem XDG, então o `.desktop` do usuário ganha do sistema.
                entries.entry(k).or_insert_with(|| icon.clone());
            }
        }
    };

    // `StartupWMClass` é a dica canônica pra casar janela↔app: quando existe,
    // é exatamente o app_id que o cliente vai anunciar.
    add(wm_class);
    // Stem do arquivo: `org.kde.dolphin.desktop` → `org.kde.dolphin`, que é o
    // app_id que apps Qt/KDE e GTK modernos anunciam.
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    add(stem.clone());
    // Cauda do stem (`org.kde.dolphin` → `dolphin`).
    add(stem.and_then(|s| s.rsplit('.').next().map(|t| t.to_string())));
    add(Some(icon.clone()));
    add(name);
    // Basename do primeiro token do Exec (`/usr/bin/foot -a x` → `foot`).
    add(exec.and_then(|e| {
        e.split_whitespace()
            .next()
            .map(|t| t.rsplit('/').next().unwrap_or(t).to_string())
    }));
}

/// Resolve o ícone de um `app_id` Wayland (o que o `xdg_toplevel` anuncia).
///
/// Cascata, do sinal mais forte pro mais fraco:
/// 1. o próprio `app_id` é nome de ícone (`"firefox"`);
/// 2. o `.desktop` que casa com ele — por `StartupWMClass`, stem do arquivo,
///    `Icon=`, `Name=` ou basename do `Exec=`. É este passo que resolve
///    `org.kde.dolphin`, cujo ícone se chama `system-file-manager`;
/// 3. a cauda depois do último ponto (`"org.mozilla.firefox"` → `"firefox"`),
///    tentada nos dois índices.
///
/// `None` = não achou; o chamador desenha o fallback (na barra, quadrado
/// colorido por hash + inicial do app).
pub fn icon_for_app_id(app_id: &str) -> Option<IconKind> {
    let app_id = app_id.trim();
    if app_id.is_empty() {
        return None;
    }
    let lower = app_id.to_lowercase();
    let tail = lower.rsplit('.').next().unwrap_or(&lower).to_string();
    let desktop = DESKTOP_INDEX.get_or_init(DesktopIndex::build);

    let candidates = [
        resolve_icon_path(app_id),
        desktop.entries.get(&lower).and_then(|i| resolve_icon_path(i)),
        resolve_icon_path(&tail),
        desktop.entries.get(&tail).and_then(|i| resolve_icon_path(i)),
    ];
    candidates.into_iter().flatten().next().map(classify_icon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifica_por_extensao() {
        assert!(matches!(
            classify_icon(PathBuf::from("/a/b.svg")),
            IconKind::Svg(_)
        ));
        assert!(matches!(
            classify_icon(PathBuf::from("/a/b.png")),
            IconKind::Raster(_)
        ));
    }

    #[test]
    fn icone_vazio_nao_resolve() {
        assert!(resolve_icon_path("").is_none());
        assert!(icon_for_app_id("").is_none());
        assert!(icon_for_app_id("   ").is_none());
    }

    #[test]
    fn indexa_chaves_do_desktop() {
        let dir = std::env::temp_dir().join("luma-icons-test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("org.kde.dolphin.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Dolphin\n\
             Exec=/usr/bin/dolphin %u\nIcon=system-file-manager\n\
             StartupWMClass=dolphin\n\n\
             [Desktop Action NewWindow]\nIcon=nao-e-pra-pegar\n",
        )
        .unwrap();

        let mut entries = HashMap::new();
        index_desktop_file(&path, &mut entries);

        // Todas as portas de entrada levam ao mesmo Icon=.
        for key in [
            "org.kde.dolphin",
            "dolphin",
            "system-file-manager",
            "dolphin", // Name= e StartupWMClass= colidem, tudo bem
        ] {
            assert_eq!(
                entries.get(key).map(String::as_str),
                Some("system-file-manager"),
                "chave {key} não indexou"
            );
        }
        // A seção [Desktop Action] não pode vazar pro índice.
        assert!(!entries.values().any(|v| v == "nao-e-pra-pegar"));
        let _ = fs::remove_file(&path);
    }

    /// Smoke test contra os apps instalados na máquina — por isso `#[ignore]`
    /// (o resultado depende do que está instalado, não serve pra CI).
    /// Rode com: `cargo test -p libluma -- --ignored --nocapture resolve_apps`
    #[test]
    #[ignore]
    fn resolve_apps_reais() {
        for app_id in [
            "firefox",
            "org.mozilla.firefox",
            "org.kde.dolphin",
            "org.kde.konsole",
            "code",
            "Alacritty",
            "app-id-que-nao-existe-mesmo",
        ] {
            match icon_for_app_id(app_id) {
                Some(k) => println!("  {app_id:38} → {}", k.path().display()),
                None => println!("  {app_id:38} → (fallback)"),
            }
        }
    }

    #[test]
    fn desktop_sem_icone_nao_entra() {
        let dir = std::env::temp_dir().join("luma-icons-test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("sem-icone.desktop");
        fs::write(&path, "[Desktop Entry]\nType=Application\nName=Nada\n").unwrap();

        let mut entries = HashMap::new();
        index_desktop_file(&path, &mut entries);
        assert!(entries.is_empty());
        let _ = fs::remove_file(&path);
    }
}
