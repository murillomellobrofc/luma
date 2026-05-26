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
    /// H56: ordem canônica pra "Organizar por tipo". Folder primeiro,
    /// depois extensões mais "informacionais" (texto, código, pdf),
    /// depois mídia, depois arquivos comprimidos, depois executáveis
    /// e por último genéricos.
    pub fn sort_order(self) -> u8 {
        match self {
            FileKind::Folder => 0,
            FileKind::Text => 1,
            FileKind::Code => 2,
            FileKind::Pdf => 3,
            FileKind::Image => 4,
            FileKind::Audio => 5,
            FileKind::Video => 6,
            FileKind::Archive => 7,
            FileKind::Executable => 8,
            FileKind::Generic => 9,
        }
    }
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

/// Estado de renomear inline (H53). `text` começa = `original_name`
/// com `all_selected = true` — primeiro Char/Backspace substitui
/// todo o texto. cursor_byte é byte offset (utf-8 safe).
#[derive(Debug, Clone)]
pub struct DesktopRename {
    pub path: PathBuf,
    pub original_name: String,
    pub text: String,
    pub cursor_byte: usize,
    pub all_selected: bool,
    pub started_at: std::time::Instant,
}

impl DesktopRename {
    pub fn new(path: PathBuf) -> Self {
        let original_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let cursor_byte = original_name.len();
        Self {
            path,
            text: original_name.clone(),
            original_name,
            cursor_byte,
            all_selected: true,
            started_at: std::time::Instant::now(),
        }
    }

    /// `true` quando o cursor deve estar visível (blink on).
    pub fn cursor_visible(&self, blink_ms: u64) -> bool {
        if blink_ms == 0 {
            return true;
        }
        let elapsed = self.started_at.elapsed().as_millis() as u64;
        (elapsed / blink_ms) % 2 == 0
    }

    /// Move o cursor pra esquerda 1 char (utf-8). Não passa do 0.
    pub fn cursor_left(&mut self) {
        self.all_selected = false;
        if self.cursor_byte == 0 {
            return;
        }
        // Move até o byte anterior que começa um char.
        let mut i = self.cursor_byte - 1;
        while i > 0 && !self.text.is_char_boundary(i) {
            i -= 1;
        }
        self.cursor_byte = i;
    }

    /// Move o cursor pra direita 1 char.
    pub fn cursor_right(&mut self) {
        self.all_selected = false;
        if self.cursor_byte >= self.text.len() {
            return;
        }
        let mut i = self.cursor_byte + 1;
        while i < self.text.len() && !self.text.is_char_boundary(i) {
            i += 1;
        }
        self.cursor_byte = i;
    }

    pub fn cursor_home(&mut self) {
        self.all_selected = false;
        self.cursor_byte = 0;
    }

    pub fn cursor_end(&mut self) {
        self.all_selected = false;
        self.cursor_byte = self.text.len();
    }

    /// Insere um char na posição do cursor. Se `all_selected`,
    /// substitui o texto inteiro pelo char. H53.rename.4.
    pub fn insert_char(&mut self, c: char) {
        if c == '/' || c.is_control() {
            return;
        }
        if self.all_selected {
            self.text.clear();
            self.cursor_byte = 0;
            self.all_selected = false;
        }
        self.text.insert(self.cursor_byte, c);
        self.cursor_byte += c.len_utf8();
        self.started_at = std::time::Instant::now(); // re-arma blink visible
    }

    /// Backspace: se `all_selected`, limpa tudo. Senão remove o char
    /// antes do cursor. H53.rename.4.
    pub fn backspace(&mut self) {
        if self.all_selected {
            self.text.clear();
            self.cursor_byte = 0;
            self.all_selected = false;
            return;
        }
        if self.cursor_byte == 0 {
            return;
        }
        let mut i = self.cursor_byte - 1;
        while i > 0 && !self.text.is_char_boundary(i) {
            i -= 1;
        }
        self.text.replace_range(i..self.cursor_byte, "");
        self.cursor_byte = i;
        self.started_at = std::time::Instant::now();
    }
}

/// Direção pra navegação por teclado. H50.2c.kb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
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

/// H56: estado da animação de "Organizar ícones". Cada entrada do
/// desktop tem uma posição source (a que tinha) e target (depois do
/// reflow). Render interpola pixel-space com ease_out_cubic.
#[derive(Debug, Clone)]
pub struct ArrangeAnim {
    pub started_at: std::time::Instant,
    pub duration_ms: u64,
    pub source: std::collections::HashMap<String, (i32, i32)>,
    pub target: std::collections::HashMap<String, (i32, i32)>,
}

impl ArrangeAnim {
    /// 0.0 → começo, 1.0 → fim. Aplicado ease_out_cubic.
    pub fn progress(&self) -> f32 {
        let elapsed = self.started_at.elapsed().as_millis() as f32;
        let raw = (elapsed / self.duration_ms as f32).clamp(0.0, 1.0);
        let inv = 1.0 - raw;
        1.0 - inv * inv * inv
    }
    pub fn done(&self) -> bool {
        self.started_at.elapsed().as_millis() as u64 >= self.duration_ms
    }
}

/// H56: ordena entradas por critério. Não muta a lista original —
/// retorna uma nova `Vec` na ordem desejada.
pub fn sort_entries(
    entries: &[DesktopEntry],
    kind: crate::context_menu::ArrangeKind,
) -> Vec<DesktopEntry> {
    use crate::context_menu::ArrangeKind;
    let mut sorted: Vec<DesktopEntry> = entries.to_vec();
    match kind {
        ArrangeKind::ByName => {
            sorted.sort_by(|a, b| {
                // Pastas antes de arquivos, depois alfabético i-case.
                let a_folder = a.kind == FileKind::Folder;
                let b_folder = b.kind == FileKind::Folder;
                match (a_folder, b_folder) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                }
            });
        }
        ArrangeKind::ByKind => {
            sorted.sort_by(|a, b| {
                let oa = a.kind.sort_order();
                let ob = b.kind.sort_order();
                oa.cmp(&ob).then_with(|| {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                })
            });
        }
        ArrangeKind::ByDate => {
            sorted.sort_by(|a, b| {
                // Mais recente primeiro. None vai pro fim.
                match (a.mtime, b.mtime) {
                    (Some(ta), Some(tb)) => tb.cmp(&ta),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => {
                        a.name.to_lowercase().cmp(&b.name.to_lowercase())
                    }
                }
            });
        }
    }
    sorted
}

/// H56: gera positions column-major a partir de uma lista ordenada.
/// Preenche o canto sup-esq, descendo nas linhas até `rows`, depois
/// pula pra coluna seguinte. Retorna `HashMap<basename, (col, row)>`.
pub fn column_major_positions(
    sorted: &[DesktopEntry],
    rows: i32,
) -> std::collections::HashMap<String, (i32, i32)> {
    let rows = rows.max(1);
    let mut out: std::collections::HashMap<String, (i32, i32)> =
        std::collections::HashMap::with_capacity(sorted.len());
    let mut col = 0i32;
    let mut row = 0i32;
    for e in sorted.iter() {
        out.insert(e.name.clone(), (col, row));
        row += 1;
        if row >= rows {
            row = 0;
            col += 1;
        }
    }
    out
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
    /// Renomear inline em curso (Some quando algum ícone está sendo
    /// renomeado). H53.
    renaming: Mutex<Option<DesktopRename>>,
    /// Animação de "Organizar ícones" em curso (H56). None quando
    /// estática. Limpa sozinha quando elapsed >= duration no render.
    arrange_anim: Mutex<Option<ArrangeAnim>>,
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
            renaming: Mutex::new(None),
            arrange_anim: Mutex::new(None),
        }
    }

    /// H56: inicia (ou substitui) a animação de organização.
    pub fn set_arrange_anim(&self, anim: Option<ArrangeAnim>) {
        *self.arrange_anim.lock().unwrap() = anim;
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

    /// Substitui o estado de renomear inline (None = sem rename).
    /// H53.
    pub fn set_renaming(&self, rename: Option<DesktopRename>) {
        *self.renaming.lock().unwrap() = rename;
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

    /// Encontra o path do vizinho em uma direção (Up/Down/Left/
    /// Right). Sem wrap — retorna None se não há ícone naquela célula.
    /// H50.2c.kb.4.
    pub fn neighbor_path(
        &self,
        current: &std::path::Path,
        positions: &std::collections::HashMap<String, (i32, i32)>,
        dir: Direction,
    ) -> Option<PathBuf> {
        let theme = crate::theme::current();
        let t = &theme.desktop_icons;
        let entries = self.entries.lock().ok()?;
        let output = self.current_output_size();
        let taskbar_h = crate::taskbar::taskbar_height();
        let usable_h = output.h - t.grid_margin - taskbar_h - 8;
        let rows = (usable_h / t.cell_height).max(1);
        let layout = compute_layout(&entries, positions, rows);
        // Localiza a célula do current.
        let curr_idx = entries.iter().position(|e| e.path == current)?;
        let (cur_col, cur_row) = layout[curr_idx];
        let (target_col, target_row) = match dir {
            Direction::Up => (cur_col, cur_row - 1),
            Direction::Down => (cur_col, cur_row + 1),
            Direction::Left => (cur_col - 1, cur_row),
            Direction::Right => (cur_col + 1, cur_row),
        };
        if target_col < 0 || target_row < 0 || target_row >= rows {
            return None;
        }
        // Encontra a entry naquela célula.
        for (idx, pos) in layout.iter().enumerate() {
            if *pos == (target_col, target_row) {
                return entries.get(idx).map(|e| e.path.clone());
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

    /// Renderiza o campo editável de rename inline no lugar do label
    /// normal: bg arredondado + borda âmbar + texto + (seleção total
    /// OU cursor blink). H53.rename.2-3.
    #[allow(clippy::too_many_arguments)]
    fn render_rename_field<R, C>(
        &self,
        rename: &DesktopRename,
        text_r: &crate::text::TextRenderer,
        cell_x: i32,
        label_y: i32,
        cell_w: i32,
        renderer: &mut R,
        location: Point<i32, Physical>,
        out: &mut Vec<C>,
    ) where
        R: Renderer + ImportAll + ImportMem,
        R::TextureId: smithay::backend::renderer::Texture + Clone + Send + 'static,
        C: From<LumaRenderElement<R>>,
    {
        let theme = crate::theme::current();
        let t = &theme.desktop_icons;
        // Rasteriza o texto completo (cream).
        let display_text = if rename.text.is_empty() {
            " " // garante altura visível mesmo vazio
        } else {
            rename.text.as_str()
        };
        let (text_px, tw, th) = text_r.rasterize_colored(
            display_text,
            t.label_size,
            t.rename_field_text,
        );
        if tw == 0 || th == 0 {
            return;
        }
        // Field: padding interno + tamanho mínimo do cell_w * 0.9.
        let pad_x = 6;
        let pad_y = 3;
        let field_w = (tw as i32 + pad_x * 2).max((cell_w as f32 * 0.9) as i32);
        let field_h = th as i32 + pad_y * 2;
        let field_x = cell_x + (cell_w - field_w) / 2;
        let field_y = label_y - pad_y;
        let bw = t.rename_field_border_width;
        let cr = t.rename_field_corner_radius;
        // Border (rounded panel maior atrás).
        let border = crate::gfx::make_rounded_panel(
            field_w + 2 * bw,
            field_h + 2 * bw,
            cr + bw,
            t.rename_field_border,
        );
        let bg = crate::gfx::make_rounded_panel(
            field_w,
            field_h,
            cr,
            t.rename_field_bg,
        );
        let push_mem = |out: &mut Vec<C>, buf: &MemoryRenderBuffer, x: i32, y: i32, r: &mut R| {
            let pos = Point::<i32, Logical>::from((x, y)).to_physical_precise_round(1.0);
            let loc = location + pos;
            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                r, loc.to_f64(), buf, None, None, None, Kind::Unspecified,
            ) {
                out.push(C::from(LumaRenderElement::Memory(elem)));
            }
        };
        // H53.fix.1: ORDEM REVERSA dos pushes — Smithay renderiza
        // end→start, então o ÚLTIMO pushed fica ATRÁS. Pra ter
        // border atrás de tudo + cursor em cima, push cursor PRIMEIRO
        // e border POR ÚLTIMO.
        //
        // Pré-cálculos do texto + cursor + sel_bg (precisam vir antes
        // dos pushes pra calcular offsets).
        let text_x = field_x + (field_w - tw as i32) / 2;
        let text_y = field_y + pad_y;
        let text_buf = MemoryRenderBuffer::from_slice(
            &text_px,
            smithay::backend::allocator::Fourcc::Argb8888,
            (tw as i32, th as i32),
            1,
            Transform::Normal,
            None,
        );
        // Cursor (só desenha se não tá tudo selecionado E visible).
        let cursor_data = if !rename.all_selected
            && rename.cursor_visible(t.rename_field_cursor_blink_ms)
        {
            let prefix = &rename.text[..rename.cursor_byte.min(rename.text.len())];
            let cursor_offset = if prefix.is_empty() {
                0
            } else {
                let (_, pw, _) = text_r.rasterize_colored(
                    prefix, t.label_size, t.rename_field_text,
                );
                pw as i32
            };
            let cur_buf = crate::gfx::make_rounded_panel(
                1, th as i32 + 2, 0, t.rename_field_cursor,
            );
            Some((cur_buf, text_x + cursor_offset, field_y + pad_y - 1))
        } else {
            None
        };
        // Sel_bg (só se all_selected).
        let sel_data = if rename.all_selected && !rename.text.is_empty() {
            let sel_bg = crate::gfx::make_rounded_panel(
                tw as i32 + 4,
                th as i32 + 2,
                3,
                t.rename_field_selection_bg,
            );
            let sel_x = field_x + (field_w - tw as i32) / 2 - 2;
            let sel_y = field_y + pad_y - 1;
            Some((sel_bg, sel_x, sel_y))
        } else {
            None
        };
        // PUSH ORDEM REVERSA (front → back).
        // 1. Cursor (front)
        if let Some((buf, x, y)) = cursor_data.as_ref() {
            push_mem(out, buf, *x, *y, renderer);
        }
        // 2. Texto
        push_mem(out, &text_buf, text_x, text_y, renderer);
        // 3. Sel_bg (atrás do texto)
        if let Some((buf, x, y)) = sel_data.as_ref() {
            push_mem(out, buf, *x, *y, renderer);
        }
        // 4. Bg do field (atrás da seleção)
        push_mem(out, &bg, field_x, field_y, renderer);
        // 5. Border (back)
        push_mem(out, &border, field_x - bw, field_y - bw, renderer);
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
        // H56: animação de "Organizar ícones" — se ativa, render usa
        // posições pixel-space lerped entre source e target. Auto-
        // clears quando termina.
        let arrange_anim: Option<ArrangeAnim> = {
            let mut g = self.arrange_anim.lock().ok();
            if let Some(g) = g.as_mut() {
                if let Some(anim) = g.as_ref() {
                    if anim.done() {
                        **g = None;
                    }
                }
                g.clone()
            } else {
                None
            }
        };
        let arrange_progress = arrange_anim
            .as_ref()
            .map(|a| a.progress());
        let rb_snap = self.rubber_band.lock().ok().and_then(|g| g.clone());
        let rename_snap = self.renaming.lock().ok().and_then(|g| g.clone());
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
            } else if let (Some(anim), Some(prog)) =
                (arrange_anim.as_ref(), arrange_progress)
            {
                // H56: pixel-space lerp source → target.
                let tgt = anim
                    .target
                    .get(&entry.name)
                    .copied()
                    .unwrap_or((col, row));
                let src = anim.source.get(&entry.name).copied().unwrap_or(tgt);
                let col_f = src.0 as f32
                    + (tgt.0 - src.0) as f32 * prog;
                let row_f = src.1 as f32
                    + (tgt.1 - src.1) as f32 * prog;
                (
                    t.grid_margin + (col_f * t.cell_width as f32) as i32,
                    t.grid_margin + (row_f * t.cell_height as f32) as i32,
                )
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
            // H53: se este ícone está sendo renomeado, render o campo
            // editável em vez do label normal.
            let is_renaming = rename_snap
                .as_ref()
                .map(|r| r.path == entry.path)
                .unwrap_or(false);
            if let Some(text_r) = text_r.as_ref() {
                if is_renaming {
                    if let Some(r) = rename_snap.as_ref() {
                        let label_y = icon_y + t.icon_size + 4;
                        self.render_rename_field(
                            r,
                            text_r,
                            cell_x,
                            label_y,
                            t.cell_width,
                            renderer,
                            location,
                            &mut out,
                        );
                    }
                } else if let Some((lbuf, lsize)) = self.load_label(text_r, &entry.name) {
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
