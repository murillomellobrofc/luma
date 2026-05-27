//! Widgets nativos do Luma. POC da fatia 1: LabelWidget (texto fixo
//! numa pill arredondada). Cada widget implementa `luma_widget::Widget`
//! e se desenha num `MemoryRenderBuffer` único — o compositor compõe
//! o resultado direto, sem precisar entender layout do widget.
//!
//! As implementações ficam aqui no crate principal porque dependem
//! de helpers locais (gfx, text). O TRAIT mora em luma-widget pra
//! permitir widgets de terceiros sem ciclo de deps.
//!
//! Fatia 2: registro + parsing do layout.toml moram aqui também
//! (ver `WidgetRegistry`, `load_layout`). O construtor cravado
//! `LabelWidget::new` continua existindo pra compat, e a fatia 2.3
//! adiciona `from_config` pra montar via toml.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use luma_widget::{RenderCtx, Widget, WidgetHost};
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::element::memory::MemoryRenderBuffer,
    },
    utils::Transform,
};

use crate::{gfx::make_rounded_panel, text::TextRenderer};

/// Widget de texto fixo numa pill arredondada Golden Hour.
/// POC pra provar que a fundação funciona — fatia 1 do sistema de
/// widgets. Sem state, sem repaint, sem interação.
pub struct LabelWidget {
    text: String,
    text_renderer: Arc<TextRenderer>,
}

impl LabelWidget {
    pub fn new(text: impl Into<String>, text_renderer: Arc<TextRenderer>) -> Self {
        Self {
            text: text.into(),
            text_renderer,
        }
    }

    /// Fatia 2: constrói a partir do bloco config do layout.toml.
    /// Espera `text = "..."`. Campo ausente ou tipo errado = erro.
    /// `text_renderer` vem do boot via captura no builder do registro.
    pub fn from_config(
        config: &toml::Table,
        text_renderer: Arc<TextRenderer>,
    ) -> Result<Self, ConfigError> {
        let text = config
            .get("text")
            .ok_or_else(|| ConfigError("label: campo `text` ausente".into()))?
            .as_str()
            .ok_or_else(|| ConfigError("label: `text` precisa ser string".into()))?
            .to_string();
        Ok(Self { text, text_renderer })
    }
}

impl Widget for LabelWidget {
    fn render(&self, ctx: &RenderCtx) -> MemoryRenderBuffer {
        render_pill_text(&self.text, ctx, &self.text_renderer)
    }
}

/// Widget que mostra um contador subindo 1 em 1 a cada segundo. POC
/// da fatia 3 — prova que tick_interval + tick funcionam. Sem uso
/// real, é só pra ver o número mudar no painel.
pub struct CounterWidget {
    count: u64,
    text_renderer: Arc<TextRenderer>,
}

impl CounterWidget {
    pub fn from_config(
        config: &toml::Table,
        text_renderer: Arc<TextRenderer>,
    ) -> Result<Self, ConfigError> {
        // Aceita `start = N` opcional pra começar de um valor diferente
        // (útil pra debug). Default 0.
        let start: u64 = match config.get("start") {
            Some(v) => v
                .as_integer()
                .ok_or_else(|| {
                    ConfigError("counter: `start` precisa ser inteiro".into())
                })?
                .max(0) as u64,
            None => 0,
        };
        Ok(Self { count: start, text_renderer })
    }
}

impl Widget for CounterWidget {
    fn render(&self, ctx: &RenderCtx) -> MemoryRenderBuffer {
        render_pill_text(&self.count.to_string(), ctx, &self.text_renderer)
    }

    fn tick_interval(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs(1))
    }

    fn tick(&mut self) {
        self.count = self.count.saturating_add(1);
    }
}

/// Helper compartilhado: desenha uma pill arredondada Golden Hour com
/// `text` centralizado. Usado pelo LabelWidget (texto fixo) e pelo
/// CounterWidget (texto dinâmico). Quando a fatia 4+ ganhar tokens
/// próprios em `theme.widgets.*`, esse helper passa a ler de lá.
fn render_pill_text(
    text: &str,
    ctx: &RenderCtx,
    text_renderer: &TextRenderer,
) -> MemoryRenderBuffer {
    // Cores e métricas do theme. Zero hex hardcoded — tudo sai dos
    // tokens. Reusa lock.notif_* como ponto de partida do Golden
    // Hour (pill discreta sobre fundo escuro).
    let lock = &ctx.theme.lock;
    let pill_color = lock.notif_row_bg;
    let text_color = lock.notif_app_color;
    let font_size = lock.notif_app_size;
    let corner = lock.notif_row_corner;
    let padding_h = lock.notif_row_padding_h;

    // 1) Rasteriza o texto pra saber a largura final da pill.
    let (text_px, text_w, text_h) =
        text_renderer.rasterize_colored(text, font_size, text_color);

    // Tamanho: user_size sobrescreve quando presente (modo de edição
    // "Redimensionar"). Senão, painel dita altura; desktop = 40px.
    let (pill_w, pill_h) = if let Some((uw, uh)) = ctx.user_size {
        (uw as i32, uh as i32)
    } else {
        let h = match ctx.host {
            WidgetHost::Panel { height } => height as i32,
            WidgetHost::Desktop => 40,
        };
        (padding_h + text_w as i32 + padding_h, h)
    };

    // 2) Pill arredondada (mesmo helper que o resto do compositor usa —
    //    alpha=0 fora dos cantos). Reconstruímos os pixels manualmente
    //    porque o MemoryRenderBuffer não expõe os bytes pra blend.
    let pill = make_rounded_panel(pill_w, pill_h, corner, pill_color);
    let mut pixels = vec![0u8; (pill_w * pill_h * 4) as usize];

    let r = (pill_color.r() * 255.0).round() as u8;
    let g = (pill_color.g() * 255.0).round() as u8;
    let b = (pill_color.b() * 255.0).round() as u8;
    let a = (pill_color.a() * 255.0).round() as u8;
    let pb = ((r as u16 * a as u16) / 255) as u8;
    let pg = ((g as u16 * a as u16) / 255) as u8;
    let pr = ((b as u16 * a as u16) / 255) as u8;
    let r_sq = corner * corner;
    for y in 0..pill_h {
        for x in 0..pill_w {
            if is_inside_rounded(x, y, pill_w, pill_h, corner, r_sq) {
                let idx = ((y * pill_w + x) * 4) as usize;
                pixels[idx] = pr;
                pixels[idx + 1] = pg;
                pixels[idx + 2] = pb;
                pixels[idx + 3] = a;
            }
        }
    }

    // 3) Compõe o texto por cima da pill (alpha-blend BGRA premultiplicado).
    if text_w > 0 && text_h > 0 {
        let tx0 = (pill_w - text_w as i32) / 2;
        let ty0 = (pill_h - text_h as i32) / 2;
        for ty in 0..text_h as i32 {
            for tx in 0..text_w as i32 {
                let dst_x = tx0 + tx;
                let dst_y = ty0 + ty;
                if dst_x < 0 || dst_y < 0 || dst_x >= pill_w || dst_y >= pill_h {
                    continue;
                }
                let src_idx = ((ty * text_w as i32 + tx) * 4) as usize;
                let sa = text_px[src_idx + 3];
                if sa == 0 {
                    continue;
                }
                let sb = text_px[src_idx];
                let sg = text_px[src_idx + 1];
                let sr = text_px[src_idx + 2];
                let dst_idx = ((dst_y * pill_w + dst_x) * 4) as usize;
                let inv = 255u16 - sa as u16;
                pixels[dst_idx] =
                    ((sb as u16 * 255 + pixels[dst_idx] as u16 * inv) / 255) as u8;
                pixels[dst_idx + 1] =
                    ((sg as u16 * 255 + pixels[dst_idx + 1] as u16 * inv) / 255) as u8;
                pixels[dst_idx + 2] =
                    ((sr as u16 * 255 + pixels[dst_idx + 2] as u16 * inv) / 255) as u8;
                pixels[dst_idx + 3] =
                    (sa as u16 + (pixels[dst_idx + 3] as u16 * inv) / 255) as u8;
            }
        }
    }

    let _ = pill; // helper só serviu pra documentar a forma esperada
    MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (pill_w, pill_h),
        1,
        Transform::Normal,
        None,
    )
}

/// Mesma lógica do gfx::is_inside_rounded — duplicada localmente
/// porque a função original é pub(crate) no gfx e a widget compõe
/// a pill manualmente em vez de usar o helper (pra alpha-blend texto
/// em cima). Quando a fatia 2 entrar, ou expõe-se o helper ou move-se
/// pra um crate de gfx compartilhado.
fn is_inside_rounded(x: i32, y: i32, w: i32, h: i32, radius: i32, r_sq: i32) -> bool {
    if radius <= 0 {
        return true;
    }
    let in_left = x < radius;
    let in_right = x >= w - radius;
    let in_top = y < radius;
    let in_bottom = y >= h - radius;
    if !(in_left || in_right) || !(in_top || in_bottom) {
        return true;
    }
    let cx = if in_left { radius } else { w - radius - 1 };
    let cy = if in_top { radius } else { h - radius - 1 };
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= r_sq
}

// ─── Fatia 2: registro + parsing do layout.toml ──────────────────────

/// Erro de configuração de widget. String-based pra POC. Os builders
/// devolvem isso quando o bloco config tem campo faltando, tipo
/// errado, valor fora de range, etc.
#[derive(Debug)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ConfigError {}

/// Função construtora de um widget a partir do bloco config dele.
/// Closures (não fn ptr puro) porque alguns widgets dependem de
/// recursos do boot (TextRenderer compartilhado, etc) — o registro
/// segura essas captures quando o builder é registrado.
pub type WidgetBuilder =
    Box<dyn Fn(&toml::Table) -> Result<Box<dyn Widget + Send>, ConfigError>>;

/// Tipo de dado de um campo de config do widget. Determina qual
/// input o formulário do Configurar mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFieldType {
    String,
    Int,
    Bool,
}

/// Schema de um campo de config. O formulário "Configurar" do modo
/// de edição renderiza um input por campo, tipo determinado por
/// `field_type`. O valor atual é lido do config block do widget vivo.
#[derive(Debug, Clone)]
pub struct ConfigFieldSchema {
    pub key: String,
    pub field_type: ConfigFieldType,
    pub label: String,
    pub default: toml::Value,
}

/// Metadata de um tipo de widget no registro. Usado pelo seletor
/// "Adicionar widget" e pelo cartão "Configurar".
pub struct WidgetTypeMeta {
    pub type_id: String,
    pub display_name: String,
    pub description: String,
    pub default_config: toml::Table,
    pub schema: Vec<ConfigFieldSchema>,
    builder: WidgetBuilder,
}

/// Mapa de "tipo no toml" pra builder+metadata. Construído uma vez
/// no boot, imutável depois. Quando widgets de terceiros entrarem
/// (pós-v1), o registro vira o ponto de plug-in.
#[derive(Default)]
pub struct WidgetRegistry {
    types: Vec<WidgetTypeMeta>,
    index: HashMap<String, usize>,
}

impl WidgetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(
        &mut self,
        type_id: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        default_config: toml::Table,
        schema: Vec<ConfigFieldSchema>,
        builder: F,
    ) where
        F: Fn(&toml::Table) -> Result<Box<dyn Widget + Send>, ConfigError> + 'static,
    {
        let id = type_id.into();
        let idx = self.types.len();
        self.types.push(WidgetTypeMeta {
            type_id: id.clone(),
            display_name: display_name.into(),
            description: description.into(),
            default_config,
            schema,
            builder: Box::new(builder),
        });
        self.index.insert(id, idx);
    }

    /// Constrói um widget pelo nome do tipo. Erro = tipo desconhecido
    /// OU builder devolveu config inválida.
    pub fn build(
        &self,
        type_name: &str,
        config: &toml::Table,
    ) -> Result<Box<dyn Widget + Send>, ConfigError> {
        match self.index.get(type_name) {
            Some(&idx) => (self.types[idx].builder)(config),
            None => Err(ConfigError(format!(
                "tipo de widget desconhecido: {type_name}"
            ))),
        }
    }

    /// Lista de tipos registrados (pra o seletor de adicionar).
    pub fn type_list(&self) -> &[WidgetTypeMeta] {
        &self.types
    }

    /// Busca metadata de um tipo pelo id.
    pub fn get_meta(&self, type_id: &str) -> Option<&WidgetTypeMeta> {
        self.index.get(type_id).map(|&idx| &self.types[idx])
    }
}

/// Onde o widget mora — versão TOML de luma_widget::WidgetHost.
/// `{ host = "panel" }` (default) ou `{ host = "desktop", x = N, y = M }`.
/// Internally-tagged via serde: o campo `host` discrimina a variante.
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(tag = "host", rename_all = "lowercase")]
pub enum Placement {
    #[default]
    Panel,
    Desktop {
        x: i32,
        y: i32,
        #[serde(default)]
        width: Option<i32>,
        #[serde(default)]
        height: Option<i32>,
    },
}

/// Entrada do layout.toml — antes do construtor rodar. O `type`
/// resolve o builder; o `placement` decide onde o widget vai
/// (painel ou desktop); o `config` é entregue cru pro builder.
/// `id` é o identificador estável — se ausente no arquivo, gerado
/// na carga e escrito no próximo save.
#[derive(Debug, serde::Deserialize)]
pub struct LayoutEntry {
    /// Identificador estável. Curto, editável na mão ("w1", "w2").
    /// Se ausente no toml, gerado pelo load e escrito no próximo save.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub placement: Placement,
    #[serde(default)]
    pub config: toml::Table,
}

#[derive(Debug, serde::Deserialize, Default)]
struct LayoutFile {
    #[serde(default, rename = "widget")]
    widgets: Vec<LayoutEntry>,
}

/// Path canônico do layout.toml: `~/.config/luma/layout.toml`.
/// Retorna None se `$HOME` não existir (não deveria, mas seguro).
pub fn default_layout_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut p = PathBuf::from(home);
    p.push(".config");
    p.push("luma");
    p.push("layout.toml");
    Some(p)
}

/// Lê + parseia o layout.toml. Comportamentos resilientes:
/// - Arquivo ausente: Vec vazio, log info (estado normal pra novo user)
/// - I/O error: Vec vazio, log warn
/// - TOML mal formado: Vec vazio, log warn (NÃO derruba o boot)
///
/// Entradas com tipo desconhecido NÃO são filtradas aqui — esse
/// load só faz parse estrutural. O registro decide o que é válido
/// quando `build()` rodar.
pub fn load_layout(path: &Path) -> Vec<LayoutEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!("layout.toml ausente em {:?} — sem widgets", path);
            return Vec::new();
        }
        Err(e) => {
            tracing::warn!("layout.toml em {:?} ilegível: {e}", path);
            return Vec::new();
        }
    };
    match toml::from_str::<LayoutFile>(&content) {
        Ok(f) => f.widgets,
        Err(e) => {
            tracing::warn!("layout.toml em {:?} mal formado: {e}", path);
            Vec::new()
        }
    }
}

/// Fatia 2: registry com todos os widgets nativos do Luma. Conhece
/// "label" por enquanto; cresce com cada novo tipo. O `text_renderer`
/// fica capturado nas closures dos builders que precisam dele —
/// caller passa um único Arc e o registro segura as cópias.
/// Garante que toda entrada [[widget]] no doc toml_edit tenha um campo
/// `id`. Usa a lista de ids gerados no load (all_ids, indexada por
/// posição no doc) pra preencher os faltantes.
fn ensure_ids_in_doc(
    doc: &mut toml_edit::DocumentMut,
    all_ids: &[String],
) {
    if let Some(widgets) = doc
        .get_mut("widget")
        .and_then(|i| i.as_array_of_tables_mut())
    {
        for (i, table) in widgets.iter_mut().enumerate() {
            if !table.contains_key("id") {
                if let Some(id) = all_ids.get(i) {
                    table.insert("id", toml_edit::value(id.as_str()));
                    table.sort_values();
                }
            }
        }
    }
}

/// Abre o layout.toml via toml_edit, escreve ids faltantes, e
/// retorna o doc mutável + path. Shared entre todas as funções de save.
fn open_layout_for_write(
    all_ids: &[String],
) -> Result<(toml_edit::DocumentMut, std::path::PathBuf), String> {
    let path = default_layout_path()
        .ok_or_else(|| "layout.toml: $HOME ausente".to_string())?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("layout.toml ilegível: {e}"))?;
    let mut doc: toml_edit::DocumentMut = content
        .parse()
        .map_err(|e| format!("layout.toml mal formado: {e}"))?;
    ensure_ids_in_doc(&mut doc, all_ids);
    Ok((doc, path))
}

/// Encontra a entrada [[widget]] pelo id no doc toml_edit. Retorna
/// o índice no ArrayOfTables (pra get_mut depois).
fn find_entry_by_id(
    doc: &toml_edit::DocumentMut,
    id: &str,
) -> Option<usize> {
    let widgets = doc
        .get("widget")?
        .as_array_of_tables()?;
    widgets
        .iter()
        .position(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))
}

/// Modo de edição: atualiza placement.x e placement.y de um widget
/// no layout.toml, mirando pela `id` (fatia 2.2+). Preserva
/// comentários, formatação e outras entradas.
pub fn save_desktop_widget_position(
    id: &str,
    x: i32,
    y: i32,
    all_ids: &[String],
) -> Result<(), String> {
    let (mut doc, path) = open_layout_for_write(all_ids)?;
    let idx = find_entry_by_id(&doc, id)
        .ok_or_else(|| format!("id '{id}' não encontrado no layout.toml"))?;
    let widgets = doc
        .get_mut("widget")
        .and_then(|i| i.as_array_of_tables_mut())
        .ok_or_else(|| "layout.toml sem [[widget]]".to_string())?;
    let entry = widgets.get_mut(idx).unwrap();
    let placement = entry
        .get_mut("placement")
        .ok_or_else(|| "entrada sem `placement`".to_string())?;
    if let Some(inline) = placement.as_inline_table_mut() {
        inline.insert("x", toml_edit::Value::from(x as i64));
        inline.insert("y", toml_edit::Value::from(y as i64));
    } else if let Some(table) = placement.as_table_mut() {
        table["x"] = toml_edit::value(x as i64);
        table["y"] = toml_edit::value(y as i64);
    } else {
        return Err("`placement` não é tabela".to_string());
    }
    std::fs::write(&path, doc.to_string())
        .map_err(|e| format!("layout.toml write falhou: {e}"))?;
    Ok(())
}

/// Modo de edição fatia 2.2: salva a ordem nova dos widgets de painel
/// no layout.toml. Entradas de painel (cujo id aparece em `panel_ids`)
/// são reordenadas pra refletir a tela. Entradas de desktop e falhas
/// ficam ao final, ordem original preservada.
///
/// Chama `ensure_ids_in_doc` antes, então ids faltantes são gravados.
pub fn save_panel_order(
    panel_ids: &[String],
    all_ids: &[String],
) -> Result<(), String> {
    let (mut doc, path) = open_layout_for_write(all_ids)?;
    let widgets = doc
        .get_mut("widget")
        .and_then(|i| i.as_array_of_tables_mut())
        .ok_or_else(|| "layout.toml sem [[widget]]".to_string())?;
    // Clone todas as entradas e limpa o array pra reordenar.
    let entries: Vec<toml_edit::Table> = widgets.iter().cloned().collect();
    while widgets.len() > 0 {
        widgets.remove(0);
    }
    // Separa: panel (cujo id está em panel_ids) vs resto.
    let panel_set: std::collections::HashSet<&str> =
        panel_ids.iter().map(|s| s.as_str()).collect();
    let mut panel_map: std::collections::HashMap<String, toml_edit::Table> =
        std::collections::HashMap::new();
    let mut others: Vec<toml_edit::Table> = Vec::new();
    for entry in entries {
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if panel_set.contains(id.as_str()) {
            panel_map.insert(id, entry);
        } else {
            others.push(entry);
        }
    }
    // Re-insere: painel na ordem de panel_ids, depois o resto.
    for id in panel_ids {
        if let Some(entry) = panel_map.remove(id.as_str()) {
            widgets.push(entry);
        }
    }
    for entry in others {
        widgets.push(entry);
    }
    std::fs::write(&path, doc.to_string())
        .map_err(|e| format!("layout.toml write falhou: {e}"))?;
    Ok(())
}

/// Modo de edição fatia 2.3: remove uma entrada [[widget]] do
/// layout.toml por id. Preserva comentários e outras entradas.
pub fn remove_widget_by_id(
    id: &str,
    all_ids: &[String],
) -> Result<(), String> {
    let (mut doc, path) = open_layout_for_write(all_ids)?;
    let idx = find_entry_by_id(&doc, id)
        .ok_or_else(|| format!("id '{id}' não encontrado no layout.toml"))?;
    let widgets = doc
        .get_mut("widget")
        .and_then(|i| i.as_array_of_tables_mut())
        .ok_or_else(|| "layout.toml sem [[widget]]".to_string())?;
    widgets.remove(idx);
    std::fs::write(&path, doc.to_string())
        .map_err(|e| format!("layout.toml write falhou: {e}"))?;
    Ok(())
}

/// Modo de edição fatia 2.4: appenda uma entrada [[widget]] NOVA no
/// fim do layout.toml via toml_edit. Preserva arquivo existente.
/// Retorna o id gerado.
pub fn append_widget(
    type_id: &str,
    placement: &Placement,
    config: &toml::Table,
    all_ids: &[String],
) -> Result<String, String> {
    let (mut doc, path) = open_layout_for_write(all_ids)?;
    // Gera id único.
    let existing: std::collections::HashSet<String> = {
        let w = doc.get("widget").and_then(|i| i.as_array_of_tables());
        w.map(|arr| arr.iter()
            .filter_map(|t| t.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect())
        .unwrap_or_default()
    };
    let mut next = 1u32;
    let new_id = loop {
        let candidate = format!("w{next}");
        next += 1;
        if !existing.contains(&candidate) && !all_ids.contains(&candidate) {
            break candidate;
        }
    };
    // Monta a table da nova entrada.
    let mut entry = toml_edit::Table::new();
    entry.insert("id", toml_edit::value(new_id.as_str()));
    entry.insert("type", toml_edit::value(type_id));
    // Placement como inline table.
    let mut pl = toml_edit::InlineTable::new();
    match placement {
        Placement::Panel => {
            pl.insert("host", toml_edit::Value::from("panel"));
        }
        Placement::Desktop { x, y, .. } => {
            pl.insert("host", toml_edit::Value::from("desktop"));
            pl.insert("x", toml_edit::Value::from(*x as i64));
            pl.insert("y", toml_edit::Value::from(*y as i64));
        }
    }
    entry.insert("placement", toml_edit::Item::Value(toml_edit::Value::InlineTable(pl)));
    // Config como inline table.
    let mut cfg_inline = toml_edit::InlineTable::new();
    for (k, v) in config {
        if let Some(tv) = toml_value_to_edit(v) {
            cfg_inline.insert(k, tv);
        }
    }
    entry.insert("config", toml_edit::Item::Value(toml_edit::Value::InlineTable(cfg_inline)));
    // Append.
    let widgets = doc
        .entry("widget")
        .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or_else(|| "widget não é array de tabelas".to_string())?;
    widgets.push(entry);
    std::fs::write(&path, doc.to_string())
        .map_err(|e| format!("layout.toml write falhou: {e}"))?;
    Ok(new_id)
}

/// Modo de edição fatia 2.5: substitui o bloco config de uma entrada
/// [[widget]] no layout.toml por um Table novo. Mira por id.
pub fn save_widget_config(
    id: &str,
    new_config: &toml::Table,
    all_ids: &[String],
) -> Result<(), String> {
    let (mut doc, path) = open_layout_for_write(all_ids)?;
    let idx = find_entry_by_id(&doc, id)
        .ok_or_else(|| format!("id '{id}' não encontrado"))?;
    let widgets = doc
        .get_mut("widget")
        .and_then(|i| i.as_array_of_tables_mut())
        .ok_or_else(|| "sem [[widget]]".to_string())?;
    let entry = widgets.get_mut(idx).unwrap();
    let mut cfg_inline = toml_edit::InlineTable::new();
    for (k, v) in new_config {
        if let Some(tv) = toml_value_to_edit(v) {
            cfg_inline.insert(k, tv);
        }
    }
    entry.insert("config", toml_edit::Item::Value(toml_edit::Value::InlineTable(cfg_inline)));
    std::fs::write(&path, doc.to_string())
        .map_err(|e| format!("write falhou: {e}"))?;
    Ok(())
}

/// Modo de edição fatia 2.6: atualiza width e height na placement
/// de um widget desktop no layout.toml, por id.
pub fn save_widget_size(
    id: &str,
    width: i32,
    height: i32,
    all_ids: &[String],
) -> Result<(), String> {
    let (mut doc, path) = open_layout_for_write(all_ids)?;
    let idx = find_entry_by_id(&doc, id)
        .ok_or_else(|| format!("id '{id}' não encontrado"))?;
    let widgets = doc
        .get_mut("widget")
        .and_then(|i| i.as_array_of_tables_mut())
        .ok_or_else(|| "sem [[widget]]".to_string())?;
    let entry = widgets.get_mut(idx).unwrap();
    let placement = entry
        .get_mut("placement")
        .ok_or_else(|| "entrada sem placement".to_string())?;
    if let Some(inline) = placement.as_inline_table_mut() {
        inline.insert("width", toml_edit::Value::from(width as i64));
        inline.insert("height", toml_edit::Value::from(height as i64));
    } else if let Some(table) = placement.as_table_mut() {
        table["width"] = toml_edit::value(width as i64);
        table["height"] = toml_edit::value(height as i64);
    }
    std::fs::write(&path, doc.to_string())
        .map_err(|e| format!("write falhou: {e}"))?;
    Ok(())
}

fn toml_value_to_edit(v: &toml::Value) -> Option<toml_edit::Value> {
    match v {
        toml::Value::String(s) => Some(toml_edit::Value::from(s.as_str())),
        toml::Value::Integer(i) => Some(toml_edit::Value::from(*i)),
        toml::Value::Float(f) => Some(toml_edit::Value::from(*f)),
        toml::Value::Boolean(b) => Some(toml_edit::Value::from(*b)),
        _ => None,
    }
}

/// Resultado da carga do layout.toml — widgets já distribuídos por host.
/// Panel vai pra fileira do painel (state.widgets); desktop vai pro Space
/// como SpaceItem::DesktopWidget no x,y de cada entrada.
pub struct WidgetLoadResult {
    /// (widget, id)
    pub panel: Vec<(Box<dyn Widget + Send>, String)>,
    /// (widget, x, y, id, user_size)
    pub desktop: Vec<(Box<dyn Widget + Send>, i32, i32, String, Option<(u32, u32)>)>,
    /// IDs de TODAS as entradas [[widget]] do arquivo, na ordem do
    /// doc (inclusive as que falharam no build). Necessário pro
    /// ensure_ids_in_doc do save, que grava ids faltantes.
    pub all_ids: Vec<String>,
}

/// Config de um painel, lida de [[panel]] no layout.toml.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PanelConfig {
    #[serde(default = "default_panel_id")]
    pub id: String,
    #[serde(default = "default_panel_height")]
    pub height: i32,
    #[serde(default = "default_panel_alignment")]
    pub alignment: String,
    #[serde(default)]
    pub floating: bool,
}

fn default_panel_id() -> String { "main".into() }
fn default_panel_height() -> i32 { 48 }
fn default_panel_alignment() -> String { "center".into() }

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            id: default_panel_id(),
            height: default_panel_height(),
            alignment: default_panel_alignment(),
            floating: false,
        }
    }
}

#[derive(Debug, serde::Deserialize, Default)]
struct PanelLayoutFile {
    #[serde(default, rename = "panel")]
    panels: Vec<PanelConfig>,
}

/// Lê a config do painel principal do layout.toml. Se não existir,
/// retorna defaults (comportamento idêntico ao pré-3.1).
pub fn load_panel_config() -> PanelConfig {
    let path = match default_layout_path() {
        Some(p) => p,
        None => return PanelConfig::default(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return PanelConfig::default(),
    };
    match toml::from_str::<PanelLayoutFile>(&content) {
        Ok(f) => f.panels.into_iter()
            .find(|p| p.id == "main")
            .unwrap_or_default(),
        Err(_) => PanelConfig::default(),
    }
}

/// Salva a config do painel principal no layout.toml via toml_edit.
/// Cria a seção [[panel]] se não existir. Mira por id = "main".
pub fn save_panel_config(cfg: &PanelConfig) -> Result<(), String> {
    let path = default_layout_path()
        .ok_or_else(|| "$HOME ausente".to_string())?;
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = content
        .parse()
        .map_err(|e| format!("parse falhou: {e}"))?;
    // Procura [[panel]] existente com id "main".
    let idx = doc.get("panel")
        .and_then(|i| i.as_array_of_tables())
        .and_then(|arr| arr.iter().position(|t|
            t.get("id").and_then(|v| v.as_str()) == Some("main")
        ));
    if let Some(idx) = idx {
        let panels = doc.get_mut("panel")
            .and_then(|i| i.as_array_of_tables_mut()).unwrap();
        let entry = panels.get_mut(idx).unwrap();
        entry.insert("height", toml_edit::value(cfg.height as i64));
        entry.insert("alignment", toml_edit::value(cfg.alignment.as_str()));
        entry.insert("floating", toml_edit::value(cfg.floating));
    } else {
        // Cria [[panel]] nova.
        let mut entry = toml_edit::Table::new();
        entry.insert("id", toml_edit::value("main"));
        entry.insert("height", toml_edit::value(cfg.height as i64));
        entry.insert("alignment", toml_edit::value(cfg.alignment.as_str()));
        entry.insert("floating", toml_edit::value(cfg.floating));
        let panels = doc
            .entry("panel")
            .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .ok_or_else(|| "panel não é array".to_string())?;
        panels.push(entry);
    }
    std::fs::write(&path, doc.to_string())
        .map_err(|e| format!("write falhou: {e}"))?;
    Ok(())
}

/// Preenche ids faltantes. "wN" onde N é o menor inteiro livre.
fn generate_ids(entries: &mut [LayoutEntry]) {
    let existing: std::collections::HashSet<String> = entries
        .iter()
        .filter_map(|e| e.id.clone())
        .collect();
    let mut next = 1u32;
    for entry in entries.iter_mut() {
        if entry.id.is_none() {
            let id = loop {
                let candidate = format!("w{next}");
                next += 1;
                if !existing.contains(&candidate) {
                    break candidate;
                }
            };
            entry.id = Some(id);
        }
    }
}

/// Pipeline completo de carga: registro default + parsing + dispatch
/// por placement. Caller (LumaState::new) consome o resultado.
/// Entradas inválidas (tipo desconhecido, config errada) são puladas
/// com warn — boot nunca derruba.
pub fn load_all(text_renderer: Arc<TextRenderer>) -> (WidgetLoadResult, WidgetRegistry) {
    let registry = build_default_registry(text_renderer);
    let mut entries = default_layout_path()
        .map(|p| load_layout(&p))
        .unwrap_or_default();
    generate_ids(&mut entries);
    tracing::info!("widgets: {} entrada(s) no layout.toml", entries.len());
    let all_ids: Vec<String> = entries.iter()
        .map(|e| e.id.clone().unwrap_or_default())
        .collect();
    let mut panel = Vec::new();
    let mut desktop = Vec::new();
    for entry in entries {
        let id = entry.id.clone().unwrap_or_default();
        match registry.build(&entry.type_name, &entry.config) {
            Ok(w) => match entry.placement {
                Placement::Panel => panel.push((w, id)),
                Placement::Desktop { x, y, width, height } => {
                    let us = match (width, height) {
                        (Some(w_), Some(h_)) => Some((w_ as u32, h_ as u32)),
                        _ => None,
                    };
                    desktop.push((w, x, y, id, us));
                }
            },
            Err(e) => tracing::warn!(
                "widget pulado (type={}): {e}", entry.type_name,
            ),
        }
    }
    (WidgetLoadResult { panel, desktop, all_ids }, registry)
}

pub fn build_default_registry(text_renderer: Arc<TextRenderer>) -> WidgetRegistry {
    let mut registry = WidgetRegistry::new();
    {
        let tr = text_renderer.clone();
        let mut def = toml::Table::new();
        def.insert("text".into(), toml::Value::String("Novo rótulo".into()));
        registry.register(
            "label",
            "Rótulo",
            "Texto fixo numa pill Golden Hour",
            def,
            vec![ConfigFieldSchema {
                key: "text".into(),
                field_type: ConfigFieldType::String,
                label: "Texto".into(),
                default: toml::Value::String("Novo rótulo".into()),
            }],
            move |cfg| {
                LabelWidget::from_config(cfg, tr.clone())
                    .map(|w| Box::new(w) as Box<dyn Widget + Send>)
            },
        );
    }
    {
        let tr = text_renderer.clone();
        let mut def = toml::Table::new();
        def.insert("start".into(), toml::Value::Integer(0));
        registry.register(
            "counter",
            "Contador",
            "Número subindo 1/s — POC de widget com estado",
            def,
            vec![ConfigFieldSchema {
                key: "start".into(),
                field_type: ConfigFieldType::Int,
                label: "Começar em".into(),
                default: toml::Value::Integer(0),
            }],
            move |cfg| {
                CounterWidget::from_config(cfg, tr.clone())
                    .map(|w| Box::new(w) as Box<dyn Widget + Send>)
            },
        );
    }
    registry
}
