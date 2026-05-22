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

/// Rubber-band em curso (drag em área vazia do desktop). H50.2c.
#[derive(Debug, Clone)]
pub struct RubberBand {
    /// Ponto onde o drag começou (mousedown).
    pub start: Point<f64, Logical>,
    /// Ponto atual do cursor (motion).
    pub current: Point<f64, Logical>,
}

impl RubberBand {
    /// Retângulo normalizado (top-left, size) das duas pontas.
    pub fn rect(&self) -> Rectangle<i32, Logical> {
        let x0 = self.start.x.min(self.current.x) as i32;
        let y0 = self.start.y.min(self.current.y) as i32;
        let x1 = self.start.x.max(self.current.x) as i32;
        let y1 = self.start.y.max(self.current.y) as i32;
        Rectangle::new(
            Point::from((x0, y0)),
            Size::from(((x1 - x0).max(1), (y1 - y0).max(1))),
        )
    }
}

/// Drag em curso. Setado quando o user pressiona esquerdo sobre um
/// ícone E move mais de `DRAG_THRESHOLD_PX`. H50.2b.
#[derive(Debug, Clone)]
pub struct DesktopDrag {
    pub path: PathBuf,
    /// Posição do cursor no início (mousedown), pra threshold.
    pub start_screen: Point<f64, Logical>,
    /// Offset do cursor relativo ao top-left da célula original.
    pub offset: Point<i32, Logical>,
    /// Posição atual do cursor (atualizada por motion).
    pub current_screen: Point<f64, Logical>,
    /// `false` enquanto não passou o threshold (ainda pode virar
    /// só click). `true` = realmente em drag, ghost renderiza.
    pub active: bool,
}

/// Distância mínima de movimento pra começar drag (pixels lógicos).
pub const DRAG_THRESHOLD_PX: f64 = 5.0;

/// Carrega posições persistidas em `~/.cache/luma/desktop-positions.
/// toml`. Vazio se arquivo não existir ou parse falhar. H50.2b.drag.5.
pub fn load_positions() -> std::collections::HashMap<String, (i32, i32)> {
    let path = positions_path();
    let Some(path) = path else {
        return Default::default();
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Default::default(),
    };
    let value: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("desktop-positions.toml parse falhou: {e}");
            return Default::default();
        }
    };
    let mut out = std::collections::HashMap::new();
    if let toml::Value::Table(table) = value {
        for (name, val) in table {
            if let toml::Value::Array(arr) = val {
                if arr.len() == 2 {
                    let col = arr[0].as_integer().unwrap_or(-1) as i32;
                    let row = arr[1].as_integer().unwrap_or(-1) as i32;
                    if col >= 0 && row >= 0 {
                        out.insert(name, (col, row));
                    }
                }
            }
        }
    }
    out
}

/// Salva posições no disco (sobreescreve o arquivo). H50.2b.drag.4.
/// Formato manual (toml crate só tem feature "parse" enabled).
pub fn save_positions(positions: &std::collections::HashMap<String, (i32, i32)>) {
    let Some(path) = positions_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut lines = String::from("# desktop-positions.toml — H50.2b\n");
    lines.push_str("# basename = [col, row]\n\n");
    let mut keys: Vec<&String> = positions.keys().collect();
    keys.sort();
    for name in keys {
        let (col, row) = positions[name];
        // Escape TOML basic-string: replace \ and " com escapes.
        let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
        lines.push_str(&format!("\"{}\" = [{}, {}]\n", escaped, col, row));
    }
    if let Err(e) = std::fs::write(&path, lines) {
        tracing::warn!("desktop-positions.toml write falhou: {e}");
    }
}

fn positions_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("luma").join("desktop-positions.toml"))
}

/// Resolve (col, row) por entrada — usa positions custom se houver,
/// senão auto-arrange column-major preenchendo as células livres.
/// H50.2b.drag.5.
pub fn compute_layout(
    entries: &[DesktopEntry],
    positions: &std::collections::HashMap<String, (i32, i32)>,
    rows: i32,
) -> Vec<(i32, i32)> {
    let rows = rows.max(1);
    let mut used: std::collections::HashSet<(i32, i32)> =
        std::collections::HashSet::new();
    // Primeiro fixa as customizadas.
    let mut result: Vec<Option<(i32, i32)>> = vec![None; entries.len()];
    for (i, e) in entries.iter().enumerate() {
        if let Some(pos) = positions.get(&e.name) {
            if !used.contains(pos) {
                result[i] = Some(*pos);
                used.insert(*pos);
            }
        }
    }
    // Depois preenche os ausentes column-major nas células livres.
    let mut cursor_col = 0i32;
    let mut cursor_row = 0i32;
    let next_free = |c: &mut i32, r: &mut i32, used: &std::collections::HashSet<(i32, i32)>| {
        loop {
            if !used.contains(&(*c, *r)) {
                return (*c, *r);
            }
            *r += 1;
            if *r >= rows {
                *r = 0;
                *c += 1;
            }
        }
    };
    for slot in result.iter_mut() {
        if slot.is_none() {
            let pos = next_free(&mut cursor_col, &mut cursor_row, &used);
            *slot = Some(pos);
            used.insert(pos);
            // Avança o cursor pra próxima célula candidata.
            cursor_row += 1;
            if cursor_row >= rows {
                cursor_row = 0;
                cursor_col += 1;
            }
        }
    }
    result.into_iter().map(|p| p.unwrap_or((0, 0))).collect()
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
    /// Posições customizadas (basename → (col, row)). H50.2b.
    positions: Mutex<std::collections::HashMap<String, (i32, i32)>>,
    /// Drag ativo. None = sem drag. H50.2b.
    drag: Mutex<Option<DesktopDrag>>,
    /// Rubber-band ativo (drag em área vazia). H50.2c.
    rubber_band: Mutex<Option<RubberBand>>,
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
            positions: Mutex::new(std::collections::HashMap::new()),
            drag: Mutex::new(None),
            rubber_band: Mutex::new(None),
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

    /// Substitui o mapa de posições customizadas. H50.2b.
    pub fn set_positions(&self, positions: std::collections::HashMap<String, (i32, i32)>) {
        *self.positions.lock().unwrap() = positions;
    }

    /// Substitui o estado de drag (Some quando active, None quando
    /// não tá arrastando ou ainda não passou do threshold). H50.2b.
    pub fn set_drag(&self, drag: Option<DesktopDrag>) {
        *self.drag.lock().unwrap() = drag;
    }

    /// Substitui o rubber-band ativo. H50.2c.
    pub fn set_rubber_band(&self, rb: Option<RubberBand>) {
        *self.rubber_band.lock().unwrap() = rb;
    }

    /// Retorna o índice de todas as entradas cujas células intersectam
    /// `rect`. Usado pelo mouseup do rubber-band pra selecionar. H50.2c.
    pub fn entries_in_rect(
        &self,
        rect: Rectangle<i32, Logical>,
    ) -> Vec<PathBuf> {
        let theme = crate::theme::current();
        let t = &theme.desktop_icons;
        let entries = self.entries.lock().ok();
        let Some(entries) = entries else { return Vec::new() };
        if entries.is_empty() {
            return Vec::new();
        }
        let output = self.current_output_size();
        let taskbar_h = crate::taskbar::taskbar_height();
        let usable_h = output.h - t.grid_margin - taskbar_h - 8;
        let rows = (usable_h / t.cell_height).max(1);
        let positions = self.positions.lock().ok();
        let positions_snap = positions
            .map(|g| (*g).clone())
            .unwrap_or_default();
        let layout = compute_layout(&entries, &positions_snap, rows);
        let rect_f = rect.to_f64();
        let mut out = Vec::new();
        for (idx, (col, row)) in layout.iter().enumerate() {
            let cell_x = t.grid_margin + col * t.cell_width;
            let cell_y = t.grid_margin + row * t.cell_height;
            let cell_rect = Rectangle::<i32, Logical>::new(
                Point::from((cell_x, cell_y)),
                Size::from((t.cell_width, t.cell_height)),
            )
            .to_f64();
            // Intersect.
            if cell_rect.loc.x < rect_f.loc.x + rect_f.size.w
                && cell_rect.loc.x + cell_rect.size.w > rect_f.loc.x
                && cell_rect.loc.y < rect_f.loc.y + rect_f.size.h
                && cell_rect.loc.y + cell_rect.size.h > rect_f.loc.y
            {
                if let Some(e) = entries.get(idx) {
                    out.push(e.path.clone());
                }
            }
        }
        out
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
    /// Hitbox = célula inteira (cell_width × cell_height). Usa
    /// compute_layout pra honrar posições customizadas. H50.2a/2b.
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
        let positions = self.positions.lock().ok()?;
        let layout = compute_layout(&entries, &positions, rows);
        drop(positions);
        for (idx, (col, row)) in layout.iter().enumerate() {
            let cell_x = t.grid_margin + col * t.cell_width;
            let cell_y = t.grid_margin + row * t.cell_height;
            if cell_x + t.cell_width > output.w - t.grid_margin {
                continue;
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

    /// Encontra a célula (col, row) mais próxima de um ponto. Usado
    /// pelo drop do drag pra snap. H50.2b.drag.3.
    pub fn nearest_cell(&self, point: Point<f64, Logical>) -> (i32, i32) {
        let theme = crate::theme::current();
        let t = &theme.desktop_icons;
        let col = ((point.x - t.grid_margin as f64) / t.cell_width as f64).round() as i32;
        let row = ((point.y - t.grid_margin as f64) / t.cell_height as f64).round() as i32;
        (col.max(0), row.max(0))
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
        let positions_snap = self
            .positions
            .lock()
            .ok()
            .map(|g| (*g).clone())
            .unwrap_or_default();
        let layout = compute_layout(&entries, &positions_snap, rows);
        let drag_snap = self.drag.lock().ok().and_then(|g| g.clone());
        let rb_snap = self.rubber_band.lock().ok().and_then(|g| g.clone());
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
            let (col, row) = layout[idx];
            // H50.2b.drag.2: se o ícone arrastado está ativo, renderiza
            // ele seguindo o cursor (ghost) e pula a célula original.
            let is_dragging = drag_snap
                .as_ref()
                .map(|d| d.active && d.path == entry.path)
                .unwrap_or(false);
            let (cell_x, cell_y) = if is_dragging {
                if let Some(d) = drag_snap.as_ref() {
                    let cx = d.current_screen.x.round() as i32 - d.offset.x;
                    let cy = d.current_screen.y.round() as i32 - d.offset.y;
                    (cx, cy)
                } else {
                    (
                        t.grid_margin + col * t.cell_width,
                        t.grid_margin + row * t.cell_height,
                    )
                }
            } else {
                (
                    t.grid_margin + col * t.cell_width,
                    t.grid_margin + row * t.cell_height,
                )
            };
            // Para se sair pela direita do output (overflow horizontal).
            // Não pula se for o ghost (pode estar em qualquer lugar).
            if !is_dragging && cell_x + t.cell_width > output_size.w - t.grid_margin {
                continue;
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
        // H50.2c.rb.2: rubber-band rect (caixa de seleção) renderizada
        // POR ÚLTIMO no Vec — fica POR CIMA dos ícones. Bg semi-
        // transparente + borda âmbar 1px (4 rects finos no perímetro).
        if let Some(rb) = rb_snap.as_ref() {
            let rect = rb.rect();
            // Bg semi-transparente.
            let bg = crate::gfx::make_rounded_panel(
                rect.size.w.max(1),
                rect.size.h.max(1),
                4,
                t.rubber_band_bg,
            );
            let bg_loc = Point::<i32, Logical>::from((rect.loc.x, rect.loc.y))
                .to_physical_precise_round(1.0);
            let loc_phys = location + bg_loc;
            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                loc_phys.to_f64(),
                &bg,
                None,
                None,
                None,
                Kind::Unspecified,
            ) {
                out.push(C::from(LumaRenderElement::Memory(elem)));
            }
            // Borda 1px (4 retângulos finos).
            let border_w = 1i32;
            let edges = [
                // Top
                (rect.loc.x, rect.loc.y, rect.size.w, border_w),
                // Bottom
                (
                    rect.loc.x,
                    rect.loc.y + rect.size.h - border_w,
                    rect.size.w,
                    border_w,
                ),
                // Left
                (rect.loc.x, rect.loc.y, border_w, rect.size.h),
                // Right
                (
                    rect.loc.x + rect.size.w - border_w,
                    rect.loc.y,
                    border_w,
                    rect.size.h,
                ),
            ];
            for (x, y, w, h) in edges {
                let edge_buf =
                    crate::gfx::make_rounded_panel(w.max(1), h.max(1), 0, t.rubber_band_border);
                let pos = Point::<i32, Logical>::from((x, y)).to_physical_precise_round(1.0);
                let loc = location + pos;
                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    loc.to_f64(),
                    &edge_buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    out.push(C::from(LumaRenderElement::Memory(elem)));
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
