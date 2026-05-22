use smithay::{
    backend::input::{Event, InputBackend, KeyState, KeyboardKeyEvent},
    input::keyboard::{FilterResult, Keysym},
    utils::SERIAL_COUNTER,
};

use crate::{
    input::shortcuts::{apply, match_shortcut, KeyAction},
    state::LumaState,
};

pub fn handle_key<I: InputBackend>(state: &mut LumaState, event: I::KeyboardKeyEvent) {
    let serial = SERIAL_COUNTER.next_serial();
    let time = Event::time_msec(&event);
    let key_code = event.key_code();
    let key_state = event.state();

    let Some(keyboard) = state.seat.get_keyboard() else {
        return;
    };

    // Quando o menu está aberto, captura teclas pra search bar em vez
    // de mandar pro cliente. Detectado dentro do filter pra ter acesso
    // ao keysym já resolvido pelo xkb.
    // H36 Bloco D: overview fecha via Ctrl+Alt+Esc (toggle) ou click
    // num thumb / fora. Esc puro não intercepta — manter simples.
    let mut menu_action: Option<MenuKey> = None;
    let menu_open_now = state.menu_open;
    // H44.volume.1: popup aberto intercepta Esc pra fechar.
    let applet_popup_open = state.applet_popup.is_some();
    // H47a.tray.2: painel de overflow aberto intercepta Esc.
    let tray_overflow_open = state.tray.overflow_open;
    // H48.ctx.1: context menu aberto intercepta Esc.
    let context_menu_open = state.context_menu.is_some();
    // H50.2a.sel.3: Esc desseleciona ícones do desktop quando não há
    // popup/menu aberto.
    let desktop_has_selection = !state.desktop_selection.is_empty()
        && !context_menu_open
        && !menu_open_now
        && !applet_popup_open
        && !tray_overflow_open;
    let mut clear_desktop_selection = false;
    let mut close_applet_popup = false;
    let mut close_tray_overflow = false;
    let mut close_context_menu = false;
    let _ = keyboard.input::<(), _>(
        state,
        key_code,
        key_state,
        serial,
        time,
        |_state, _modifiers, handle| {
            if applet_popup_open && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    close_applet_popup = true;
                    return FilterResult::Intercept(());
                }
            }
            if tray_overflow_open && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    close_tray_overflow = true;
                    return FilterResult::Intercept(());
                }
            }
            if context_menu_open && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    close_context_menu = true;
                    return FilterResult::Intercept(());
                }
            }
            if desktop_has_selection && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    clear_desktop_selection = true;
                    return FilterResult::Intercept(());
                }
            }
            if menu_open_now && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    menu_action = Some(MenuKey::Close);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::BackSpace {
                    menu_action = Some(MenuKey::Backspace);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Return || sym == Keysym::KP_Enter {
                    menu_action = Some(MenuKey::Enter);
                    return FilterResult::Intercept(());
                }
                // H32.9.10 + H32.10.9: navegação por teclado.
                if sym == Keysym::Down {
                    menu_action = Some(MenuKey::Down);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Up {
                    menu_action = Some(MenuKey::Up);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Left {
                    menu_action = Some(MenuKey::Left);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Right {
                    menu_action = Some(MenuKey::Right);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Tab {
                    menu_action = Some(MenuKey::Tab);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Page_Up {
                    menu_action = Some(MenuKey::PageUp);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Page_Down {
                    menu_action = Some(MenuKey::PageDown);
                    return FilterResult::Intercept(());
                }
                // Caracter visível: converte keysym pra UTF32 via xkb.
                // `keysym_to_utf32` retorna 0 se a tecla não tem
                // representação textual (Shift sozinho, etc).
                let codepoint = smithay::input::keyboard::xkb::keysym_to_utf32(sym);
                if codepoint != 0 {
                    if let Some(ch) = char::from_u32(codepoint) {
                        if !ch.is_control() {
                            menu_action = Some(MenuKey::Char(ch));
                            return FilterResult::Intercept(());
                        }
                    }
                }
            }
            FilterResult::Forward
        },
    );

    if close_applet_popup {
        state.applet_popup = None;
        return;
    }

    if close_tray_overflow {
        state.tray.overflow_open = false;
        return;
    }

    if close_context_menu {
        state.context_menu = None;
        return;
    }

    if clear_desktop_selection {
        state.desktop_selection.clear();
        if let Some(l) = state.desktop_icons.as_ref() {
            l.set_selection(std::collections::HashSet::new());
        }
        return;
    }

    if let Some(action) = menu_action {
        match action {
            MenuKey::Close => {
                use crate::menu::state::{FocusedZone, MenuMode};
                // H32.10.fix.modal: Esc cancela modal antes de fechar.
                if state.menu_new.pending_action.is_some() {
                    state.menu_new.pending_action = None;
                    return;
                }
                // Esc no menu fullscreen: query → AllApps → fechar.
                if !state.menu_new.search_query.is_empty() {
                    state.menu_new.search_query.clear();
                    state.menu_new.search_results.clear();
                    state.menu_new.search_selected = 0;
                } else if state.menu_new.mode == MenuMode::AllApps {
                    state.menu_new.mode = MenuMode::Favorites;
                    state.menu_new.focused_zone = FocusedZone::Search;
                } else {
                    state.menu_open = false;
                    state.menu_new.reset();
                }
            }
            MenuKey::Backspace => {
                state.menu_new.search_query.pop();
                state.menu_new.refresh_search_results(&state.apps);
                state.menu_new.focused_zone = crate::menu::state::FocusedZone::Search;
            }
            MenuKey::Char(c) => {
                state.menu_new.search_query.push(c);
                state.menu_new.refresh_search_results(&state.apps);
                state.menu_new.focused_zone = crate::menu::state::FocusedZone::Search;
            }
            MenuKey::Down => {
                if !state.menu_new.search_query.is_empty() {
                    let n = state.menu_new.search_results.len();
                    if n > 0 {
                        state.menu_new.search_selected =
                            (state.menu_new.search_selected + 1) % n;
                    }
                } else {
                    fullscreen_arrow(state, ArrowDir::Down);
                }
            }
            MenuKey::Up => {
                if !state.menu_new.search_query.is_empty() {
                    let n = state.menu_new.search_results.len();
                    if n > 0 {
                        state.menu_new.search_selected =
                            (state.menu_new.search_selected + n - 1) % n;
                    }
                } else {
                    fullscreen_arrow(state, ArrowDir::Up);
                }
            }
            MenuKey::Left => fullscreen_arrow(state, ArrowDir::Left),
            MenuKey::Right => fullscreen_arrow(state, ArrowDir::Right),
            MenuKey::PageUp => {
                let total = crate::menu::all_apps_grid::page_count(state.apps.len());
                let p = state.menu_new.all_apps_page;
                state.menu_new.all_apps_page = if p == 0 { total - 1 } else { p - 1 };
            }
            MenuKey::PageDown => {
                let total = crate::menu::all_apps_grid::page_count(state.apps.len());
                state.menu_new.all_apps_page = (state.menu_new.all_apps_page + 1) % total;
            }
            MenuKey::Tab => {
                // Toggla modo Favorites ↔ AllApps. Preserva search_query.
                use crate::menu::state::{FocusedZone, MenuMode};
                state.menu_new.mode = match state.menu_new.mode {
                    MenuMode::Favorites => MenuMode::AllApps,
                    MenuMode::AllApps => MenuMode::Favorites,
                };
                state.menu_new.focused_zone = match state.menu_new.mode {
                    MenuMode::Favorites => FocusedZone::FavGrid,
                    MenuMode::AllApps => FocusedZone::AllGrid,
                };
            }
            MenuKey::Enter => {
                // Enter: search ativa → lança result selecionado.
                // AllGrid focado → lança app na pos selected.
                if !state.menu_new.search_query.is_empty() {
                    if let Some(&app_idx) = state.menu_new.search_results
                        .get(state.menu_new.search_selected)
                    {
                        if let Some(app) = state.apps.get(app_idx).cloned() {
                            tracing::info!("enter search: {}", app.name);
                            crate::apps::launch(&app.exec);
                        }
                    }
                } else if state.menu_new.focused_zone == crate::menu::state::FocusedZone::AllGrid {
                    let theme = crate::theme::current();
                    let cols = theme.menu_all_apps.columns as usize;
                    let rows = theme.menu_all_apps.rows_per_page as usize;
                    let (col, row) = state.menu_new.all_apps_selected;
                    let slot_in_page = row * cols + col;
                    let app_idx = state.menu_new.all_apps_page * cols * rows + slot_in_page;
                    if let Some(app) = state.apps.get(app_idx).cloned() {
                        tracing::info!("enter all: {}", app.name);
                        crate::apps::launch(&app.exec);
                    }
                }
                state.menu_open = false;
                state.menu_new.reset();
            }
        }
        return;
    }

    // Caminho normal: atalhos do compositor (Alt+Shift+F4 etc) e forward
    // pro cliente focado.
    let action = keyboard.input::<KeyAction, _>(
        state,
        key_code,
        key_state,
        serial,
        time,
        |_state, modifiers, handle| match match_shortcut(modifiers, handle.modified_sym(), key_state)
        {
            Some(action) => FilterResult::Intercept(action),
            None => FilterResult::Forward,
        },
    );

    if let Some(action) = action {
        apply(state, action);
    }
}

enum MenuKey {
    Close,
    Backspace,
    Enter,
    Char(char),
    Up,
    Down,
    Left,
    Right,
    Tab,
    PageUp,
    PageDown,
}

#[derive(Clone, Copy)]
enum ArrowDir { Up, Down, Left, Right }

/// H32.10.9 + H32.10.12: setas no menu fullscreen híbrido. Move foco
/// entre zonas e cursor dentro dos grids (FavGrid 5×2 ou AllGrid 8×4
/// com wrap entre páginas).
fn fullscreen_arrow(state: &mut LumaState, dir: ArrowDir) {
    use crate::menu::state::{FocusedZone, MenuMode};

    // Auto-enter no grid quando a 1ª seta cai vindo da Search.
    if state.menu_new.focused_zone == FocusedZone::Search {
        state.menu_new.focused_zone = match state.menu_new.mode {
            MenuMode::Favorites => FocusedZone::FavGrid,
            MenuMode::AllApps => FocusedZone::AllGrid,
        };
        return;
    }

    match state.menu_new.focused_zone {
        FocusedZone::FavGrid => {
            let cols = crate::theme::current().menu_favorites.columns as usize;
            let rows = crate::theme::current().menu_favorites.rows as usize;
            let total = cols * rows;
            let cur = state.menu_new.fav_selected.min(total - 1);
            let (col, row) = (cur % cols, cur / cols);
            let (new_col, new_row) = match dir {
                ArrowDir::Left => ((col + cols - 1) % cols, row),
                ArrowDir::Right => ((col + 1) % cols, row),
                ArrowDir::Up => (col, (row + rows - 1) % rows),
                ArrowDir::Down => (col, (row + 1) % rows),
            };
            state.menu_new.fav_selected = new_row * cols + new_col;
        }
        FocusedZone::AllGrid => {
            let cols = crate::theme::current().menu_all_apps.columns as usize;
            let rows = crate::theme::current().menu_all_apps.rows_per_page as usize;
            let total = cols * rows;
            let total_pages = crate::menu::all_apps_grid::page_count(state.apps.len());
            let (col, row) = state.menu_new.all_apps_selected;
            let (new_col, new_row, page_delta) = match dir {
                ArrowDir::Left => {
                    if col == 0 { (cols - 1, row, -1isize) } else { (col - 1, row, 0) }
                }
                ArrowDir::Right => {
                    if col + 1 == cols { (0, row, 1isize) } else { (col + 1, row, 0) }
                }
                ArrowDir::Up => {
                    let nr = (row + rows - 1) % rows;
                    (col, nr, 0)
                }
                ArrowDir::Down => {
                    let nr = (row + 1) % rows;
                    (col, nr, 0)
                }
            };
            let _ = total; // wrap horizontal só vira pra próxima página, não usa total
            state.menu_new.all_apps_selected = (new_col, new_row);
            if page_delta != 0 {
                let p = state.menu_new.all_apps_page as isize;
                let np = (p + page_delta).rem_euclid(total_pages as isize);
                state.menu_new.all_apps_page = np as usize;
            }
        }
        _ => {}
    }
}
