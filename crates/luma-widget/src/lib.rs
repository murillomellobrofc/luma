//! Trait `Widget` + `RenderCtx` — fundação do sistema de widgets do
//! Luma. Crate intencionalmente mínimo: implementações concretas
//! (LabelWidget, ClockWidget, etc) vivem no crate que consome.
//!
//! Filosofia: o widget é puro buffer-rendering. Recebe um contexto
//! com a altura disponível e o tema, devolve um `MemoryRenderBuffer`
//! que o compositor compõe na tela. State mutável fica DENTRO do
//! widget — a luma-bar chama `tick()` quando o intervalo vence e
//! `render()` lê o estado atual.

use std::time::Duration;

use luma_theme::Theme;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;

/// Onde o widget está hospedado. Cada host impõe restrições
/// diferentes — o painel dita a altura, o desktop deixa o widget
/// escolher o próprio tamanho.
///
/// Fatia 4: host-aware substitui `panel_height` solto, pra preparar
/// pra widgets no desktop sem mexer na trait.
pub enum WidgetHost {
    /// Widget na fileira do painel. Recebe a altura disponível.
    Panel { height: u32 },
    /// Widget solto no desktop. Escolhe o próprio tamanho.
    Desktop,
}

/// Tudo que um widget recebe pra se desenhar. Struct (não tuple) de
/// propósito: dá pra adicionar campo depois sem mexer na assinatura
/// da trait, mantendo compat com widgets já existentes.
pub struct RenderCtx<'a> {
    /// Onde o widget está sendo renderizado (painel x desktop, etc).
    /// O widget casa com isso pra escolher dimensões.
    pub host: WidgetHost,
    /// Tema atual (cores Golden Hour, tamanhos de fonte, etc).
    pub theme: &'a Theme,
}

/// Um widget do Luma. Sabe se desenhar num buffer e (opcionalmente)
/// se atualizar com o tempo.
///
/// Modelo de ciclo de vida (fatia 3):
/// - `tick_interval()` declara de quanto em quanto tempo o widget
///   quer atualizar. None = estático (nunca tica).
/// - A luma-bar chama `tick()` quando o intervalo vence — uma vez,
///   sem catch-up de ticks perdidos. O widget atualiza o próprio
///   estado interno aqui.
/// - `render()` é chamado todo frame e lê o estado atual.
///
/// Sem cache, sem dirty tracking: a luma-bar redesenha tudo todo
/// frame (loop contínuo, ~60fps). O `tick()` só precisa marcar o
/// estado novo, o `render()` do frame seguinte materializa.
pub trait Widget {
    fn render(&self, ctx: &RenderCtx) -> MemoryRenderBuffer;

    /// De quanto em quanto tempo o widget quer atualizar.
    /// None = estático, não atualiza sozinho.
    fn tick_interval(&self) -> Option<Duration> {
        None
    }

    /// Chamado quando o intervalo do widget vence. O widget atualiza
    /// o próprio estado aqui. O render do frame seguinte reflete.
    fn tick(&mut self) {}
}
