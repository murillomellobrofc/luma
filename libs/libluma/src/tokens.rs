//! Camada de tokens semânticos pros apps Luma.
//!
//! Re-exporta os tipos compositor-free do `luma-tokens` (camada
//! semântica accent/background/surface/text) e adiciona o que é
//! específico do lado-app: a conversão pros tipos de cor do iced e as
//! constantes de spacing, tipografia e alvo de toque PCD que os
//! componentes consomem.
//!
//! As cores não têm hex aqui — nascem no `luma-tokens` (default Golden
//! Hour + override do `luma.toml`). Spacing/tipografia/PCD são valores
//! de layout, então vivem em código (constantes).

use iced::Color;

pub use luma_tokens::{Color as TokenColor, Palette, Semantic};

/// Flag global do modo de contraste (HC parte 2). Processo-wide pq o modo é
/// global e os componentes leem via funções livres (`semantic_default`),
/// sem passar o flag em cada chamada. O app seta via [`set_high_contrast`]
/// (no boot lendo o D-Bus e a cada sinal de mudança), e re-renderiza.
static HIGH_CONTRAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// **Alto contraste ativo?** ÚNICO ponto de decisão do modo de contraste —
/// `semantic_default`, o `GoldenHour` e o theme leem daqui, então tudo vira
/// junto quando o app chama [`set_high_contrast`].
pub fn high_contrast() -> bool {
    HIGH_CONTRAST.load(std::sync::atomic::Ordering::Relaxed)
}

/// Liga/desliga o modo de alto contraste do processo (HC parte 2). O app
/// chama isto ao ler o flag real (boot) e a cada sinal D-Bus de mudança;
/// depois re-resolve o `GoldenHour` e re-renderiza.
pub fn set_high_contrast(on: bool) {
    HIGH_CONTRAST.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Paleta semântica resolvida do `luma.toml` (default Golden Hour +
/// overrides), respeitando o [`high_contrast`]. Construir uma vez no
/// `init()` do app e segurar.
pub fn semantic() -> Semantic {
    Semantic::resolve_contrast(high_contrast())
}

/// Semântica SEM ler o `luma.toml` (consts) — pros componentes pegarem
/// `surface`/`text`/`border` sem custo de IO por frame. Respeita o
/// [`high_contrast`]: HC on => paleta HC; off => Golden Hour.
pub fn semantic_default() -> Semantic {
    Semantic::from_palette(&Palette::for_contrast(high_contrast()))
}

/// Token de cor do `luma-tokens` → `Color` do iced. É assim que os
/// componentes consomem um token de cor.
pub fn to_iced(c: TokenColor) -> Color {
    let [r, g, b, a] = c.components();
    Color::from_rgba(r, g, b, a)
}

/// Flag do **Realçar foco do teclado** (Teclado passo 4). Quando ligado, o
/// anel de foco fica mais forte (mais grosso + cor mais contrastante). Igual
/// o [`high_contrast`]: processo-wide, o app seta no boot/sinal e re-renderiza.
static FOCUS_HIGHLIGHT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Realçar foco ativo? O [`focus_ring`]/[`focus_ring_width`] leem daqui.
pub fn focus_highlight() -> bool {
    FOCUS_HIGHLIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Liga/desliga o realce de foco do processo (passo 4). O app chama ao ler o
/// flag real (boot) e a cada sinal D-Bus de mudança; depois re-renderiza.
pub fn set_focus_highlight(on: bool) {
    FOCUS_HIGHLIGHT.store(on, std::sync::atomic::Ordering::Relaxed);
}

// ─── Acento curado (modelo Cinnamon) ──────────────────────────────────

/// **Acento curado**: um conjunto fixo de hues do espírito Golden Hour, cada
/// um pré-verificado AAA contra os fundos no normal E no HC (usa a variante
/// HC quando ligado). Cinnamon cura o acento de propósito — cor arbitrária
/// quebra legibilidade; cor livre fica pro caminho avançado (futuro).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    Amber,
    Coral,
    Honey,
    Sage,
    Terracota,
    Rosa,
    Lavanda,
}

impl Accent {
    /// O conjunto curado, na ordem de exibição na página.
    pub const ALL: [Accent; 7] = [
        Accent::Amber,
        Accent::Coral,
        Accent::Honey,
        Accent::Sage,
        Accent::Terracota,
        Accent::Rosa,
        Accent::Lavanda,
    ];

    /// Id estável (D-Bus/persist) — nunca muda.
    pub const fn id(self) -> &'static str {
        match self {
            Accent::Amber => "amber",
            Accent::Coral => "coral",
            Accent::Honey => "honey",
            Accent::Sage => "sage",
            Accent::Terracota => "terracota",
            Accent::Rosa => "rosa",
            Accent::Lavanda => "lavanda",
        }
    }

    /// Nome pra UI (pt-BR).
    pub const fn label(self) -> &'static str {
        match self {
            Accent::Amber => "Âmbar",
            Accent::Coral => "Coral",
            Accent::Honey => "Mel",
            Accent::Sage => "Sálvia",
            Accent::Terracota => "Terracota",
            Accent::Rosa => "Rosa",
            Accent::Lavanda => "Lavanda",
        }
    }

    /// Resolve o id pro acento; desconhecido cai no default (Âmbar).
    pub fn from_id(id: &str) -> Accent {
        Accent::ALL
            .into_iter()
            .find(|a| a.id() == id)
            .unwrap_or(Accent::Amber)
    }

    /// Token de cor do acento na paleta do modo (hc-aware). Sem hex solto.
    pub fn token(self, hc: bool) -> TokenColor {
        let p = Palette::for_contrast(hc);
        match self {
            Accent::Amber => p.amber,
            Accent::Coral => p.coral,
            Accent::Honey => p.honey,
            Accent::Sage => p.sage,
            Accent::Terracota => p.terracota,
            Accent::Rosa => p.rosa,
            Accent::Lavanda => p.lavanda,
        }
    }

    /// Cor iced do acento pro modo atual (pra amostra da página).
    pub fn color(self) -> Color {
        to_iced(self.token(high_contrast()))
    }
}

/// Acento escolhido (runtime), guardado como índice em [`Accent::ALL`].
/// Default 0 = Âmbar. Igual `high_contrast`/`focus_highlight`: o app seta no
/// boot/sinal e re-renderiza.
static ACCENT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Acento escolhido agora.
pub fn current_accent() -> Accent {
    let i = ACCENT.load(std::sync::atomic::Ordering::Relaxed);
    Accent::ALL.get(i).copied().unwrap_or(Accent::Amber)
}

/// Id do acento escolhido (pra escrever no D-Bus).
pub fn accent_id() -> &'static str {
    current_accent().id()
}

/// Seta o acento do processo (boot/sinal). Índice em [`Accent::ALL`].
pub fn set_accent(a: Accent) {
    let i = Accent::ALL.iter().position(|x| *x == a).unwrap_or(0);
    ACCENT.store(i, std::sync::atomic::Ordering::Relaxed);
}

/// Seta o acento por id (string do D-Bus); desconhecido → Âmbar.
pub fn set_accent_id(id: &str) {
    set_accent(Accent::from_id(id));
}

/// **Cor de acento** resolvida (escolha + hc). Fonte única do destaque que os
/// componentes consomem: toggle ligado, fill do slider, anel de foco, glifo
/// de voltar, borda de foco da busca. Default âmbar.
pub fn accent() -> Color {
    current_accent().color()
}

/// Mistura linear de duas cores iced (`t` de 0=a a 1=b). Helper local pro
/// anel forte clarear o acento sem hex solto.
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// Cor do **anel de foco** do teclado (passo 2), agora baseada no [`accent`]
/// escolhido (passo "acento curado"). Realce ON ([`focus_highlight`], passo 4)
/// clareia o acento em direção ao creme pra ficar inconfundível; OFF usa o
/// acento puro. Hc-aware via o próprio `accent()`.
pub fn focus_ring() -> Color {
    let base = accent();
    if focus_highlight() {
        let cream = to_iced(Palette::for_contrast(high_contrast()).cream);
        mix(base, cream, 0.45)
    } else {
        base
    }
}

/// Espessura do anel de foco (px). Realce ON engrossa pra ficar inconfundível.
pub fn focus_ring_width() -> f32 {
    if focus_highlight() {
        4.0
    } else {
        2.0
    }
}

// ─── Alvo de toque (PCD) ──────────────────────────────────────────────

/// Baseline do hit target — WCAG AAA = 44px. É o ÂNCORA da escala: no
/// default `hit_target_min()` == este valor, então `hit_scale()` == 1.0 e
/// os componentes ficam idênticos ao de hoje. Mesmo valor de
/// `accessibility.hit_target_min` do `luma-theme` (quadrados 44×44 da
/// taskbar).
pub const HIT_TARGET_MIN: u16 = 44;

/// Valor de **runtime** do alvo mínimo de toque (parte 1 de Alvos de
/// toque). Os componentes leem por [`hit_target_min`], não por const, então
/// dá pra crescer ao vivo. Default = [`HIT_TARGET_MIN`] (baseline) →
/// idêntico ao de hoje. Parte 2 liga no toggle + slider.
static HIT_TARGET: std::sync::atomic::AtomicU16 =
    std::sync::atomic::AtomicU16::new(HIT_TARGET_MIN);

/// Alvo mínimo de toque atual (px). Função LIVRE — os componentes leem
/// daqui pra dimensionar a área interativa.
pub fn hit_target_min() -> u16 {
    HIT_TARGET.load(std::sync::atomic::Ordering::Relaxed)
}

/// Seta o alvo mínimo de toque (parte 2 chama no boot/D-Bus). Px.
pub fn set_hit_target_min(px: u16) {
    HIT_TARGET.store(px, std::sync::atomic::Ordering::Relaxed);
}

/// Fator de escala dos alvos de toque: `hit_target_min() / baseline`. 1.0
/// no default (44). Os componentes multiplicam suas dimensões INTERATIVAS
/// (altura de linha, toggle, slider, quadrado do tile, botão de voltar) por
/// isto — assim crescer o alvo escala a UI tocável proporcionalmente e o
/// default fica idêntico. Não escala texto nem padding decorativo.
pub fn hit_scale() -> f32 {
    hit_target_min() as f32 / HIT_TARGET_MIN as f32
}

// ─── Spacing (escala 4px) ─────────────────────────────────────────────

pub const SPACE_XS: u16 = 4;
pub const SPACE_SM: u16 = 8;
pub const SPACE_MD: u16 = 12;
pub const SPACE_LG: u16 = 16;
pub const SPACE_XL: u16 = 24;

// ─── Tipografia ───────────────────────────────────────────────────────

/// Família de display (títulos grandes) — espelha o uso do compositor
/// (clock/calendário em Fraunces).
pub const FONT_DISPLAY: &str = "Fraunces";
/// Bytes da fonte de display (Fraunces variável, OFL) bundlada no libluma.
/// O app registra via `.font(libluma::tokens::DISPLAY_FONT_BYTES)` no boot
/// do iced pra o [`FONT_DISPLAY`] renderizar de verdade.
pub const DISPLAY_FONT_BYTES: &[u8] = include_bytes!("../assets/fraunces.ttf");
/// Família de UI (títulos/labels) — espelha `title_font_family` (Manrope).
pub const FONT_UI: &str = "Manrope";

/// Escala de tamanhos de texto (px), espelhando os tamanhos já em uso.
pub const TEXT_SM: u16 = 12;
pub const TEXT_BASE: u16 = 14;
pub const TEXT_LG: u16 = 18;
pub const TEXT_XL: u16 = 24;
