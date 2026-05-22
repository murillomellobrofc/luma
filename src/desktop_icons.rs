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
    /// Paths dos ícones selecionados. Path-based (não índice) porque
    /// o polling pode re-ordenar a lista. H50.2a.
    selection: Mutex<std::collections::HashSet<PathBuf>>,
    /// Cache do bg de seleção (rect arredondado) por size_chave.
    selection_bg_cache: Mutex<Option<(MemoryRenderBuffer, i32, i32)>>,
}

impl DesktopIconsLayer {
    pub fn new() -> Self {
        Self {
            output_size: Mutex::new(Size::from((0, 0))),
            entries: Mutex::new(Vec::new()),
            icon_cache: Mutex::new(HashMap::new()),
            label_cache: Mutex::new(HashMap::new()),
            selection: Mutex::new(std::collections::HashSet::new()),
            selection_bg_cache: Mutex::new(None),
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

    /// Substitui o set de paths selecionados. Chamado pelo dispatch
    /// de click em pointer.rs. H50.2a.
    pub fn set_selection(&self, selection: std::collections::HashSet<PathBuf>) {
        *self.selection.lock().unwrap() = selection;
    }

    /// Snapshot da seleção atual (path-based, estável entre re-scans).
    pub fn selection_snapshot(&self) -> std::collections::HashSet<PathBuf> {
        self.selection.lock().unwrap().clone()
    }

    /// Snapshot das entradas atuais.
    pub fn entries_snapshot(&self) -> Vec<DesktopEntry> {
        self.entries.lock().unwrap().clone()
    }

    fn current_output_size(&self) -> Size<i32, Logical> {
        *self.output_size.lock().unwrap()
    }

    /// Hit-test do grid: dado um ponto em coords lógicas do output,
    /// retorna o índice da entrada cuja célula contém o ponto.
    /// Hitbox = célula inteira (cell_width × cell_height). H50.2a.sel.1.
    pub fn hit_test_cell(&self, point: Point<f64, Logical>) -> Option<usize> {
        let theme = crate::theme::current();
        let t = &theme.desktop_icons;
        let entries = self.entries.lock().ok()?;
        if entries.is_empty() {
            return None;
        }
        let output = self.current_output_size();
        let taskbar_h = crate::taskbar::taskbar_height();
        let usable_h = output.h - t.grid_margin - taskbar_h - 8;
        let rows = (usable_h / t.cell_height).max(1);

        for (idx, _) in entries.iter().enumerate() {
            let col = (idx as i32) / rows;
            let row = (idx as i32) % rows;
            let cell_x = t.grid_margin + col * t.cell_width;
            let cell_y = t.grid_margin + row * t.cell_height;
            if cell_x + t.cell_width > output.w - t.grid_margin {
                break;
            }
            let rect: Rectangle<i32, Logical> = Rectangle::new(
                Point::from((cell_x, cell_y)),
                Size::from((t.cell_width, t.cell_height)),
            );
            if rect.to_f64().contains(point) {
                return Some(idx);
            }
        }
        None
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
    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        // H50.2a.sel.1: clicks sobre células de ícone são consumidos
        // pela layer (pra seleção/open); área vazia entre/após ícones
        // passa pra surface debaixo (wallpaper) — caller depois usa
        // a posição vazia pra deselect / right-click → context menu.
        self.hit_test_cell(*point).is_some()
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
        let selection = self.selection.lock().ok();
        // H50.2a.sel.2: bg de seleção lazy-cacheado (uma textura só,
        // reusada pra todos selecionados; mesmo tamanho da célula
        // menos um pequeno padding).
        let sel_bg = {
            let pad = 4i32;
            let w = t.cell_width - pad * 2;
            let h = t.cell_height - pad * 2;
            let mut cache = self.selection_bg_cache.lock().ok();
            let valid = cache
                .as_ref()
                .and_then(|c| c.as_ref())
                .map(|(_, cw, ch)| *cw == w && *ch == h)
                .unwrap_or(false);
            if !valid {
                if let Some(c) = cache.as_mut() {
                    let buf = crate::gfx::make_rounded_panel(
                        w,
                        h,
                        t.selection_corner_radius,
                        t.selection_bg,
                    );
                    **c = Some((buf, w, h));
                }
            }
            cache.as_ref().and_then(|c| c.as_ref().cloned())
        };
        for (idx, entry) in entries.iter().enumerate() {
            let col = (idx as i32) / rows;
            let row = (idx as i32) % rows;
            let cell_x = t.grid_margin + col * t.cell_width;
            let cell_y = t.grid_margin + row * t.cell_height;
            // Para se sair pela direita do output (overflow horizontal).
            if cell_x + t.cell_width > output_size.w - t.grid_margin {
                break;
            }
            // Bg de seleção (atrás do ícone) — só pros selecionados.
            let is_selected = selection
                .as_ref()
                .map(|s| s.contains(&entry.path))
                .unwrap_or(false);
            if is_selected {
                if let Some((sel_buf, sw, sh)) = sel_bg.as_ref() {
                    let bg_x = cell_x + (t.cell_width - sw) / 2;
                    let bg_y = cell_y + (t.cell_height - sh) / 2;
                    let pos = Point::<i32, Logical>::from((bg_x, bg_y))
                        .to_physical_precise_round(1.0);
                    let loc = location + pos;
                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                        renderer,
                        loc.to_f64(),
                        sel_buf,
                        None,
                        None,
                        None,
                        Kind::Unspecified,
                    ) {
                        out.push(C::from(LumaRenderElement::Memory(elem)));
                    }
                }
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
