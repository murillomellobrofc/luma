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

/// Cache do index de pastas de $HOME. Lazy: indexa na primeira chamada
/// de `ensure_indexed`, refresh quando TTL expira. H32.9.5.
/// Campos internos + ensure_indexed permanecem pra quando o painel
/// de search results voltar a consumir esse índice.
#[allow(dead_code)]
pub struct HomeIndex {
    pub paths: Vec<PathBuf>,
    indexed_at: Option<Instant>,
    cache_ttl: Duration,
}

#[allow(dead_code)]
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

}
