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

/// Cor do **anel de foco** do teclado (passo 2), ciente do [`high_contrast`]
/// E do [`focus_highlight`] (passo 4). Sem hex solto — sai da paleta. Quatro
/// combinações: realce OFF dá âmbar (normal) / borda forte (HC); realce ON
/// puxa a cor mais clara/contrastante (mel no normal, creme puro no HC).
pub fn focus_ring() -> Color {
    let hc = high_contrast();
    let p = Palette::for_contrast(hc);
    to_iced(match (hc, focus_highlight()) {
        (true, true) => p.cream,    // HC + realce: creme puro (máx contraste)
        (true, false) => p.border,  // HC: borda #FFF6EA
        (false, true) => p.honey,   // realce: mel, mais claro/vibrante
        (false, false) => p.amber,  // normal: âmbar
    })
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
