//! Persistência dos settings de comportamento/acessibilidade do Luma (3b).
//!
//! **O compositor é o dono ÚNICO deste arquivo.** Lê no boot, reescreve
//! quando um set chega via D-Bus. O `luma-config` NUNCA toca aqui — ele só
//! fala via D-Bus (igual no 3a). Um escritor só = uma fonte de verdade só,
//! sem corrida.
//!
//! Path XDG: `$XDG_CONFIG_HOME/luma/settings.toml` (cai pra
//! `~/.config/luma/settings.toml`). Separado do `theme.toml` de propósito —
//! tema é estética curada; isto é comportamento/a11y.
//!
//! Estruturado pra crescer: cada domínio é uma seção. Tudo com `Default` +
//! `#[serde(default)]`, então um arquivo parcial (ou ausente) sempre
//! carrega — campo faltando vira o default.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

/// Raiz do `settings.toml`. Cada seção é um domínio de settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub accessibility: Accessibility,
    pub appearance: Appearance,
}

/// Seção `[appearance]` — estética escolhida pelo usuário. Começa com o
/// **acento curado** (modelo Cinnamon): um id de um conjunto fixo verificado
/// AAA. Cor livre + guardrail virão na camada avançada (futuro).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Id do acento curado ("amber", "coral", "honey", "sage", "terracota",
    /// "rosa", "lavanda"). Default "amber". Os apps re-resolvem o token de
    /// accent a partir disto. Id desconhecido → âmbar (no lado do app).
    pub accent: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            accent: "amber".to_string(),
        }
    }
}

/// Seção `[accessibility]`. 3b persistiu o sino visual; 3c soma o modo
/// fotossensibilidade e a intensidade do flash. Graceful: um `settings.toml`
/// antigo (só `visual_bell_enabled`) carrega liso e ganha os defaults nos
/// campos novos (via `#[serde(default)]` + `Default`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Accessibility {
    /// Sino visual (piscada de tela). Espelha o `flash_enabled` em memória.
    pub visual_bell_enabled: bool,
    /// Modo seguro pra fotossensibilidade (3c). Ligado = a intensidade
    /// efetiva é capada num teto seguro, sem território de estrobo.
    pub photosafe_enabled: bool,
    /// Intensidade do flash em % (3c, 0–100). Escala a força da piscada.
    pub flash_intensity: u8,
    /// Reduzir movimento (3d). Ligado = animações/transições do Luma ficam
    /// instantâneas (sem deslizar/interpolar). Convenção: TODA animação
    /// checa este flag antes de animar. Default OFF.
    pub reduce_motion: bool,
    /// Alto contraste (HC parte 2). Ligado = os apps Luma re-resolvem pra a
    /// paleta HC. Diferente dos outros: muda a APARÊNCIA, então o set emite
    /// um sinal D-Bus e os apps re-renderizam ao vivo. Default OFF.
    pub high_contrast: bool,
    /// Alvos de toque maiores (Alvos parte 2). Ligado = usa
    /// `min_target_size`; desligado = baseline 44. Efetivo calculado pelo
    /// compositor. Default OFF.
    pub larger_targets: bool,
    /// Tamanho do alvo de toque em px quando `larger_targets` está on
    /// (Alvos parte 2). Range 44–72. Default 48.
    pub min_target_size: u16,
    /// Realçar foco do teclado (Teclado passo 4). Ligado = o anel de foco
    /// dos apps fica mais forte. Muda APARÊNCIA → emite sinal D-Bus, igual o
    /// `high_contrast`. O compositor só guarda/serve; quem desenha é o app.
    /// Default OFF.
    pub focus_highlight: bool,
}

impl Default for Accessibility {
    fn default() -> Self {
        // Defaults: sino OFF (igual token `flash_enabled_default`),
        // fotossensibilidade ON (seguro por padrão), intensidade 60%.
        // Só valem quando o arquivo existe mas omite o campo; sem arquivo,
        // o boot usa os defaults do compositor (`load` = None).
        Self {
            visual_bell_enabled: false,
            photosafe_enabled: true,
            flash_intensity: 60,
            reduce_motion: false,
            high_contrast: false,
            larger_targets: false,
            min_target_size: 48,
            focus_highlight: false,
        }
    }
}

/// Carrega o `settings.toml`. `None` se o arquivo não existe (o boot então
/// cai no default/token) ou se não dá pra parsear (loga e ignora — settings
/// corrompido não derruba o compositor).
pub fn load() -> Option<Settings> {
    let path = settings_path()?;
    if !path.exists() {
        info!("settings.toml não encontrado em {path:?}, usando defaults");
        return None;
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => match toml::from_str::<Settings>(&content) {
            Ok(s) => {
                info!("settings carregados de {path:?}");
                Some(s)
            }
            Err(err) => {
                warn!("erro parseando settings.toml: {err}, usando defaults");
                None
            }
        },
        Err(err) => {
            warn!("erro lendo settings.toml: {err}, usando defaults");
            None
        }
    }
}

/// Salva o `settings.toml` de forma **atômica**: grava num `.tmp` e renomeia
/// pro destino. Rename é atômico no mesmo filesystem — nunca deixa um
/// arquivo pela metade se cair no meio da escrita. Cria o diretório se
/// faltar.
pub fn save(settings: &Settings) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    let Some(path) = settings_path() else {
        return Err(Error::new(
            ErrorKind::NotFound,
            "sem XDG_CONFIG_HOME nem HOME pra resolver o path de settings",
        ));
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = toml::to_string_pretty(settings)
        .map_err(|err| Error::new(ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &path)?;
    info!("settings salvos em {path:?}");
    Ok(())
}

/// Resolve o path do `settings.toml` respeitando `XDG_CONFIG_HOME` (cai pra
/// `$HOME/.config`). Mesma lógica leve do `config_path` do luma-theme — sem
/// dep extra.
fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("luma").join("settings.toml"))
}
