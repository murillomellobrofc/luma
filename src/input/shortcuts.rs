use smithay::{
    backend::input::KeyState,
    input::keyboard::{Keysym, ModifiersState},
};

use crate::state::LumaState;

/// Ações disparadas por combinações de tecla do compositor.
///
/// O esquema completo de atalhos é decidido perto do final do
/// desenvolvimento (ver `luma_project` memory). Por enquanto só
/// o mínimo pra fechar janela sem depender da decoração do cliente.
#[derive(Debug, Clone, Copy)]
pub enum KeyAction {
    CloseFocused,
    /// Abre o menu launcher (Super+K). Search já recebe foco enquanto
    /// o menu está aberto, então abrir o menu equivale a "focar search".
    OpenMenuToSearch,
    /// Vai pro workspace de índice N-1 (Super+N). Auto-cria se não existe.
    /// H36.4.
    WorkspaceSwitch(usize),
    /// Move janela focada pro workspace N-1 (Super+Shift+N). Auto-cria.
    /// H36.5.
    WorkspaceMoveWindow(usize),
    /// Cicla pro próximo workspace (Super+Tab). H36.6.
    WorkspaceCycleNext,
    /// Toggle do overview screen (Super+Esc). H36.6.
    ToggleOverview,
    /// Toggle "Mostrar área de trabalho" (Super+D). H33.taskbar.F.17.
    ToggleShowDesktop,
}

/// Mapeia uma tecla pra uma ação. Retorna `None` quando não há atalho
/// ligado a essa combinação, caso em que a tecla deve ser repassada
/// pro cliente focado.
///
/// Modo aninhado: o compositor host (KDE/GNOME/etc) intercepta atalhos
/// padrão como Super+* e Alt+F4 antes deles chegarem aqui. Por isso
/// CloseFocused vive em Alt+Shift+F4 enquanto o luma roda dentro de
/// outro compositor. Em modo nativo (futuro backend drm) volta pra
/// Alt+F4 puro.
pub fn match_shortcut(
    modifiers: &ModifiersState,
    keysym: Keysym,
    key_state: KeyState,
) -> Option<KeyAction> {
    if key_state != KeyState::Pressed {
        return None;
    }
    if modifiers.alt && modifiers.shift && keysym == Keysym::F4 {
        return Some(KeyAction::CloseFocused);
    }
    // Super+K: abre o menu launcher e dá foco implícito à search bar.
    // H32.7.8. Aceita logo OU ctrl como modificador (compositor host
    // pode mexer com Super), mas o caminho canônico é Super.
    if modifiers.logo && keysym == Keysym::k {
        return Some(KeyAction::OpenMenuToSearch);
    }
    // Super+D: toggle "Mostrar área de trabalho". H33.taskbar.F.17.
    // Aceita Ctrl+Alt+D como fallback em modo aninhado (KDE/GNOME que
    // interceptam Super).
    let show_desktop_mod = modifiers.logo || (modifiers.ctrl && modifiers.alt);
    if show_desktop_mod && (keysym == Keysym::d || keysym == Keysym::D) {
        return Some(KeyAction::ToggleShowDesktop);
    }
    // H36.4-6: workspaces.
    // CANONICAL (futuro DRM): Super+N, Super+Shift+N, Super+Tab, Super+Esc.
    // FALLBACK (modo aninhado em KDE/GNOME que interceptam Super):
    //   Ctrl+Alt+N (switch), Ctrl+Alt+Shift+N (move), Ctrl+Alt+Tab (cycle),
    //   Ctrl+Alt+Esc (overview). H36.4-6.fix.
    let workspaces_mod = modifiers.logo || (modifiers.ctrl && modifiers.alt);
    if workspaces_mod {
        if keysym == Keysym::Tab {
            return Some(KeyAction::WorkspaceCycleNext);
        }
        if keysym == Keysym::Escape {
            return Some(KeyAction::ToggleOverview);
        }
        if let Some(n) = keysym_to_digit(keysym) {
            if modifiers.shift {
                return Some(KeyAction::WorkspaceMoveWindow(n as usize - 1));
            }
            return Some(KeyAction::WorkspaceSwitch(n as usize - 1));
        }
    }
    None
}

/// Mapeia Keysym de digitos 1..9 (top row) pro número correspondente.
/// Retorna None pra outros keysyms. H36.4.
fn keysym_to_digit(k: Keysym) -> Option<u32> {
    match k {
        Keysym::_1 => Some(1),
        Keysym::_2 => Some(2),
        Keysym::_3 => Some(3),
        Keysym::_4 => Some(4),
        Keysym::_5 => Some(5),
        Keysym::_6 => Some(6),
        Keysym::_7 => Some(7),
        Keysym::_8 => Some(8),
        Keysym::_9 => Some(9),
        _ => None,
    }
}

pub fn apply(state: &mut LumaState, action: KeyAction) {
    match action {
        KeyAction::CloseFocused => close_focused(state),
        KeyAction::OpenMenuToSearch => open_menu_to_search(state),
        KeyAction::WorkspaceSwitch(idx) => workspace_switch(state, idx),
        KeyAction::WorkspaceMoveWindow(idx) => workspace_move_window(state, idx),
        KeyAction::WorkspaceCycleNext => workspace_cycle_next(state),
        KeyAction::ToggleOverview => toggle_overview(state),
        KeyAction::ToggleShowDesktop => state.toggle_show_desktop(),
    }
}

/// Super+N: vai pro workspace N. Auto-cria se idx >= count. H36.4.
/// H36.fix: SEMPRE roda refresh_workspace_visibility, mesmo se
/// switch_to retornar false (caso create() já tenha setado current).
fn workspace_switch(state: &mut LumaState, idx: usize) {
    while state.workspaces.count() <= idx {
        state.workspaces.create();
    }
    state.workspaces.switch_to(idx);
    state.refresh_workspace_visibility();
    tracing::info!("workspace_switch: idx={}", idx + 1);
}

/// Super+Shift+N: move janela focada pro workspace N. H36.5.
fn workspace_move_window(state: &mut LumaState, target: usize) {
    let Some(keyboard) = state.seat.get_keyboard() else {
        return;
    };
    let Some(focus) = keyboard.current_focus() else {
        return;
    };
    state.workspaces.move_window_to_workspace(&focus, target);
    state.refresh_workspace_visibility();
    tracing::info!("Super+Shift+{}: janela movida pra workspace {}", target + 1, target + 1);
}

/// Super+Tab: cicla pro próximo workspace. H36.6.
fn workspace_cycle_next(state: &mut LumaState) {
    state.workspaces.cycle_next();
    state.refresh_workspace_visibility();
    tracing::info!("Super+Tab: workspace {}", state.workspaces.current + 1);
}

/// Super+Esc: toggle overview. Stub por enquanto — visual em Bloco D.
/// H36.6.
fn toggle_overview(state: &mut LumaState) {
    state.overview_open = !state.overview_open;
    tracing::info!("Super+Esc: overview {}", state.overview_open);
}

/// Super+K: garante que o menu está aberto (com search pronta pra
/// receber input). O menu já consome qualquer tecla pra search bar
/// enquanto aberto, então abrir = "foco na search".
fn open_menu_to_search(state: &mut LumaState) {
    if !state.menu_open {
        state.menu_open = true;
        state.menu_new.reset();
        tracing::info!("Super+K: menu aberto pra search");
    }
}

fn close_focused(state: &mut LumaState) {
    let Some(keyboard) = state.seat.get_keyboard() else {
        return;
    };
    let Some(focus) = keyboard.current_focus() else {
        return;
    };
    let Some(window) = state
        .space
        .elements()
        .filter_map(|i| i.as_window())
        .find(|w| w.window().toplevel().unwrap().wl_surface() == &focus)
        .cloned()
    else {
        return;
    };
    tracing::info!("Alt+Shift+F4: fechando janela focada");
    window.window().toplevel().unwrap().send_close();
}
