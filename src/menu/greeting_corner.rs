//! Greeting corner do menu fullscreen híbrido. H32.10.6.
//!
//! Canto sup-direito do output: avatar 44×44 circle gradient
//! Honey→Rust + "M" Fraunces 20px no centro, nome "Murillo"
//! Fraunces 16px à direita do avatar, saudação dinâmica
//! "Bom dia/tarde/noite" embaixo do nome em Italic 13px cinza.

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::Size,
};

type Label = (MemoryRenderBuffer, Size<i32, smithay::utils::Logical>);

/// Buffers do greeting corner. Avatar é rasterizado 1x. Os 3 labels
/// (inicial, nome, saudação) podem ser rasterizados sob demanda
/// quando o user/hostname/hora muda.
#[derive(Debug)]
pub struct GreetingCorner {
    /// Círculo 44×44 com gradient Honey→Rust (= logoBeam spec).
    pub avatar: MemoryRenderBuffer,
    /// Inicial "M" rasterizada Cream pra centralizar no avatar.
    pub avatar_letter: Option<Label>,
    /// Nome "Murillo" rasterizado Cream Fraunces 16px.
    pub username: Option<Label>,
    /// Saudação dinâmica ("Bom dia"/"Boa tarde"/"Boa noite") em cinza
    /// italic 13px. Recalcula quando a hora cruza limites de período.
    pub greeting: Option<Label>,
    /// Hora atual cached pra evitar rerasterizar a cada frame.
    last_hour: Option<u32>,
}

impl GreetingCorner {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let g = &theme.menu_greeting;
        let avatar = crate::gfx::make_avatar_circle(
            g.avatar_size,
            g.avatar_gradient_top,
            g.avatar_gradient_bottom,
        );
        Self {
            avatar,
            avatar_letter: None,
            username: None,
            greeting: None,
            last_hour: None,
        }
    }

    /// Rasteriza inicial+nome 1x (lazy). Pode ser chamada todo frame —
    /// só faz trabalho se ainda não tiver feito.
    pub fn ensure_labels(
        &mut self,
        text: &crate::text::TextRenderer,
        user: &str,
    ) {
        let theme = crate::theme::current();
        let g = &theme.menu_greeting;
        let display_user = capitalize_first(user);

        if self.avatar_letter.is_none() {
            if let Some(letter) = display_user.chars().next() {
                self.avatar_letter = rasterize_colored(
                    text,
                    &letter.to_uppercase().to_string(),
                    g.avatar_letter_size,
                    theme.colors.text_primary, // Cream
                );
            }
        }
        if self.username.is_none() {
            self.username = rasterize_colored(
                text,
                &display_user,
                g.username_size,
                g.username_color,
            );
        }
    }

    /// Atualiza saudação se a hora mudou. Devolve true se rerasterizou.
    pub fn update_greeting(
        &mut self,
        text: &crate::text::TextRenderer,
        now_hour: u32,
    ) -> bool {
        if self.last_hour == Some(now_hour) && self.greeting.is_some() {
            return false;
        }
        let theme = crate::theme::current();
        let g = &theme.menu_greeting;
        let phrase = match now_hour {
            5..=11 => &g.morning_text,
            12..=17 => &g.afternoon_text,
            _ => &g.evening_text,
        };
        self.greeting = rasterize_colored(
            text,
            phrase,
            g.greeting_size,
            g.greeting_color,
        );
        self.last_hour = Some(now_hour);
        true
    }
}

impl Default for GreetingCorner {
    fn default() -> Self {
        Self::new()
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn rasterize_colored(
    text: &crate::text::TextRenderer,
    s: &str,
    size: f32,
    color: smithay::backend::renderer::Color32F,
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
