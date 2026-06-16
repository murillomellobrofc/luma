//! luma-tokens — fundação de tokens **compositor-free** do Luma.
//!
//! Mora aqui o que tanto o compositor (`luma-theme`, que adiciona a
//! casca `Color32F`/smithay por cima) quanto os apps (`libluma`,
//! libcosmic) precisam compartilhar SEM arrastar a stack GL do
//! smithay:
//!
//! - [`Color`] — tipo de cor RGBA f32 plano (espelha a API do
//!   `Color32F` do smithay: `new`/`r`/`g`/`b`/`a`/`components`).
//! - [`Palette`] — as cores nomeadas Golden Hour. [`Palette::GOLDEN_HOUR`]
//!   é a fonte única dos VALORES default (os hex moram só aqui, não
//!   mais duplicados no `luma-theme`). [`Palette::load`] resolve os
//!   overrides do `[colors]` no `luma.toml`.
//! - [`Semantic`] — camada semântica fina (accent/background/surface/
//!   text) mapeada sobre a paleta, pros componentes dos apps.
//!
//! Decisão trancada: zero hex nos apps; as cores nascem aqui (default)
//! e do `luma.toml` (override), e fluem pro `luma-theme` e pro
//! `libluma` a partir deste crate.

use std::path::PathBuf;

use serde::Deserialize;

// ─── Color ────────────────────────────────────────────────────────────

/// Cor RGBA, componentes f32 em 0..1. Plano, sem smithay. A API
/// espelha o `Color32F` (new/r/g/b/a/components) pra conversão trivial
/// nas duas pontas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Constrói de bytes RGB (alpha = 1.0). `const` pra usar nos
    /// defaults da paleta.
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
    }

    /// Bytes RGB + alpha já normalizado (0..1).
    pub const fn rgb8_a(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a,
        }
    }

    pub fn r(&self) -> f32 {
        self.r
    }
    pub fn g(&self) -> f32 {
        self.g
    }
    pub fn b(&self) -> f32 {
        self.b
    }
    pub fn a(&self) -> f32 {
        self.a
    }

    /// Clareia em direção ao branco por `f` (0..1), preservando alpha.
    /// Derivação (não hex novo): `coffee.lighten(0.14)` dá o `surface`
    /// elevado (~1.5:1 de contraste contra o coffee), pro card flutuar.
    pub fn lighten(self, f: f32) -> Color {
        Color {
            r: self.r + (1.0 - self.r) * f,
            g: self.g + (1.0 - self.g) * f,
            b: self.b + (1.0 - self.b) * f,
            a: self.a,
        }
    }

    /// `[r, g, b, a]` — mesmo formato do `Color32F::components()`, pra
    /// as pontas converterem sem nomear tipos uma da outra.
    pub fn components(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// Parse de `#RRGGBB` ou `#RRGGBBAA` → [`Color`]. Mesma semântica do
/// parser do `luma-theme` (6 ou 8 dígitos hex; qualquer outra coisa =
/// `None`), pra override de `luma.toml` resolver idêntico nas pontas.
pub fn parse_hex(s: &str) -> Option<Color> {
    let hex = s.trim().trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b, 255u8)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };
    Some(Color::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ))
}

// ─── Palette ──────────────────────────────────────────────────────────

/// Cores nomeadas Golden Hour. Fonte única dos valores (default +
/// override de `luma.toml`). O `luma-theme` consome estes valores pros
/// campos correspondentes da sua `ColorTokens` (mantendo o resto da
/// paleta de componentes do compositor onde está), e o `libluma`
/// consome direto — nenhum dos dois redeclara hex.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// `window_bg_dark` — bg principal escuro (#1F1611).
    pub coffee: Color,
    /// `taskbar_bg_bottom` — base da taskbar (#0F0A07 @0.97).
    /// Override de alpha continua no `luma-theme` (parse_with); aqui só
    /// mora o default, consumido como cor cheia pelos apps.
    pub coffee_deep: Color,
    /// `applet_bg` — surface de blocos sobre o bg (#2A1F1A).
    pub coffee_surface: Color,
    /// `hover_close` — coral, accent destrutivo (#D97757).
    pub coral: Color,
    /// `hover_minimize` — honey, text tint (#F5D5A8).
    pub honey: Color,
    /// `hover_maximize` — sage (#6B8E7F).
    pub sage: Color,
    /// `text_primary` — cream, texto alto contraste (#FBF6EE).
    pub cream: Color,
    /// `accent_amber` — amber, destaque (#E89B3D).
    pub amber: Color,
    /// `text_secondary` — cream apagado (#A89D92).
    pub cream_dim: Color,
    /// Neutro quente (cinza-café, #9C8B79). Hue da seção estrutural
    /// "Sistema". Único valor de cor novo do 2b.1 — não existia primitivo
    /// neutro quente; mora aqui, na fonte única, nunca no libluma.
    pub warm_neutral: Color,
    /// Chrome de janela (#1B1714) — fundo da barra de título custom,
    /// um tom só abaixo do canvas. Token novo do M2 (moldura Golden
    /// Hour); mora aqui, na fonte única, nunca hardcoded no libluma.
    pub chrome: Color,
}

impl Palette {
    /// Paleta default Golden Hour. Os hex literais moram SÓ aqui.
    pub const GOLDEN_HOUR: Palette = Palette {
        coffee: Color::rgb8(0x1F, 0x16, 0x11),
        coffee_deep: Color::rgb8_a(0x0F, 0x0A, 0x07, 0.97),
        coffee_surface: Color::rgb8(0x2A, 0x1F, 0x1A),
        coral: Color::rgb8(0xD9, 0x77, 0x57),
        honey: Color::rgb8(0xF5, 0xD5, 0xA8),
        sage: Color::rgb8(0x6B, 0x8E, 0x7F),
        cream: Color::rgb8(0xFB, 0xF6, 0xEE),
        amber: Color::rgb8(0xE8, 0x9B, 0x3D),
        cream_dim: Color::rgb8(0xA8, 0x9D, 0x92),
        warm_neutral: Color::rgb8(0x9C, 0x8B, 0x79),
        chrome: Color::rgb8(0x1B, 0x17, 0x14),
    };

    /// Aplica overrides hex sobre a paleta default. Hex inválido cai
    /// no default (mesma regra do `luma-theme`).
    pub fn resolve(ov: &PaletteOverrides) -> Palette {
        let d = Palette::GOLDEN_HOUR;
        Palette {
            coffee: over(ov.coffee, d.coffee),
            // coffee_deep não é resolvido por override aqui (o alpha
            // override mora no luma-theme); apps usam o default.
            coffee_deep: d.coffee_deep,
            coffee_surface: over(ov.coffee_surface, d.coffee_surface),
            coral: over(ov.coral, d.coral),
            honey: over(ov.honey, d.honey),
            sage: over(ov.sage, d.sage),
            cream: over(ov.cream, d.cream),
            amber: over(ov.amber, d.amber),
            cream_dim: over(ov.cream_dim, d.cream_dim),
            warm_neutral: over(ov.warm_neutral, d.warm_neutral),
            chrome: over(ov.chrome, d.chrome),
        }
    }

    /// Lê `~/.config/luma/luma.toml` (bloco `[colors]`), resolve os
    /// overrides e devolve a paleta. Ausência/erro = Golden Hour puro.
    /// Mesmo arquivo e mesmas chaves que o `luma-theme` consome.
    pub fn load() -> Palette {
        let raw = config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str::<RawFile>(&s).ok())
            .unwrap_or_default();
        let c = raw.colors.unwrap_or_default();
        Palette::resolve(&PaletteOverrides {
            coffee: c.window_bg_dark.as_deref(),
            coffee_surface: c.applet_bg.as_deref(),
            coral: c.hover_close.as_deref(),
            honey: c.hover_minimize.as_deref(),
            sage: c.hover_maximize.as_deref(),
            cream: c.text_primary.as_deref(),
            amber: c.accent_amber.as_deref(),
            cream_dim: c.text_secondary.as_deref(),
            warm_neutral: c.warm_neutral.as_deref(),
            chrome: c.chrome.as_deref(),
        })
    }
}

fn over(s: Option<&str>, default: Color) -> Color {
    s.and_then(parse_hex).unwrap_or(default)
}

/// Overrides hex opcionais por cor nomeada (vêm do `[colors]` do
/// `luma.toml`). `None` = usa o default Golden Hour.
#[derive(Debug, Default, Clone, Copy)]
pub struct PaletteOverrides<'a> {
    pub coffee: Option<&'a str>,
    pub coffee_surface: Option<&'a str>,
    pub coral: Option<&'a str>,
    pub honey: Option<&'a str>,
    pub sage: Option<&'a str>,
    pub cream: Option<&'a str>,
    pub amber: Option<&'a str>,
    pub cream_dim: Option<&'a str>,
    pub warm_neutral: Option<&'a str>,
    pub chrome: Option<&'a str>,
}

// ─── Semantic ─────────────────────────────────────────────────────────

/// Camada semântica fina pros apps. Mapeada sobre a [`Palette`] —
/// nomes genéricos de design system, valores vindos da paleta.
#[derive(Debug, Clone, Copy)]
pub struct Semantic {
    /// Destaque primário ← `amber`.
    pub accent: Color,
    /// Fundo principal ← `coffee`.
    pub background: Color,
    /// Superfície de cards/tiles sobre o fundo — `coffee` clareado pra
    /// ~1.5:1 de contraste com o `background`, pro card flutuar.
    pub surface: Color,
    /// Texto primário ← `cream`.
    pub text: Color,
    /// Texto secundário ← `cream_dim`.
    pub text_secondary: Color,
}

impl Semantic {
    /// Deriva os roles semânticos de uma paleta resolvida.
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            accent: p.amber,
            background: p.coffee,
            // surface = coffee clareado (~1.5:1 vs background) — elevação
            // pro card flutuar. Derivado, não hex novo. (coffee_surface
            // segue sendo o applet_bg do compositor, role separado.)
            surface: p.coffee.lighten(0.14),
            text: p.cream,
            text_secondary: p.cream_dim,
        }
    }

    /// Atalho: paleta resolvida do `luma.toml` → semântica.
    pub fn load() -> Self {
        Self::from_palette(&Palette::load())
    }
}

// ─── luma.toml (bloco [colors]) ───────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct RawFile {
    colors: Option<RawColors>,
}

/// Só as chaves de cor que esta crate possui. As demais chaves do
/// `[colors]` (e o resto do `luma.toml`) seguem sendo responsabilidade
/// do `luma-theme`; serde ignora campos desconhecidos.
#[derive(Debug, Default, Deserialize)]
struct RawColors {
    window_bg_dark: Option<String>,
    applet_bg: Option<String>,
    accent_amber: Option<String>,
    hover_minimize: Option<String>,
    hover_maximize: Option<String>,
    hover_close: Option<String>,
    text_primary: Option<String>,
    text_secondary: Option<String>,
    warm_neutral: Option<String>,
    chrome: Option<String>,
}

/// `$XDG_CONFIG_HOME/luma/luma.toml` ou `~/.config/luma/luma.toml`.
/// Mesmo caminho que o `luma-theme` usa.
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("luma").join("luma.toml"))
}
