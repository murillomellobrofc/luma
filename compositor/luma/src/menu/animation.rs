//! Animação fade-in/out do menu fullscreen. H32.10.5.
//!
//! Acompanha `state.menu_open` (intent do usuário) e gera um alpha
//! 0.0 → 1.0 que o backend multiplica no render. Quando o usuário
//! fecha, o alpha decai de volta a 0.0 sem cortar a transição.
//!
//! Respeita `LUMA_REDUCE_MOTION=1` (env var) — alpha vira binário
//! 0/1 sem interpolar. Quando migrarmos pro DRM backend, troca por
//! polling da setting do GNOME/KDE de prefers-reduced-motion.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// alpha indo de 0 → 1 (abrindo).
    In,
    /// alpha indo de 1 → 0 (fechando).
    Out,
}

#[derive(Debug)]
pub struct MenuAnimation {
    /// Alpha atual (0.0 invisível, 1.0 totalmente visível).
    alpha: f32,
    /// Pra onde estamos indo (In = abrindo, Out = fechando).
    direction: Direction,
    /// Último tick — usado pra calcular delta no `tick`.
    last_tick: Instant,
    /// Última leitura de `menu_open` — pra detectar transições.
    last_intent_open: bool,
    /// Cache do flag de reduce-motion (lido 1x no init pra evitar
    /// chamada de env var por frame).
    reduce_motion: bool,
}

impl MenuAnimation {
    pub fn new() -> Self {
        Self {
            alpha: 0.0,
            direction: Direction::Out,
            last_tick: Instant::now(),
            last_intent_open: false,
            reduce_motion: std::env::var("LUMA_REDUCE_MOTION")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }

    /// Atualiza o alpha baseado em quanto tempo passou desde o
    /// último tick e a direção atual. Deve ser chamado uma vez por
    /// frame antes de ler `alpha()`. `menu_open` é o intent atual
    /// do usuário — se mudou desde o último tick, flipa a direção.
    pub fn tick(&mut self, menu_open: bool) {
        let theme = crate::theme::current();
        let f = &theme.menu_fullscreen;

        // Detecta toggle: usuário abriu ou fechou.
        if menu_open != self.last_intent_open {
            self.direction = if menu_open { Direction::In } else { Direction::Out };
            self.last_intent_open = menu_open;
            self.last_tick = Instant::now();
        }

        // Reduce-motion: pula interpolação.
        if self.reduce_motion {
            self.alpha = if menu_open { 1.0 } else { 0.0 };
            return;
        }

        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_tick).as_millis() as f32;
        self.last_tick = now;

        let duration = match self.direction {
            Direction::In => f.fade_in_ms as f32,
            Direction::Out => f.fade_out_ms as f32,
        }
        .max(1.0);

        // Velocidade linear de 1.0 por `duration` ms. Easing aplicado
        // no `alpha()` em vez de aqui, pra não acumular erro.
        let delta = dt_ms / duration;
        match self.direction {
            Direction::In => self.alpha = (self.alpha + delta).min(1.0),
            Direction::Out => self.alpha = (self.alpha - delta).max(0.0),
        }
    }

    /// Alpha visual atual, já com easing ease-out aplicado. Range 0.0-1.0.
    pub fn alpha(&self) -> f32 {
        // ease-out cubic: f(t) = 1 - (1 - t)^3
        let t = self.alpha.clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }

    /// Se a camada do menu deve ser desenhada neste frame. Continua
    /// retornando true durante o fade-out pra não cortar o final.
    pub fn is_visible(&self) -> bool {
        self.alpha > 0.001 || self.last_intent_open
    }
}

impl Default for MenuAnimation {
    fn default() -> Self {
        Self::new()
    }
}
