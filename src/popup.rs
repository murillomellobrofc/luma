//! Popups de applets do system tray. H44.volume.1.
//!
//! Sistema de popup INTERNO do compositor (não wayland surface) que
//! abre ancorado a um ícone do tray e fecha em Esc / click fora.
//! Renderizado direto pelo Luma — sem usar xdg_popup do cliente.

use smithay::utils::{Logical, Point, Size};

/// Tipos de applet popup. Cada um tem conteúdo próprio (devices/apps/etc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppletKind {
    Volume,
}

/// Aba ativa no popup de volume. H44.volume.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeTab {
    Devices,
    Applications,
}

impl Default for VolumeTab {
    fn default() -> Self {
        VolumeTab::Devices
    }
}

/// Estado visual + posicional de um applet popup ativo.
#[derive(Debug, Clone)]
pub struct AppletPopup {
    pub kind: AppletKind,
    /// Coordenada x do CENTRO do ícone do tray onde a seta aponta.
    pub anchor_x: i32,
    /// Tamanho lógico do popup (w, h).
    pub size: Size<i32, Logical>,
    /// Aba ativa (válido pra Volume). H44.volume.2.
    pub volume_tab: VolumeTab,
}

impl AppletPopup {
    /// Cria um popup pro applet `kind`, ancorado em `anchor_x`. Usa
    /// tamanho default vindo do tema (futuro: por kind).
    pub fn new(kind: AppletKind, anchor_x: i32) -> Self {
        let size = match kind {
            AppletKind::Volume => Size::from((360, 420)),
        };
        Self {
            kind,
            anchor_x,
            size,
            volume_tab: VolumeTab::default(),
        }
    }

    /// Retângulos das 2 tabs (Devices/Apps) no topo do popup, em
    /// coords lógicas do output. Tab header tem `tab_h` altura.
    /// H44.volume.2.
    pub fn tab_rects(
        &self,
        output_size: Size<i32, Logical>,
    ) -> (
        smithay::utils::Rectangle<i32, Logical>,
        smithay::utils::Rectangle<i32, Logical>,
    ) {
        use smithay::utils::Rectangle;
        let o = self.origin(output_size);
        let tab_h = 40;
        let half = self.size.w / 2;
        let devices = Rectangle::from_loc_and_size((o.x, o.y), (half, tab_h));
        let apps = Rectangle::from_loc_and_size((o.x + half, o.y), (self.size.w - half, tab_h));
        (devices, apps)
    }

    /// Hit-test nas 2 abas. Retorna a aba clicada se o ponto cai em
    /// alguma. H44.volume.2.
    pub fn tab_hit(
        &self,
        output_size: Size<i32, Logical>,
        point: Point<f64, Logical>,
    ) -> Option<VolumeTab> {
        let (d, a) = self.tab_rects(output_size);
        if d.to_f64().contains(point) {
            Some(VolumeTab::Devices)
        } else if a.to_f64().contains(point) {
            Some(VolumeTab::Applications)
        } else {
            None
        }
    }

    /// Calcula o canto superior-esquerdo do popup em coords lógicas
    /// do output. Centraliza horizontalmente sobre o anchor; topo do
    /// popup fica `margin` acima da taskbar (= acima do ícone).
    pub fn origin(&self, output_size: Size<i32, Logical>) -> Point<i32, Logical> {
        let margin = 8;
        let taskbar_h = crate::taskbar::taskbar_height();
        let y = output_size.h - taskbar_h - self.size.h - margin;
        let x = (self.anchor_x - self.size.w / 2)
            .max(8)
            .min(output_size.w - self.size.w - 8);
        Point::from((x, y))
    }

    /// True se o ponto está DENTRO do retângulo do popup. Usado pra
    /// decidir se um click fecha (fora) ou interage (dentro).
    pub fn contains(&self, output_size: Size<i32, Logical>, point: Point<f64, Logical>) -> bool {
        let o = self.origin(output_size);
        let px = point.x as i32;
        let py = point.y as i32;
        px >= o.x && px < o.x + self.size.w && py >= o.y && py < o.y + self.size.h
    }
}
