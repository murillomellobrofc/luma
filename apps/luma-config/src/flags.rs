//! Argumentos de ativação do app. Servem 2 fluxos:
//!
//! 1. **Boot**: `main` parseia `std::env::args` em `CliArgs` e passa
//!    `LumaConfigFlags` pra `cosmic::app::run_single_instance`.
//! 2. **Re-ativação**: quando o compositor (ou outro caller) dispara
//!    `luma-config --tab ... --target ...` e o app já tá rodando, o
//!    single-instance do cosmic encaminha os args via D-Bus
//!    (`org.freedesktop.DbusActivation::ActivateAction`). O handler
//!    `Application::dbus_activation` no `AppModel` re-parseia esses
//!    args e troca aba/contexto sem subir nova janela.
//!
//! Decisão (3c.4): mantemos `tab` e `target` como `String` porque a
//! API do `CosmicFlags::args` retorna `Vec<&str>` e é mais limpo
//! emprestar slices do flags do que converter de enum cada vez.
//! Validação/match acontece dentro do `app.rs`.

use clap::Parser;
use cosmic::app::CosmicFlags;

/// CLI do app (clap). Os 2 args saem do compositor (menus da Fatia 1
/// que abrem com contexto). Sem args = abre default (aba Widgets,
/// contexto livre).
#[derive(Parser, Debug, Clone, Default)]
#[command(name = "luma-config", about = "Configurações do Luma")]
pub struct CliArgs {
    /// Aba inicial: `widgets`, `wallpaper` ou `preset`.
    #[arg(long)]
    pub tab: Option<String>,
    /// Destino contextual pra Adicionar widget: `panel` ou `desktop`.
    /// Sem isso, contexto "livre" e o mini-modal pergunta o destino.
    #[arg(long)]
    pub target: Option<String>,
}

/// Marcador de subcomando do `CosmicFlags`. Hoje só `Open` — o
/// trait exige `Some(&SubCommand)` quando `flags.args()` precisa
/// cruzar pra outra instância via D-Bus `ActivateAction`.
#[derive(Debug, Clone)]
pub enum SubCommand {
    Open,
}

impl std::fmt::Display for SubCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SubCommand::Open => "open",
        })
    }
}

#[derive(Debug, Clone)]
pub struct LumaConfigFlags {
    pub cli: CliArgs,
    /// Necessário pra `CosmicFlags::action` retornar `&Self::SubCommand`
    /// — precisa morar em algum campo. `SubCommand::Open` é constante
    /// neste momento, mas guardar como field deixa o ponteiro válido
    /// pelo tempo de vida do flags.
    sub: SubCommand,
}

impl LumaConfigFlags {
    pub fn from_cli(cli: CliArgs) -> Self {
        Self { cli, sub: SubCommand::Open }
    }
}

impl Default for LumaConfigFlags {
    fn default() -> Self {
        Self::from_cli(CliArgs::default())
    }
}

impl CosmicFlags for LumaConfigFlags {
    type SubCommand = SubCommand;
    type Args = Vec<String>;

    fn action(&self) -> Option<&Self::SubCommand> {
        Some(&self.sub)
    }

    fn args(&self) -> Vec<&str> {
        let mut v = Vec::with_capacity(4);
        if let Some(tab) = self.cli.tab.as_deref() {
            v.push("--tab");
            v.push(tab);
        }
        if let Some(target) = self.cli.target.as_deref() {
            v.push("--target");
            v.push(target);
        }
        v
    }
}
