//! Sistema de busca Spotlight-like do menu launcher. H32.9.
//!
//! Estado e tipos pra busca unificada com 4 categorias:
//! - Apps (.desktop indexados)
//! - Pastas (walkdir em $HOME)
//! - Cálculo (meval inline)
//! - Web (xdg-open com query encoded)
//!
//! Lógica de filter/fuzzy/eval vive aqui; render vive em menu.rs e
//! winit.rs como o resto. H32.9.1 só introduz os tipos.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use tracing::info;

/// Resultado de match de app (.desktop). Score do fuzzy-matcher pro
/// ranking; `matched_indices` pro highlight char-a-char (H32.9.4).
#[derive(Debug, Clone)]
pub struct AppResult {
    /// Índice em `LumaState.apps` pra resolver o DesktopApp completo.
    pub app_idx: usize,
    /// Score do fuzzy match — quanto maior, melhor (descending sort).
    pub score: i64,
    /// Posições dos chars matched no `app.name` (pra render highlight).
    pub matched_indices: Vec<usize>,
}

/// Match de pasta/arquivo indexado em $HOME. H32.9.5.
#[derive(Debug, Clone)]
pub struct FolderResult {
    pub path: PathBuf,
    /// Path relativo a $HOME pra display (`~/Documentos/foo`).
    pub display: String,
    pub score: i64,
    pub matched_indices: Vec<usize>,
}

/// Resultado de cálculo inline (meval). H32.9.6.
#[derive(Debug, Clone)]
pub struct CalcResult {
    /// Expressão original como digitada (com normalização visual).
    pub expression: String,
    /// Valor avaliado em f64. Formatado pra display em `display`.
    pub value: f64,
    pub display: String,
}

/// Sugestão de busca web. H32.9.7. URL final é gerada na hora de abrir.
#[derive(Debug, Clone)]
pub struct WebResult {
    /// Query do user, já trimmed (sem prefixo `w ` se houver).
    pub query: String,
    /// Engine resolvida do `theme.search.web_search_engine`.
    pub engine: WebEngine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebEngine {
    DuckDuckGo,
    Google,
    Bing,
}

impl WebEngine {
    pub fn from_token(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "google" => WebEngine::Google,
            "bing" => WebEngine::Bing,
            _ => WebEngine::DuckDuckGo,
        }
    }

    /// Monta a URL final pra `xdg-open`. Query é url-encoded.
    pub fn url(&self, query: &str) -> String {
        let q = urlencoding::encode(query);
        match self {
            WebEngine::DuckDuckGo => format!("https://duckduckgo.com/?q={q}"),
            WebEngine::Google => format!("https://www.google.com/search?q={q}"),
            WebEngine::Bing => format!("https://www.bing.com/search?q={q}"),
        }
    }
}

/// Conjunto de resultados de uma busca. Categorias vazias somem
/// (renderiza só as preenchidas).
#[derive(Debug, Default, Clone)]
pub struct SearchResults {
    pub apps: Vec<AppResult>,
    pub folders: Vec<FolderResult>,
    pub calc: Option<CalcResult>,
    pub web: Option<WebResult>,
}

impl SearchResults {
    pub fn is_empty(&self) -> bool {
        self.apps.is_empty() && self.folders.is_empty() && self.calc.is_none() && self.web.is_none()
    }

    /// Total de items "navegáveis" no ↑/↓. Web e Calc contam como 1.
    pub fn total(&self) -> usize {
        self.apps.len()
            + self.folders.len()
            + self.calc.is_some() as usize
            + self.web.is_some() as usize
    }
}

/// Estado completo da search. Substitui `MenuState.search_text` +
/// `MenuState.filtered_apps` da impl anterior. H32.9.1.
#[derive(Debug, Default)]
pub struct SearchState {
    pub query: String,
    pub results: SearchResults,
    /// Índice global do item selecionado (0 = primeiro app, depois
    /// folders, depois calc, depois web). H32.9.10.
    pub selected_idx: usize,
    /// Marcado como `true` quando query muda e o filter precisa rodar
    /// de novo. Backend confere e limpa após processar.
    pub dirty: bool,
}

/// Sempre retorna WebResult quando query não vazia. Se começa com `w `,
/// strip o prefix (força a busca; reservado pra futuro re-ranking).
/// H32.9.7.
pub fn try_web(query: &str, engine: WebEngine) -> Option<WebResult> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let q = if let Some(stripped) = trimmed.strip_prefix("w ") {
        stripped.trim().to_string()
    } else {
        trimmed.to_string()
    };
    if q.is_empty() {
        return None;
    }
    Some(WebResult { query: q, engine })
}

/// Tenta avaliar `query` como expressão matemática via meval. Retorna
/// `None` quando a query claramente não é matemática (sem operadores
/// nem funções) ou quando meval falha. H32.9.6.
pub fn try_calc(query: &str) -> Option<CalcResult> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !is_likely_math(trimmed) {
        return None;
    }
    let normalized = trimmed.replace(',', ".");
    let value = meval::eval_str(&normalized).ok()?;
    if !value.is_finite() {
        return None;
    }
    let display = if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{trimmed} = {}", value as i64)
    } else {
        format!("{trimmed} = {value:.6}")
    };
    Some(CalcResult {
        expression: trimmed.to_string(),
        value,
        display,
    })
}

fn is_likely_math(s: &str) -> bool {
    const FUNCS: &[&str] = &[
        "sqrt", "sin", "cos", "tan", "log", "ln", "exp", "abs", "pi", "e",
    ];
    let has_op = s
        .chars()
        .any(|c| matches!(c, '+' | '-' | '*' | '/' | '^' | '%' | '('));
    let has_fn = FUNCS.iter().any(|f| s.contains(f));
    if !has_op && !has_fn {
        return false;
    }
    s.chars().all(|c| {
        c.is_ascii_digit()
            || c.is_ascii_alphabetic()
            || matches!(c, '+' | '-' | '*' | '/' | '^' | '%' | '(' | ')' | '.' | ',' | ' ')
    })
}

/// Cache do index de pastas de $HOME. Lazy: indexa na primeira chamada
/// de `ensure_indexed`, refresh quando TTL expira. H32.9.5.
pub struct HomeIndex {
    pub paths: Vec<PathBuf>,
    indexed_at: Option<Instant>,
    cache_ttl: Duration,
}

impl HomeIndex {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            paths: Vec::new(),
            indexed_at: None,
            cache_ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub fn ensure_indexed(&mut self, depth: usize) {
        let stale = self
            .indexed_at
            .map(|t| t.elapsed() > self.cache_ttl)
            .unwrap_or(true);
        if !stale {
            return;
        }
        let home = match std::env::var("HOME") {
            Ok(h) => PathBuf::from(h),
            Err(_) => return,
        };
        self.paths.clear();
        // Skip dirs comuns que poluiriam os resultados.
        let skip = [
            "node_modules",
            "target",
            ".git",
            ".cache",
            ".cargo",
            ".rustup",
            ".npm",
            ".local",
            ".config",
            "snap",
        ];
        for entry in walkdir::WalkDir::new(&home)
            .max_depth(depth)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                !skip.contains(&name.as_str())
            })
            .flatten()
        {
            self.paths.push(entry.path().to_path_buf());
        }
        self.indexed_at = Some(Instant::now());
        info!("home indexed: {} paths em depth {}", self.paths.len(), depth);
    }

    pub fn search(&self, query: &str, max: usize) -> Vec<FolderResult> {
        use fuzzy_matcher::skim::SkimMatcherV2;
        use fuzzy_matcher::FuzzyMatcher;
        if query.is_empty() {
            return Vec::new();
        }
        let home = std::env::var("HOME").ok().unwrap_or_default();
        let home_pb = PathBuf::from(&home);
        let matcher = SkimMatcherV2::default();
        let mut scored: Vec<(PathBuf, String, i64, Vec<usize>)> = Vec::new();
        // Limite de chars do display path pra caber na col direita do
        // menu. Fonte é proporcional, chars largos (M, W) ocupam ~10px;
        // 20 conservador pra garantir que não transborde. H32.9.fix.
        const MAX_DISPLAY_CHARS: usize = 20;
        for path in &self.paths {
            // Match no nome (basename) por velocidade; display é
            // construído depois pra paths matched.
            let basename = match path.file_name().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };
            if let Some((score, indices)) = matcher.fuzzy_indices(basename, query) {
                let raw = if let Ok(rel) = path.strip_prefix(&home_pb) {
                    format!("~/{}", rel.display())
                } else {
                    path.display().to_string()
                };
                let display = if raw.chars().count() > MAX_DISPLAY_CHARS {
                    let suffix: String = raw
                        .chars()
                        .skip(raw.chars().count() - (MAX_DISPLAY_CHARS - 1))
                        .collect();
                    format!("…{suffix}")
                } else {
                    raw
                };
                scored.push((path.clone(), display, score, indices));
            }
        }
        scored.sort_by(|a, b| b.2.cmp(&a.2));
        scored.truncate(max);
        scored
            .into_iter()
            .map(|(path, display, score, indices)| FolderResult {
                path,
                display,
                score,
                matched_indices: indices,
            })
            .collect()
    }
}

impl SearchState {
    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.dirty = true;
        self.selected_idx = 0;
    }

    pub fn pop(&mut self) {
        self.query.pop();
        self.dirty = true;
        self.selected_idx = 0;
    }

    pub fn clear(&mut self) {
        if !self.query.is_empty() {
            self.query.clear();
            self.dirty = true;
            self.selected_idx = 0;
            self.results = SearchResults::default();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.query.is_empty()
    }
}
