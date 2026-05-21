//! Ícones de arquivo no desktop. H50.tray (fase 1).
//!
//! Lê `~/Desktop` periodicamente e mantém uma lista de entradas
//! ordenadas (pastas primeiro, alfabético case-insensitive). Cada
//! entrada conhece seu `FileKind` derivado da extensão / metadados,
//! que mapeia direto no pack `assets/branding/icons/file-*.svg`.
//!
//! Fase 1 NÃO faz: seleção, drag, abrir. Só ler + desenhar.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use smithay::{
    backend::renderer::{
        element::{
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            AsRenderElements, Kind,
        },
        ImportAll, ImportMem, Renderer,
    },
    desktop::space::SpaceElement,
    output::Output,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale, Size, Transform},
};

use crate::render::LumaRenderElement;

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

/// Camada de ícones do desktop como `SpaceElement` com z_index=5 —
/// entre o wallpaper (0) e janelas (10). Mapeada em (0,0) ocupando
/// o output inteiro. Não recebe input nesta fase (fase 1 = só ler
/// e desenhar). H50.icons.3.
#[derive(Debug)]
pub struct DesktopIconsLayer {
    output_size: Mutex<Size<i32, Logical>>,
    entries: Mutex<Vec<DesktopEntry>>,
    /// Cache de ícones do pack rasterizados por (name, size_px).
    icon_cache: Mutex<HashMap<(String, i32), MemoryRenderBuffer>>,
    /// Cache de labels rasterizados por (texto_truncado, tamanho).
    /// Reseta toda vez que muda o tema (font size).
    label_cache: Mutex<HashMap<String, (MemoryRenderBuffer, Size<i32, Logical>)>>,
}

impl DesktopIconsLayer {
    pub fn new() -> Self {
        Self {
            output_size: Mutex::new(Size::from((0, 0))),
            entries: Mutex::new(Vec::new()),
            icon_cache: Mutex::new(HashMap::new()),
            label_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_output_size(&self, size: Size<i32, Logical>) {
        *self.output_size.lock().unwrap() = size;
    }

    /// Substitui a lista de entradas. Chamado pelo polling em
    /// `state.rs::poll_desktop_icons` quando muda. H50.icons.7.
    pub fn set_entries(&self, entries: Vec<DesktopEntry>) {
        *self.entries.lock().unwrap() = entries;
    }

    fn current_output_size(&self) -> Size<i32, Logical> {
        *self.output_size.lock().unwrap()
    }

    /// Lookup/cache de um SVG do pack `assets/branding/icons/`.
    /// Diferente do `tray::ensure_icon_sized` que prepende `icon-` —
    /// o pack já vem nomeado `file-folder.svg` etc. H50.icons.5.
    fn load_pack_icon(&self, name: &str, size: i32) -> Option<MemoryRenderBuffer> {
        let mut cache = self.icon_cache.lock().ok()?;
        if let Some(b) = cache.get(&(name.to_string(), size)) {
            return Some(b.clone());
        }
        let path = format!(
            "{}/{}.svg",
            crate::theme::current().taskbar.system_tray.icons_path,
            name
        );
        let buf = crate::menu::icon_loader::load_svg_file(&path, size as u32)?;
        cache.insert((name.to_string(), size), buf.clone());
        Some(buf)
    }

    /// Lookup/cache de label rasterizado. Truncado em `label_max_chars`
    /// chars + ellipsis. H50.icons.6.
    fn load_label(
        &self,
        text_r: &crate::text::TextRenderer,
        raw: &str,
    ) -> Option<(MemoryRenderBuffer, Size<i32, Logical>)> {
        let t = &crate::theme::current().desktop_icons;
        let display: String = if raw.chars().count() > t.label_max_chars {
            let mut s: String = raw.chars().take(t.label_max_chars.saturating_sub(1)).collect();
            s.push('…');
            s
        } else {
            raw.to_string()
        };
        let mut cache = self.label_cache.lock().ok()?;
        if let Some(entry) = cache.get(&display) {
            return Some(entry.clone());
        }
        let (pixels, w, h) =
            text_r.rasterize_colored(&display, t.label_size, t.label_color);
        if w == 0 || h == 0 {
            return None;
        }
        let buf = MemoryRenderBuffer::from_slice(
            &pixels,
            smithay::backend::allocator::Fourcc::Argb8888,
            (w as i32, h as i32),
            1,
            Transform::Normal,
            None,
        );
        let size = Size::from((w as i32, h as i32));
        cache.insert(display, (buf.clone(), size));
        Some((buf, size))
    }
}

impl IsAlive for DesktopIconsLayer {
    fn alive(&self) -> bool {
        true
    }
}

impl SpaceElement for DesktopIconsLayer {
    fn bbox(&self) -> Rectangle<i32, Logical> {
        Rectangle::from_size(self.current_output_size())
    }
    fn geometry(&self) -> Rectangle<i32, Logical> {
        Rectangle::from_size(self.current_output_size())
    }
    fn is_in_input_region(&self, _point: &Point<f64, Logical>) -> bool {
        // Fase 1: sem input. Clicks passam pra wallpaper / desktop.
        false
    }
    fn set_activate(&self, _activated: bool) {}
    fn output_enter(&self, _output: &Output, _overlap: Rectangle<i32, Logical>) {}
    fn output_leave(&self, _output: &Output) {}
    fn refresh(&self) {}
    fn z_index(&self) -> u8 {
        // Entre wallpaper (0) e janelas (10).
        5
    }
}

impl<R> AsRenderElements<R> for DesktopIconsLayer
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + Send + 'static,
{
    type RenderElement = LumaRenderElement<R>;

    fn render_elements<C: From<LumaRenderElement<R>>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        _scale: Scale<f64>,
        _alpha: f32,
    ) -> Vec<C> {
        let theme = crate::theme::current();
        let t = &theme.desktop_icons;
        let entries = self.entries.lock().ok();
        let entries = match entries {
            Some(g) => g,
            None => return Vec::new(),
        };
        if entries.is_empty() {
            return Vec::new();
        }
        let output_size = self.current_output_size();
        // Quantas células cabem por coluna (subtraindo a taskbar e
        // margens superior/inferior).
        let taskbar_h = crate::taskbar::taskbar_height();
        let usable_h = output_size.h - t.grid_margin - taskbar_h - 8;
        let rows = (usable_h / t.cell_height).max(1);

        let mut out: Vec<C> = Vec::new();
        let text_r = crate::text::TextRenderer::load();
        for (idx, entry) in entries.iter().enumerate() {
            let col = (idx as i32) / rows;
            let row = (idx as i32) % rows;
            let cell_x = t.grid_margin + col * t.cell_width;
            let cell_y = t.grid_margin + row * t.cell_height;
            // Para se sair pela direita do output (overflow horizontal).
            if cell_x + t.cell_width > output_size.w - t.grid_margin {
                break;
            }
            let icon_x = cell_x + (t.cell_width - t.icon_size) / 2;
            let icon_y = cell_y + 8;
            if let Some(icon_buf) = self.load_pack_icon(entry.kind.icon_name(), t.icon_size) {
                let pos = Point::<i32, Logical>::from((icon_x, icon_y))
                    .to_physical_precise_round(1.0);
                let loc = location + pos;
                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    loc.to_f64(),
                    &icon_buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    out.push(C::from(LumaRenderElement::Memory(elem)));
                }
            }
            // Label (truncado + centralizado abaixo do ícone).
            if let Some(text_r) = text_r.as_ref() {
                if let Some((lbuf, lsize)) = self.load_label(text_r, &entry.name) {
                    let label_x = cell_x + (t.cell_width - lsize.w) / 2;
                    let label_y = icon_y + t.icon_size + 4;
                    let pos = Point::<i32, Logical>::from((label_x, label_y))
                        .to_physical_precise_round(1.0);
                    let loc = location + pos;
                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                        renderer,
                        loc.to_f64(),
                        &lbuf,
                        None,
                        None,
                        None,
                        Kind::Unspecified,
                    ) {
                        out.push(C::from(LumaRenderElement::Memory(elem)));
                    }
                }
            }
        }
        out
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
