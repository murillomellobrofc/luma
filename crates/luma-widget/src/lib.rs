//! Trait `Widget` + `RenderCtx` — fundação do sistema de widgets do
//! Luma. Crate intencionalmente mínimo: implementações concretas
//! (LabelWidget, ClockWidget, etc) vivem no crate que consome.
//!
//! Filosofia: o widget é puro buffer-rendering. Recebe um contexto
//! com a altura disponível e o tema, devolve um `MemoryRenderBuffer`
//! que o compositor compõe na tela. Sem state interno mutável aqui —
//! widgets stateful resolvem isso depois (fatia 3+).

use luma_theme::Theme;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;

/// Tudo que um widget recebe pra se desenhar. Struct (não tuple) de
/// propósito: dá pra adicionar campo depois sem mexer na assinatura
/// da trait, mantendo compat com widgets já existentes.
pub struct RenderCtx<'a> {
    /// Altura disponível em pixels lógicos. O widget desenha um
    /// buffer dessa altura — a largura é decisão do widget.
    pub panel_height: u32,
    /// Tema atual (cores Golden Hour, tamanhos de fonte, etc).
    pub theme: &'a Theme,
}

/// Um widget do Luma. Sabe se desenhar num buffer.
///
/// Stateless por enquanto — a fatia 3 adiciona ciclo de vida
/// (state, tick, repaint). Pra POC da fatia 1, render puro basta.
pub trait Widget {
    fn render(&self, ctx: &RenderCtx) -> MemoryRenderBuffer;
}
