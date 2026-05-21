//! Ícones de arquivo no desktop. H50.tray (fase 1).
//!
//! Lê `~/Desktop` periodicamente e mantém uma lista de entradas
//! ordenadas (pastas primeiro, alfabético case-insensitive). Cada
//! entrada conhece seu `FileKind` derivado da extensão / metadados,
//! que mapeia direto no pack `assets/branding/icons/file-*.svg`.
//!
//! Fase 1 NÃO faz: seleção, drag, abrir. Só ler + desenhar.

use std::path::PathBuf;
use std::time::SystemTime;

/// Tipo de arquivo pro mapeamento icon do pack `assets/branding/
/// icons/file-*.svg`. H50.icons.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Folder,
    Image,
    Text,
    Video,
    Audio,
    Pdf,
    Code,
    Archive,
    Executable,
    Generic,
}

impl FileKind {
    /// Nome do SVG no pack (sem extensão).
    pub fn icon_name(self) -> &'static str {
        match self {
            FileKind::Folder => "file-folder",
            FileKind::Image => "file-image",
            FileKind::Text => "file-text",
            FileKind::Video => "file-video",
            FileKind::Audio => "file-audio",
            FileKind::Pdf => "file-pdf",
            FileKind::Code => "file-code",
            FileKind::Archive => "file-archive",
            FileKind::Executable => "file-executable",
            FileKind::Generic => "file-generic",
        }
    }
}

/// Detecta o `FileKind` de um path por: (1) é diretório? (2) extensão.
/// Executável é decidido por bit `+x` em arquivo regular OU extensão
/// `.AppImage` / `.sh` / `.run`. H50.icons.2.
pub fn kind_of(path: &std::path::Path, is_dir: bool) -> FileKind {
    if is_dir {
        return FileKind::Folder;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" | "tiff" | "tif" => {
            FileKind::Image
        }
        "txt" | "md" | "log" | "ini" | "conf" | "yaml" | "yml" | "toml" | "json"
        | "xml" | "html" | "htm" | "css" => FileKind::Text,
        "mp4" | "mkv" | "avi" | "webm" | "mov" | "flv" => FileKind::Video,
        "mp3" | "ogg" | "wav" | "flac" | "opus" | "m4a" | "aac" => FileKind::Audio,
        "pdf" => FileKind::Pdf,
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "cpp" | "cc" | "h"
        | "hpp" | "go" | "java" | "rb" | "lua" | "php" | "swift" | "kt" => FileKind::Code,
        "zip" | "tar" | "gz" | "xz" | "bz2" | "7z" | "rar" | "zst" => FileKind::Archive,
        "appimage" | "sh" | "run" => FileKind::Executable,
        _ => {
            // Executável por bit +x (lookup metadata once).
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = path.metadata() {
                    if meta.is_file() && (meta.permissions().mode() & 0o111) != 0 {
                        return FileKind::Executable;
                    }
                }
            }
            FileKind::Generic
        }
    }
}

/// Uma entrada do desktop (arquivo ou pasta visível).
#[derive(Debug, Clone)]
pub struct DesktopEntry {
    pub path: PathBuf,
    /// Nome do arquivo sem path (display).
    pub name: String,
    pub kind: FileKind,
    /// mtime do arquivo — usado pelo polling pra detectar mudança.
    /// `None` se metadata falhar.
    pub mtime: Option<SystemTime>,
}

/// Lista as entradas visíveis de `~/Desktop`. Ordem: pastas primeiro,
/// depois arquivos, ambos alfabético case-insensitive. Hidden dotfiles
/// excluídos (UX padrão de qualquer gerenciador de arquivos). Se a
/// pasta não existe, retorna Vec vazio — luma NUNCA cria a pasta.
/// H50.icons.1-2.
pub fn scan() -> Vec<DesktopEntry> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let dir = PathBuf::from(home).join("Desktop");
    let rd = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut entries: Vec<DesktopEntry> = Vec::new();
    for ent in rd.flatten() {
        let name_os = ent.file_name();
        let Some(name) = name_os.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let path = ent.path();
        let meta = ent.metadata().ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let mtime = meta.as_ref().and_then(|m| m.modified().ok());
        let kind = kind_of(&path, is_dir);
        entries.push(DesktopEntry {
            path,
            name: name.to_string(),
            kind,
            mtime,
        });
    }
    entries.sort_by(|a, b| {
        // Pastas antes de arquivos, depois alfabético case-insensitive.
        let a_dir = matches!(a.kind, FileKind::Folder);
        let b_dir = matches!(b.kind, FileKind::Folder);
        match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    entries
}
