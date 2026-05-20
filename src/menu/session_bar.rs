//! Session bar bottom do menu fullscreen. H32.10.13.
//!
//! Barra horizontal embaixo da tela com:
//! - 5 botões 44×44 esquerda (Lock, Logout, Sleep, Restart, Power)
//! - Hint de atalhos centro ("↑↓ navegar...")
//! - 4 mini-places direita (Documentos, Downloads, Imagens, Sistema)
//!
//! Background Coffee escuro alpha 0.7 com corner 12. Power é o único
//! botão colorido por default (Coral, ação perigosa sinalizada).
//!
//! Pra esse commit foco no LAYOUT: bg + 5 placeholders quadrados +
//! hint + 4 mini placeholders. Ícones SVG reais vêm em H32.10.14.

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Size},
};

type Label = (MemoryRenderBuffer, Size<i32, Logical>);

#[derive(Debug)]
pub struct SessionBar {
    /// Background da bar (coffee escuro alpha 0.7, rounded 12).
    pub bg: MemoryRenderBuffer,
    /// Placeholder de cada um dos 5 botões (44×44 cream alpha baixa).
    /// Power é diferente: Coral sólido.
    pub button_placeholder: MemoryRenderBuffer,
    pub power_placeholder: MemoryRenderBuffer,
    /// 5 ícones rasterizados dos SVGs (H32.10.14). Index segue
    /// SessionButton: 0=Lock, 1=Logout, 2=Sleep, 3=Restart, 4=Power.
    /// `None` se o arquivo SVG não existir ou falhou parse.
    pub icons: [Option<MemoryRenderBuffer>; 5],
    /// Bg hover por botão, pré-renderizados em cor própria (H32.10.fix).
    pub hover_bgs: [MemoryRenderBuffer; 5],
    /// Glow âmbar (proxy de blur) renderizado ao redor do botão hovered.
    /// Tamanho maior que o ícone pra aparecer fora do bg integrado.
    pub hover_glows: [MemoryRenderBuffer; 5],
    pub hover_glow_pad: i32,
    /// Hint "↑ ↓ navegar..." centralizado.
    pub hint_label: Option<Label>,
    /// 4 mini-places à direita: folder, download, image, drive.
    /// `None` se SVG falhou (fallback é placeholder).
    pub places: [Option<MemoryRenderBuffer>; 4],
    /// Versão âmbar dos mini-places pra hover.
    pub places_hover: [Option<MemoryRenderBuffer>; 4],
    /// Fallback (cinza alpha baixa) usado quando algum places[i] é None.
    pub place_placeholder: MemoryRenderBuffer,
    /// Tamanho lógico do bg pra recálculo no resize do output.
    bar_width: i32,
    bar_height: i32,
}

impl SessionBar {
    pub fn new() -> Self {
        let theme = crate::theme::current();
        let s = &theme.menu_session_bar;

        // Bg inicial 1×1 — resize() chamado no render conforme output.
        let bg_color = smithay::backend::renderer::Color32F::new(
            s.bg_color.r(), s.bg_color.g(), s.bg_color.b(), s.bg_alpha,
        );
        let bg = crate::gfx::make_rounded_panel(1, 1, s.corner_radius, bg_color);

        // Placeholder dos 4 botões transparent: cream alpha baixa só
        // pra dar contorno até o SVG entrar em H32.10.14.
        let placeholder_color = smithay::backend::renderer::Color32F::new(
            s.button_default_color.r(),
            s.button_default_color.g(),
            s.button_default_color.b(),
            0.15,
        );
        let button_placeholder = crate::gfx::make_rounded_panel(
            s.button_size, s.button_size, s.button_corner_radius, placeholder_color,
        );
        // Power Coral sólido.
        let power_placeholder = crate::gfx::make_rounded_panel(
            s.button_size, s.button_size, s.button_corner_radius, s.power_default_color,
        );
        // Mini-place: cream alpha baixa quadrado 18×18.
        let place_color = smithay::backend::renderer::Color32F::new(
            s.place_icon_color.r(),
            s.place_icon_color.g(),
            s.place_icon_color.b(),
            0.4,
        );
        let place_placeholder = crate::gfx::make_rounded_panel(
            s.place_icon_size, s.place_icon_size, 3, place_color,
        );

        // 5 ícones SVG (H32.10.14). Lock = inline path SVG. Os
        // outros 4 vêm de arquivos. `CARGO_MANIFEST_DIR` resolve em
        // tempo de compilação pro path absoluto do crate root.
        const ICONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/branding/icons");
        let icon_size = s.button_size as u32;
        // Lock agora é arquivo (icon-bloquear.svg, design final Honey
        // preenchido com keyhole contrastado). Inline serve só de
        // fallback se o arquivo sumir.
        let lock = crate::menu::icon_loader::load_svg_file(
            &format!("{}/icon-bloquear.svg", ICONS_DIR), icon_size,
        ).or_else(|| crate::menu::icon_loader::lock_svg_inline("#FBF6EE", icon_size));
        let icons = [
            lock,
            crate::menu::icon_loader::load_svg_file(
                &format!("{}/icon-sair.svg", ICONS_DIR), icon_size,
            ),
            crate::menu::icon_loader::load_svg_file(
                &format!("{}/icon-suspender.svg", ICONS_DIR), icon_size,
            ),
            crate::menu::icon_loader::load_svg_file(
                &format!("{}/icon-reiniciar.svg", ICONS_DIR), icon_size,
            ),
            crate::menu::icon_loader::load_svg_file(
                &format!("{}/icon-desligar.svg", ICONS_DIR), icon_size,
            ),
        ];

        let place_size = s.place_icon_size as u32;
        let place_hex = "#A89D92"; // text_secondary cinza
        let amber_hex = "#E89B3D"; // accent_amber pra hover
        let places = [
            crate::menu::icon_loader::folder_svg_inline(place_hex, place_size),
            crate::menu::icon_loader::download_svg_inline(place_hex, place_size),
            crate::menu::icon_loader::image_svg_inline(place_hex, place_size),
            crate::menu::icon_loader::drive_svg_inline(place_hex, place_size),
        ];
        let places_hover = [
            crate::menu::icon_loader::folder_svg_inline(amber_hex, place_size),
            crate::menu::icon_loader::download_svg_inline(amber_hex, place_size),
            crate::menu::icon_loader::image_svg_inline(amber_hex, place_size),
            crate::menu::icon_loader::drive_svg_inline(amber_hex, place_size),
        ];

        let hover_bgs = std::array::from_fn(|i| {
            let c = s.hover_colors[i];
            let color = smithay::backend::renderer::Color32F::new(
                c.r(), c.g(), c.b(), s.hover_alpha,
            );
            crate::gfx::make_rounded_panel(
                s.button_size, s.button_size, s.button_corner_radius, color,
            )
        });
        // Glow maior que o ícone (= visível ao redor mesmo com bg do
        // SVG cobrindo o botão). Mesma cor do hover_bgs, alpha menor.
        let hover_glow_pad = 6;
        let hover_glows = std::array::from_fn(|i| {
            let c = s.hover_colors[i];
            let color = smithay::backend::renderer::Color32F::new(
                c.r(), c.g(), c.b(), 0.6,
            );
            crate::gfx::make_rounded_panel(
                s.button_size + 2 * hover_glow_pad,
                s.button_size + 2 * hover_glow_pad,
                s.button_corner_radius + hover_glow_pad,
                color,
            )
        });

        Self {
            bg,
            button_placeholder,
            power_placeholder,
            icons,
            hover_bgs,
            hover_glows,
            hover_glow_pad,
            hint_label: None,
            places,
            places_hover,
            place_placeholder,
            bar_width: 1,
            bar_height: 1,
        }
    }

    /// Atualiza tamanho do bg pra cobrir o zone[4] (passado pelo caller).
    pub fn resize(&mut self, bar_size: Size<i32, Logical>) {
        if bar_size.w == self.bar_width && bar_size.h == self.bar_height {
            return;
        }
        let theme = crate::theme::current();
        let s = &theme.menu_session_bar;
        let bg_color = smithay::backend::renderer::Color32F::new(
            s.bg_color.r(), s.bg_color.g(), s.bg_color.b(), s.bg_alpha,
        );
        // make_rounded_panel não tem resize — recria. Não é hot path.
        self.bg = crate::gfx::make_rounded_panel(
            bar_size.w, bar_size.h, s.corner_radius, bg_color,
        );
        self.bar_width = bar_size.w;
        self.bar_height = bar_size.h;
    }

    pub fn ensure_labels(&mut self, text: &crate::text::TextRenderer) {
        let theme = crate::theme::current();
        let s = &theme.menu_session_bar;
        if self.hint_label.is_none() {
            self.hint_label = rasterize_colored(
                text, &s.hint_text, s.hint_size, s.hint_color,
            );
        }
    }
}

impl Default for SessionBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Hit-test pros 5 botões da session bar. Retorna qual SessionButton
/// foi clicado (Lock/Logout/Sleep/Restart/Power), ou None se fora.
pub fn hit_test_buttons(
    point: smithay::utils::Point<f64, Logical>,
    zone_origin: Point<i32, Logical>,
) -> Option<crate::menu::state::SessionButton> {
    use crate::menu::state::SessionButton::{Lock, Logout, Sleep, Restart, Power};
    let s = &crate::theme::current().menu_session_bar;
    let buttons = [Lock, Logout, Sleep, Restart, Power];
    for (i, origin) in button_origins(zone_origin).iter().enumerate() {
        let x = origin.x as f64;
        let y = origin.y as f64;
        let w = s.button_size as f64;
        let h = s.button_size as f64;
        if point.x >= x && point.x < x + w && point.y >= y && point.y < y + h {
            return Some(buttons[i]);
        }
    }
    None
}

/// Hit-test pros 4 mini-places à direita. Retorna o índice clicado
/// (0=Documentos, 1=Downloads, 2=Imagens, 3=Sistema/raiz).
///
/// H32.10.17 a11y: hit area expandida pra 44×44 (WCAG AAA min target)
/// centralizada no ícone 18×18. Ícone visual fica pequeno mas o alvo
/// clicável é grande.
pub fn hit_test_places(
    point: smithay::utils::Point<f64, Logical>,
    zone_origin: Point<i32, Logical>,
    zone_size: smithay::utils::Size<i32, Logical>,
) -> Option<usize> {
    let s = &crate::theme::current().menu_session_bar;
    let hit_size = crate::theme::current().accessibility.hit_target_min as f64;
    let pad = (hit_size - s.place_icon_size as f64) / 2.0;
    for (i, origin) in place_origins(zone_origin, zone_size).iter().enumerate() {
        let x = origin.x as f64 - pad;
        let y = origin.y as f64 - pad;
        if point.x >= x && point.x < x + hit_size && point.y >= y && point.y < y + hit_size {
            return Some(i);
        }
    }
    None
}

/// Abre via xdg-open o path correspondente ao índice de places.
pub fn execute_place_action(idx: usize) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let path = match idx {
        0 => format!("{home}/Documentos"),
        1 => format!("{home}/Downloads"),
        2 => format!("{home}/Imagens"),
        3 => String::from("/"),
        _ => return,
    };
    tracing::info!("place: xdg-open {path}");
    let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
}

/// Executa a ação real do botão de sessão via systemctl/loginctl.
/// Sem confirmação modal nesse commit (TODO H32.10.14.fix).
pub fn execute_action(button: crate::menu::state::SessionButton) {
    use crate::menu::state::SessionButton::*;
    let (program, args): (&str, &[&str]) = match button {
        Lock => ("loginctl", &["lock-session"]),
        Logout => ("loginctl", &["terminate-user", ""]), // empty user = caller
        Sleep => ("systemctl", &["suspend"]),
        Restart => ("systemctl", &["reboot"]),
        Power => ("systemctl", &["poweroff"]),
    };
    tracing::info!("session action: {} {:?}", program, args);
    let _ = std::process::Command::new(program).args(args).spawn();
}

/// Layout dos 5 botões à esquerda da session bar. Retorna o
/// top-left de cada um relativo ao zone_origin.
pub fn button_origins(zone_origin: Point<i32, Logical>) -> Vec<Point<i32, Logical>> {
    let s = &crate::theme::current().menu_session_bar;
    let zone_h = crate::menu::render::layout::SESSION_H;
    let y = zone_origin.y + (zone_h - s.button_size) / 2;
    let start_x = zone_origin.x + s.padding_horizontal;
    (0..5)
        .map(|i| Point::from((start_x + i * (s.button_size + s.button_gap), y)))
        .collect()
}

/// Layout dos 4 mini-places à direita da session bar.
pub fn place_origins(
    zone_origin: Point<i32, Logical>,
    zone_size: Size<i32, Logical>,
) -> Vec<Point<i32, Logical>> {
    let s = &crate::theme::current().menu_session_bar;
    let total_w = 4 * s.place_icon_size + 3 * s.place_gap;
    let start_x = zone_origin.x + zone_size.w - s.padding_horizontal - total_w;
    let y = zone_origin.y + (zone_size.h - s.place_icon_size) / 2;
    (0..4)
        .map(|i| Point::from((start_x + i * (s.place_icon_size + s.place_gap), y)))
        .collect()
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
