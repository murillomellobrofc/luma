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
    /// `text` opcional pra que o AddWidget do D-Bus (3c.1) consiga
    /// criar uma label sem config — usuário edita depois. Tipo errado
    /// continua erro.
    pub fn from_config(
        config: &toml::Table,
        text_renderer: Arc<TextRenderer>,
    ) -> Result<Self, ConfigError> {
        let text = match config.get("text") {
            Some(v) => v
                .as_str()
                .ok_or_else(|| ConfigError("label: `text` precisa ser string".into()))?
                .to_string(),
            None => "Etiqueta".to_string(),
        };
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

    // Altura: painel dita; desktop escolhe (40px é uma altura
    // razoável que combina com a font size atual de 13.5).
    let pill_h = match ctx.host {
        WidgetHost::Panel { height } => height as i32,
        WidgetHost::Desktop => 40,
    };
    let pill_w = padding_h + text_w as i32 + padding_h;

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

/// Descreve um tipo de widget pro usuário (picker do app, futuros
/// pickers). 3c.1: hoje em string fixa pt-BR; i18n vira dívida quando
/// o sistema de localização maturar nos dois lados (compositor + app).
#[derive(Debug, Clone)]
pub struct WidgetMetadata {
    /// Nome humano do tipo (ex.: "Etiqueta", "Contador").
    pub display_name: String,
    /// Frase curta explicando o que o widget faz.
    pub description: String,
    /// Nome do ícone no icon theme freedesktop (ex.: "list-add-symbolic").
    pub icon_name: String,
}

/// Entrada do registro: builder + metadata. Internal — o público
/// itera via `available()` que entrega `(type_name, &metadata)`.
struct RegistryEntry {
    builder: WidgetBuilder,
    metadata: WidgetMetadata,
}

/// Mapa de "tipo no toml" pra builder + metadata. Construído uma vez
/// no boot, imutável depois. Quando widgets de terceiros entrarem
/// (pós-v1), o registro vira o ponto de plug-in.
#[derive(Default)]
pub struct WidgetRegistry {
    entries: HashMap<String, RegistryEntry>,
}

impl WidgetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        type_name: &str,
        builder: WidgetBuilder,
        metadata: WidgetMetadata,
    ) {
        self.entries.insert(
            type_name.to_string(),
            RegistryEntry { builder, metadata },
        );
    }

    /// Constrói um widget pelo nome do tipo. Erro = tipo desconhecido
    /// OU builder devolveu config inválida. Caller (a luma-bar) pula
    /// a entrada + loga warn, em vez de derrubar o painel.
    pub fn build(
        &self,
        type_name: &str,
        config: &toml::Table,
    ) -> Result<Box<dyn Widget + Send>, ConfigError> {
        match self.entries.get(type_name) {
            Some(e) => (e.builder)(config),
            None => Err(ConfigError(format!(
                "tipo de widget desconhecido: {type_name}"
            ))),
        }
    }

    /// Itera os tipos disponíveis com sua metadata. Usado pelo
    /// `ListAvailableWidgets` do D-Bus (3c.1) pra preencher o picker
    /// do app. Ordem do HashMap não é estável — caller que precisar
    /// de ordem determinística ordena ele mesmo.
    pub fn available(&self) -> impl Iterator<Item = (&str, &WidgetMetadata)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), &v.metadata))
    }
}

/// Onde o widget mora — versão TOML de luma_widget::WidgetHost.
/// `{ host = "panel" }` (default) ou `{ host = "desktop", x = N, y = M }`.
/// Internally-tagged via serde: o campo `host` discrimina a variante.
#[derive(Debug, serde::Deserialize, Default)]
#[serde(tag = "host", rename_all = "lowercase")]
pub enum Placement {
    #[default]
    Panel,
    Desktop { x: i32, y: i32 },
}

/// Entrada do layout.toml — antes do construtor rodar. O `type`
/// resolve o builder; o `placement` decide onde o widget vai
/// (painel ou desktop); o `config` é entregue cru pro builder.
#[derive(Debug, serde::Deserialize)]
pub struct LayoutEntry {
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
/// Resultado da carga do layout.toml — widgets já distribuídos por host.
/// Panel vai pra fileira do painel (state.widgets); desktop vai pro Space
/// como SpaceItem::DesktopWidget no x,y de cada entrada.
pub struct WidgetLoadResult {
    pub panel: Vec<Box<dyn Widget + Send>>,
    /// (widget, x, y) — coordenadas lógicas do desktop.
    pub desktop: Vec<(Box<dyn Widget + Send>, i32, i32)>,
}

/// Config de um painel, lida de [[panel]] no layout.toml (read-only).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PanelConfig {
    #[serde(default = "default_main_id")]
    pub id: String,
    #[serde(default = "default_48")]
    pub height: i32,
}
fn default_main_id() -> String { "main".into() }
fn default_48() -> i32 { 48 }

impl Default for PanelConfig {
    fn default() -> Self {
        Self { id: default_main_id(), height: default_48() }
    }
}

#[derive(Debug, serde::Deserialize, Default)]
struct PanelLayoutFile {
    #[serde(default, rename = "panel")]
    panels: Vec<PanelConfig>,
}

/// Lê a config do painel principal do layout.toml. Se não existir,
/// retorna default (height=48). Read-only — sem modo de edição.
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

/// Pipeline completo de carga: registro default + parsing + dispatch
/// por placement. Caller (LumaState::new) consome o resultado.
/// Entradas inválidas (tipo desconhecido, config errada) são puladas
/// com warn — boot nunca derruba.
pub fn load_all(registry: &WidgetRegistry) -> WidgetLoadResult {
    let entries = default_layout_path()
        .map(|p| load_layout(&p))
        .unwrap_or_default();
    tracing::info!("widgets: {} entrada(s) no layout.toml", entries.len());
    let mut panel = Vec::new();
    let mut desktop = Vec::new();
    for entry in entries {
        match registry.build(&entry.type_name, &entry.config) {
            Ok(w) => match entry.placement {
                Placement::Panel => panel.push(w),
                Placement::Desktop { x, y } => desktop.push((w, x, y)),
            },
            Err(e) => tracing::warn!(
                "widget pulado (type={}): {e}", entry.type_name,
            ),
        }
    }
    WidgetLoadResult { panel, desktop }
}

pub fn build_default_registry(text_renderer: Arc<TextRenderer>) -> WidgetRegistry {
    let mut registry = WidgetRegistry::new();
    {
        let tr = text_renderer.clone();
        registry.register(
            "label",
            Box::new(move |cfg| {
                LabelWidget::from_config(cfg, tr.clone())
                    .map(|w| Box::new(w) as Box<dyn Widget + Send>)
            }),
            WidgetMetadata {
                display_name: "Etiqueta".into(),
                description: "Mostra um texto fixo configurável.".into(),
                icon_name: "text-x-generic-symbolic".into(),
            },
        );
    }
    {
        let tr = text_renderer.clone();
        registry.register(
            "counter",
            Box::new(move |cfg| {
                CounterWidget::from_config(cfg, tr.clone())
                    .map(|w| Box::new(w) as Box<dyn Widget + Send>)
            }),
            WidgetMetadata {
                display_name: "Contador".into(),
                description: "Contador simples que sobe um a cada segundo.".into(),
                icon_name: "list-add-symbolic".into(),
            },
        );
    }
    registry
}
