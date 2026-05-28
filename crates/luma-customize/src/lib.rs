//! Diálogo de personalização do Luma — overlay custom_element.
//!
//! Renderizado como overlay fixo 720×520 centralizado. Sem
//! redimensionar nem mover nesta fase (vira janela real quando o
//! framework de janelas maturar). State vive aqui, render no
//! winit.rs do crate principal.

/// Aba ativa na sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Widgets,
    Wallpaper,
    Preset,
}

/// Destino de widgets adicionados pelo picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetTarget {
    Panel,
    Desktop,
    /// Deixa o usuário escolher via mini-modal.
    Free,
}

/// State do diálogo de personalização.
#[derive(Debug)]
pub struct CustomizeDialog {
    pub open: bool,
    pub active_tab: Tab,
    pub target: WidgetTarget,
    /// Item da sidebar sob hover (0=Widgets, 1=Wallpaper, 2=Preset).
    pub sidebar_hover: Option<usize>,
    /// Hover do botão X do header.
    pub close_hover: bool,
}

impl CustomizeDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            active_tab: Tab::Widgets,
            target: WidgetTarget::Free,
            sidebar_hover: None,
            close_hover: false,
        }
    }

    pub fn open(&mut self, tab: Tab, target: WidgetTarget) {
        if self.open {
            self.active_tab = tab;
            self.target = target;
        } else {
            self.open = true;
            self.active_tab = tab;
            self.target = target;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }
}

/// Dimensões fixas do overlay.
pub const DIALOG_W: i32 = 720;
pub const DIALOG_H: i32 = 520;
pub const HEADER_H: i32 = 52;
pub const SIDEBAR_W: i32 = 160;

pub const SIDEBAR_ITEMS: &[(&str, Tab)] = &[
    ("Widgets", Tab::Widgets),
    ("Wallpaper", Tab::Wallpaper),
    ("Preset", Tab::Preset),
];
