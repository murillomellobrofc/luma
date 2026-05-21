//! Popups de applets do system tray. H44.volume.1.
//!
//! Sistema de popup INTERNO do compositor (não wayland surface) que
//! abre ancorado a um ícone do tray e fecha em Esc / click fora.
//! Renderizado direto pelo Luma — sem usar xdg_popup do cliente.

use smithay::utils::{Logical, Point, Rectangle, Size};

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

/// Região de um device card no popup. H44.volume.7-8.
#[derive(Debug, Clone, Copy)]
pub enum DeviceRegion {
    /// Ícone de mute (esquerda do nome).
    MuteIcon,
    /// Label do nome (toggle set-default).
    Label,
    /// Slider — x_pct é 0..1 (clamped).
    Slider(f32),
}

/// Resultado do hit-test de devices: idx no Vec + região clicada.
#[derive(Debug, Clone, Copy)]
pub struct DeviceHit {
    pub idx: usize,
    pub region: DeviceRegion,
}

/// Hit-test nos cards de device (Sinks ou Sources). Replica o layout
/// do render no winit. `entry_h` deve casar com o render. H44.volume.7.
pub fn hit_test_devices(
    popup: &AppletPopup,
    output_size: Size<i32, Logical>,
    n: usize,
    point: Point<f64, Logical>,
) -> Option<DeviceHit> {
    let theme = crate::theme::current();
    let t_popup = &theme.popup;
    let t_slider = &t_popup.slider;
    let o = popup.origin(output_size);
    let entry_h = 64;
    let track_w = popup.size.w - t_popup.padding * 2;
    let track_h = t_slider.track_height;
    let handle_r = t_slider.handle_radius;
    let icon_w = 24;
    let mut y_cursor = o.y + t_popup.tab_height + t_popup.padding;
    for idx in 0..n {
        if y_cursor + entry_h > o.y + popup.size.h - t_popup.padding {
            break;
        }
        let row_top = y_cursor;
        let row_bot = y_cursor + entry_h;
        if (point.y as i32) >= row_top && (point.y as i32) < row_bot {
            // Decide região por x.
            // Mute icon: esquerda (24px wide).
            let mute_left = o.x + t_popup.padding;
            let mute_right = mute_left + icon_w;
            // Label: meio.
            let label_top = row_top;
            let label_bot = row_top + 20;
            // Slider: y entre track_top-handle_r e track_bot+handle_r.
            let track_y = row_top + 22;
            let slider_top = track_y - handle_r;
            let slider_bot = track_y + track_h + handle_r;
            let track_x = o.x + t_popup.padding;
            if (point.y as i32) >= slider_top && (point.y as i32) <= slider_bot {
                let px = point.x as f64;
                let tx = track_x as f64;
                if px >= tx && px <= tx + track_w as f64 {
                    let v = ((px - tx) / track_w as f64).clamp(0.0, 1.0) as f32;
                    return Some(DeviceHit {
                        idx,
                        region: DeviceRegion::Slider(v),
                    });
                }
            }
            if (point.y as i32) >= label_top && (point.y as i32) < label_bot {
                if (point.x as i32) >= mute_left && (point.x as i32) < mute_right {
                    return Some(DeviceHit {
                        idx,
                        region: DeviceRegion::MuteIcon,
                    });
                }
                return Some(DeviceHit {
                    idx,
                    region: DeviceRegion::Label,
                });
            }
            return None;
        }
        y_cursor += entry_h;
    }
    None
}

/// Slider horizontal reutilizável. H44.volume.3.
///
/// Trilho cinza no fundo, fill âmbar do início até o valor, bolinha
/// (handle) honey no x do valor. `value` é 0.0..max (max default 1.0;
/// boost de 100%+ ajusta max em A.E).
#[derive(Debug, Clone, Copy)]
pub struct Slider {
    pub bounds: Rectangle<i32, Logical>,
    pub value: f32,
    pub max: f32,
    pub dragging: bool,
}

impl Slider {
    pub fn new(bounds: Rectangle<i32, Logical>, value: f32) -> Self {
        Self {
            bounds,
            value: value.clamp(0.0, 1.0),
            max: 1.0,
            dragging: false,
        }
    }

    /// True se `point` está dentro do retângulo do slider (hit area).
    pub fn hit_test(&self, point: Point<f64, Logical>) -> bool {
        self.bounds.to_f64().contains(point)
    }

    /// Converte coordenada x (clamped pros limites) em value [0, max].
    pub fn value_from_x(&self, x: f64) -> f32 {
        let dx = (x - self.bounds.loc.x as f64) / self.bounds.size.w as f64;
        (dx.clamp(0.0, 1.0) as f32) * self.max
    }
}
