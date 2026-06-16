//! WIP: esta crate carrega state e lógica da personalização do Luma.
//!
//! O render foi removido do compositor (overlay no winit.rs revertido).
//! O destino final é ser usada pelo app nativo de configuração
//! (cliente Wayland em libcosmic, IPC D-Bus), ainda não construído.
//!
//! Até o app nativo nascer, a API pública desta crate
//! (`open_customize`, `close_customize`, `is_customize_open`)
//! retorna no-op ou stub. Não chama essa API de lugar nenhum
//! por enquanto.

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
    Free,
}

/// State do diálogo de personalização (WIP — usado pelo app nativo futuro).
#[derive(Debug)]
pub struct CustomizeDialog {
    pub open: bool,
    pub active_tab: Tab,
    pub target: WidgetTarget,
    pub sidebar_hover: Option<usize>,
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
        self.open = true;
        self.active_tab = tab;
        self.target = target;
    }

    pub fn close(&mut self) {
        self.open = false;
    }
}

// Stubs — no-op até o app nativo existir.

pub fn open_customize(_initial_tab: Tab, _context: WidgetTarget) {
    tracing::warn!("open_customize chamado, mas app nativo ainda não existe. No-op.");
}

pub fn close_customize() {}

pub fn is_customize_open() -> bool {
    false
}
