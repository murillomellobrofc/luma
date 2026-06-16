//! Modal de confirmação pras ações destrutivas. H32.10.fix.modal.
//!
//! Logout/Restart/Power exigem 2-step: 1º click abre modal centralizado
//! com "Cancelar" + "Confirmar". 2º click no Confirmar executa.
//! Esc cancela.
//!
//! Lock/Sleep não usam modal — efeito reversível.

use smithay::{
    backend::renderer::{element::memory::MemoryRenderBuffer, Color32F},
    utils::{Logical, Point, Size},
};

type Label = (MemoryRenderBuffer, Size<i32, Logical>);

/// Dimensões fixas (não vai pro theme nesse momento, é confirmation
/// modal simples).
pub const MODAL_WIDTH: i32 = 420;
pub const MODAL_HEIGHT: i32 = 180;
pub const BTN_WIDTH: i32 = 140;
pub const BTN_HEIGHT: i32 = 44;
pub const BTN_GAP: i32 = 16;

#[derive(Debug)]
pub struct ConfirmModal {
    /// Backdrop semi-transparente cobrindo o output (escurece atrás).
    pub backdrop: smithay::backend::renderer::element::solid::SolidColorBuffer,
    /// Caixa central Coffee dark corner 12.
    pub box_bg: MemoryRenderBuffer,
    /// Botão Cancelar (Coffee+).
    pub cancel_bg: MemoryRenderBuffer,
    /// Botão Confirmar (Coral pra Power, Amber pra Restart/Logout).
    pub confirm_bg_coral: MemoryRenderBuffer,
    pub confirm_bg_amber: MemoryRenderBuffer,
    /// Labels rasterizados.
    pub cancel_label: Option<Label>,
    pub confirm_label: Option<Label>,
    /// Título dinâmico ("Desligar agora?" / "Reiniciar?" / "Encerrar sessão?").
    pub title_label: Option<Label>,
    title_for: Option<crate::menu::state::SessionButton>,
}

impl ConfirmModal {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let box_color = Color32F::new(0.12, 0.09, 0.07, 0.98);
        let coffee_plus = theme.colors.row_bg_hover;
        let coral = Color32F::new(0xD9 as f32 / 255.0, 0x77 as f32 / 255.0, 0x57 as f32 / 255.0, 1.0);
        let amber = theme.colors.accent_amber;

        Self {
            backdrop: smithay::backend::renderer::element::solid::SolidColorBuffer::new(
                (1, 1),
                Color32F::new(0.0, 0.0, 0.0, 0.55),
            ),
            box_bg: crate::gfx::make_rounded_panel(MODAL_WIDTH, MODAL_HEIGHT, 12, box_color),
            cancel_bg: crate::gfx::make_rounded_panel(BTN_WIDTH, BTN_HEIGHT, 8, coffee_plus),
            confirm_bg_coral: crate::gfx::make_rounded_panel(BTN_WIDTH, BTN_HEIGHT, 8, coral),
            confirm_bg_amber: crate::gfx::make_rounded_panel(BTN_WIDTH, BTN_HEIGHT, 8, amber),
            cancel_label: None,
            confirm_label: None,
            title_label: None,
            title_for: None,
        }
    }

    pub fn ensure_labels(
        &mut self,
        text: &crate::text::TextRenderer,
        action: crate::menu::state::SessionButton,
    ) {
        let theme = crate::theme::current();
        if self.cancel_label.is_none() {
            self.cancel_label = rasterize_colored(
                text, "Cancelar", 14.0, theme.colors.text_primary,
            );
        }
        if self.confirm_label.is_none() {
            self.confirm_label = rasterize_colored(
                text, "Confirmar", 14.0, theme.colors.text_primary,
            );
        }
        if self.title_for != Some(action) {
            let title = title_for(action);
            self.title_label = rasterize_colored(
                text, title, 18.0, theme.colors.text_primary,
            );
            self.title_for = Some(action);
        }
    }
}

impl Default for ConfirmModal {
    fn default() -> Self {
        Self::new()
    }
}

pub fn title_for(action: crate::menu::state::SessionButton) -> &'static str {
    use crate::menu::state::SessionButton::*;
    match action {
        Logout => "Encerrar sessão?",
        Restart => "Reiniciar o sistema?",
        Power => "Desligar o sistema?",
        Lock => "Bloquear?",   // unused (Lock não usa modal)
        Sleep => "Suspender?", // unused (Sleep não usa modal)
    }
}

/// Retorna `true` se a ação requer confirmação (Logout/Restart/Power).
pub fn requires_confirmation(action: crate::menu::state::SessionButton) -> bool {
    use crate::menu::state::SessionButton::*;
    matches!(action, Logout | Restart | Power)
}

/// Origem (top-left logical) da modal central dado o output.
pub fn modal_origin(output_size: Size<i32, Logical>) -> Point<i32, Logical> {
    Point::from((
        (output_size.w - MODAL_WIDTH) / 2,
        (output_size.h - MODAL_HEIGHT) / 2,
    ))
}

pub fn cancel_button_origin(modal_origin: Point<i32, Logical>) -> Point<i32, Logical> {
    let y = modal_origin.y + MODAL_HEIGHT - 24 - BTN_HEIGHT;
    let x = modal_origin.x + (MODAL_WIDTH - 2 * BTN_WIDTH - BTN_GAP) / 2;
    Point::from((x, y))
}

pub fn confirm_button_origin(modal_origin: Point<i32, Logical>) -> Point<i32, Logical> {
    let cancel = cancel_button_origin(modal_origin);
    Point::from((cancel.x + BTN_WIDTH + BTN_GAP, cancel.y))
}

/// Hit-test botões do modal: 0=cancel, 1=confirm.
pub fn hit_test_modal(
    point: smithay::utils::Point<f64, Logical>,
    output_size: Size<i32, Logical>,
) -> Option<u8> {
    let origin = modal_origin(output_size);
    let in_btn = |pt: Point<i32, Logical>| {
        let x = pt.x as f64;
        let y = pt.y as f64;
        let w = BTN_WIDTH as f64;
        let h = BTN_HEIGHT as f64;
        point.x >= x && point.x < x + w && point.y >= y && point.y < y + h
    };
    if in_btn(cancel_button_origin(origin)) {
        Some(0)
    } else if in_btn(confirm_button_origin(origin)) {
        Some(1)
    } else {
        None
    }
}

fn rasterize_colored(
    text: &crate::text::TextRenderer,
    s: &str,
    size: f32,
    color: Color32F,
) -> Option<Label> {
    let (pixels, w, h) = text.rasterize_colored(s, size, color);
    if w == 0 || h == 0 {
        return None;
    }
    let buffer = MemoryRenderBuffer::from_slice(
        &pixels,
        smithay::backend::allocator::Fourcc::Argb8888,
        (w as i32, h as i32),
        1,
        smithay::utils::Transform::Normal,
        None,
    );
    Some((buffer, Size::from((w as i32, h as i32))))
}
