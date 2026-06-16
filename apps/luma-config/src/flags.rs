//! Argumentos de linha de comando do app (clap).
//!
//! `--tab` e `--target` saíam do compositor (menus que abrem o config
//! com contexto). Em M1 são só parseados no boot e ignorados — a
//! navegação por aba e o contexto de destino voltam na 2c. A glue de
//! single-instance via D-Bus (`CosmicFlags`) era do libcosmic e saiu na
//! migração pro iced.

use clap::Parser;

/// CLI do app. Sem args = abre default.
#[derive(Parser, Debug, Clone, Default)]
#[command(name = "luma-config", about = "Configurações do Luma")]
pub struct CliArgs {
    /// Aba inicial: `widgets`, `wallpaper` ou `preset`.
    #[arg(long)]
    pub tab: Option<String>,
    /// Destino contextual pra Adicionar widget: `panel` ou `desktop`.
    #[arg(long)]
    pub target: Option<String>,
}
