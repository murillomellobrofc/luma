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
    // H55: popup do calendário aberto → Left/Right navegam meses.
    let calendar_open = state.applet_popup.as_ref()
        .map(|p| p.kind == crate::popup::AppletKind::Calendar)
        .unwrap_or(false);
    enum CalNav { Prev, Next, TabFwd, TabBack, Enter }
    let mut cal_nav: Option<CalNav> = None;
    // H57.2: popup wifi com password expansion → captura teclado.
    let wifi_pw_active = state.applet_popup.as_ref()
        .map(|p| p.kind == crate::popup::AppletKind::Wifi)
        .unwrap_or(false)
        && state.wifi_password_for.is_some();
    enum WifiPwKey { Char(char), Backspace, Submit, Cancel }
    let mut wifi_pw_event: Option<WifiPwKey> = None;
    // H47a.tray.2: painel de overflow aberto intercepta Esc.
    let tray_overflow_open = state.tray.overflow_open;
    // H48.ctx.1: context menu aberto intercepta Esc.
    let context_menu_open = state.context_menu.is_some();
    // H50.2a.sel.3 + H56 parte 3 (pre-Colar): Esc desseleciona ícones
    // do desktop OU limpa o clipboard interno do Luma quando há algo
    // pra limpar e nenhum popup aberto.
    let any_popup =
        context_menu_open || menu_open_now || applet_popup_open || tray_overflow_open;
    let desktop_has_selection = !state.desktop_selection.is_empty() && !any_popup;
    let desktop_has_clipboard = state.luma_clipboard.is_some() && !any_popup;
    let mut clear_desktop_selection = false;
    let mut clear_luma_clipboard = false;
    // H50.2c.kb: navegação por teclado quando há seleção + nenhuma
    // window com keyboard focus.
    let no_window_focus = keyboard.current_focus().is_none();
    let desktop_kb_active =
        desktop_has_selection && no_window_focus;
    let mut desktop_arrow: Option<crate::desktop_icons::Direction> = None;
    let mut desktop_enter = false;
    // H53.rename: campo inline aberto intercepta toda digitação.
    // Prioridade ALTA — antes do kb_active e do desktop_has_selection.
    let renaming_open = state.desktop_renaming.is_some();
    enum RenameKey {
        Commit,
        Cancel,
        Char(char),
        Backspace,
        Left,
        Right,
        Home,
        End,
    }
    let mut rename_event: Option<RenameKey> = None;
    let mut close_applet_popup = false;
    let mut close_tray_overflow = false;
    let mut close_context_menu = false;
    // H58: lock screen ativo. Esc no placeholder libera (substituído
    // por PAM nas próximas fatias).
    let locked = state.locked;
    let lock_revealed = state.lock_revealed;
    let mut unlock_request = false;
    enum LockKey { RevealOnly, Char(char), Backspace, Submit, Cancel }
    let mut lock_event: Option<LockKey> = None;
    let mut caps_state: Option<bool> = None;
    // H58 fatia 6: whitelist quando locked — XF86 brilho/volume/mídia
    // disparam shortcut sem cair no fluxo do lock e sem chegar em
    // cliente nenhum (nada tem foco durante lock).
    let mut lock_whitelist_action: Option<KeyAction> = None;
    // Modo de edição: Esc desliga (prioridade baixa — só dispara se
    // nenhum popup/menu/etc capturar antes).
    let edit_mode_on = state.edit_mode;
    let mut edit_mode_exit_request = false;
    enum NudgeDir { Left, Right, Up, Down }
    let mut edit_nudge: Option<NudgeDir> = None;
    // Fatia 2.5: campo string inline do config card.
    let config_editing = state.config_editing_field.is_some() && state.config_card_open;
    let config_card_open = state.config_card_open;
    enum ConfigKey { Char(char), Backspace, Enter, EscCancel }
    let mut config_key: Option<ConfigKey> = None;
    // H54.P5: painel da central de notificações aberto intercepta Esc.
    let notification_panel_open = match state.notifications.lock() {
        Ok(g) => g.panel_open,
        Err(_) => false,
    };
    let mut close_notif_panel = false;
    let _ = keyboard.input::<(), _>(
        state,
        key_code,
        key_state,
        serial,
        time,
        |_state, _modifiers, handle| {
            // H58: lock ativo intercepta TODA tecla. Whitelist (fatia 6):
            // XF86 de brilho, volume e mídia passam pro shortcut handler
            // mesmo locked. Resto fica preso no fluxo de lock.
            if locked && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                // Track Caps Lock pra indicator no auth panel.
                caps_state = Some(_modifiers.caps_lock);
                // Whitelist: resolve shortcut e, se a action for de
                // brilho/volume/mídia, dispatcha sem chegar em cliente
                // nenhum. Cobre tanto teclas XF86 nativas quanto
                // fallback Ctrl+Alt+X (modo aninhado em KDE host).
                if let Some(action) =
                    crate::input::shortcuts::match_shortcut(_modifiers, sym, key_state)
                {
                    if is_lock_whitelisted_action(action) {
                        lock_whitelist_action = Some(action);
                        return FilterResult::Intercept(());
                    }
                }
                if !lock_revealed {
                    // Primeira tecla revela o auth panel. Não entra
                    // no campo de senha (per spec H58).
                    if sym == Keysym::Escape {
                        // Esc na capa: libera (placeholder até PAM).
                        unlock_request = true;
                    } else {
                        lock_event = Some(LockKey::RevealOnly);
                    }
                    return FilterResult::Intercept(());
                }
                // Auth panel revelado: edição do campo de senha.
                if sym == Keysym::Escape {
                    // Esc no auth: volta pra capa (cancela tentativa).
                    lock_event = Some(LockKey::Cancel);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Return || sym == Keysym::KP_Enter {
                    // Enter: submete (placeholder até PAM na fatia 5).
                    lock_event = Some(LockKey::Submit);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::BackSpace {
                    lock_event = Some(LockKey::Backspace);
                    return FilterResult::Intercept(());
                }
                let codepoint = smithay::input::keyboard::xkb::keysym_to_utf32(sym);
                if codepoint != 0 {
                    if let Some(ch) = char::from_u32(codepoint) {
                        if !ch.is_control() {
                            lock_event = Some(LockKey::Char(ch));
                            return FilterResult::Intercept(());
                        }
                    }
                }
                return FilterResult::Intercept(());
            }
            if notification_panel_open && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    close_notif_panel = true;
                    return FilterResult::Intercept(());
                }
            }
            if applet_popup_open && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    close_applet_popup = true;
                    return FilterResult::Intercept(());
                }
                // H57.2: password inline do Wi-Fi captura teclas.
                if wifi_pw_active {
                    if sym == Keysym::Return || sym == Keysym::KP_Enter {
                        wifi_pw_event = Some(WifiPwKey::Submit);
                        return FilterResult::Intercept(());
                    }
                    if sym == Keysym::BackSpace {
                        wifi_pw_event = Some(WifiPwKey::Backspace);
                        return FilterResult::Intercept(());
                    }
                    let codepoint = smithay::input::keyboard::xkb::keysym_to_utf32(sym);
                    if codepoint != 0 {
                        if let Some(ch) = char::from_u32(codepoint) {
                            if !ch.is_control() {
                                wifi_pw_event = Some(WifiPwKey::Char(ch));
                                return FilterResult::Intercept(());
                            }
                        }
                    }
                }
                // H55: navegação por teclado quando é o calendário.
                if calendar_open {
                    if sym == Keysym::Left {
                        cal_nav = Some(CalNav::Prev);
                        return FilterResult::Intercept(());
                    }
                    if sym == Keysym::Right {
                        cal_nav = Some(CalNav::Next);
                        return FilterResult::Intercept(());
                    }
                    // Fatia 4: Tab cicla foco entre Prev/Hoje/Next.
                    if sym == Keysym::Tab {
                        let shift = _modifiers.shift;
                        cal_nav = Some(if shift {
                            CalNav::TabBack
                        } else {
                            CalNav::TabFwd
                        });
                        return FilterResult::Intercept(());
                    }
                    if sym == Keysym::Return || sym == Keysym::KP_Enter {
                        cal_nav = Some(CalNav::Enter);
                        return FilterResult::Intercept(());
                    }
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
            // H53.rename: prioridade absoluta sobre os outros gates
            // do desktop quando o campo está aberto.
            if renaming_open && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    rename_event = Some(RenameKey::Cancel);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Return || sym == Keysym::KP_Enter {
                    rename_event = Some(RenameKey::Commit);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::BackSpace {
                    rename_event = Some(RenameKey::Backspace);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Left {
                    rename_event = Some(RenameKey::Left);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Right {
                    rename_event = Some(RenameKey::Right);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Home {
                    rename_event = Some(RenameKey::Home);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::End {
                    rename_event = Some(RenameKey::End);
                    return FilterResult::Intercept(());
                }
                let codepoint = smithay::input::keyboard::xkb::keysym_to_utf32(sym);
                if codepoint != 0 {
                    if let Some(ch) = char::from_u32(codepoint) {
                        if !ch.is_control() {
                            rename_event = Some(RenameKey::Char(ch));
                            return FilterResult::Intercept(());
                        }
                    }
                }
                // Outras teclas (modifiers só, F-keys etc) passam.
            }
            if (desktop_has_selection || desktop_has_clipboard)
                && key_state == KeyState::Pressed
            {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    if desktop_has_selection {
                        clear_desktop_selection = true;
                    }
                    if desktop_has_clipboard {
                        clear_luma_clipboard = true;
                    }
                    return FilterResult::Intercept(());
                }
            }
            // H50.2c.kb.4-5: navegação por teclado no desktop.
            if desktop_kb_active && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                use crate::desktop_icons::Direction;
                if sym == Keysym::Up {
                    desktop_arrow = Some(Direction::Up);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Down {
                    desktop_arrow = Some(Direction::Down);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Left {
                    desktop_arrow = Some(Direction::Left);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Right {
                    desktop_arrow = Some(Direction::Right);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Return || sym == Keysym::KP_Enter {
                    desktop_enter = true;
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
            // Modo de edição: Esc desliga. Roda DEPOIS de todos os
            // outros Esc handlers — se um popup/menu já fechou via
            // Esc, ele retornou Intercept e nunca chega aqui.
            // Fatia 2.5: input do campo string do config card.
            if config_editing && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Return || sym == Keysym::KP_Enter {
                    config_key = Some(ConfigKey::Enter);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Escape {
                    config_key = Some(ConfigKey::EscCancel);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::BackSpace {
                    config_key = Some(ConfigKey::Backspace);
                    return FilterResult::Intercept(());
                }
                let codepoint = smithay::input::keyboard::xkb::keysym_to_utf32(sym);
                if codepoint != 0 {
                    if let Some(ch) = char::from_u32(codepoint) {
                        if !ch.is_control() {
                            config_key = Some(ConfigKey::Char(ch));
                            return FilterResult::Intercept(());
                        }
                    }
                }
                return FilterResult::Intercept(());
            }
            if edit_mode_on && key_state == KeyState::Pressed {
                let sym = handle.modified_sym();
                if sym == Keysym::Escape {
                    if config_card_open {
                        config_key = Some(ConfigKey::EscCancel);
                        return FilterResult::Intercept(());
                    }
                    edit_mode_exit_request = true;
                    return FilterResult::Intercept(());
                }
                // Fatia 2.3: nudge do widget selecionado por seta.
                if sym == Keysym::Left {
                    edit_nudge = Some(NudgeDir::Left);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Right {
                    edit_nudge = Some(NudgeDir::Right);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Up {
                    edit_nudge = Some(NudgeDir::Up);
                    return FilterResult::Intercept(());
                }
                if sym == Keysym::Down {
                    edit_nudge = Some(NudgeDir::Down);
                    return FilterResult::Intercept(());
                }
            }
            FilterResult::Forward
        },
    );

    if let Some(c) = caps_state {
        if state.lock_caps_on != c {
            state.lock_caps_on = c;
        }
    }
    // H58 fatia 6: whitelist (brilho/volume/mídia) durante lock — aplica
    // a action diretamente, sem passar pelo input() do cliente.
    if let Some(action) = lock_whitelist_action {
        apply(state, action);
        return;
    }
    // Fatia 2.5: config card input dispatch.
    if let Some(ck) = config_key {
        match ck {
            ConfigKey::Char(ch) => {
                if let Some(key) = state.config_editing_field.as_ref() {
                    let val = state.config_form_values
                        .get(key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let mut s = val;
                    s.push(ch);
                    state.config_form_values.insert(
                        key.clone(), toml::Value::String(s),
                    );
                }
            }
            ConfigKey::Backspace => {
                if let Some(key) = state.config_editing_field.as_ref() {
                    let val = state.config_form_values
                        .get(key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let mut s = val;
                    s.pop();
                    state.config_form_values.insert(
                        key.clone(), toml::Value::String(s),
                    );
                }
            }
            ConfigKey::Enter => {
                state.config_editing_field = None;
            }
            ConfigKey::EscCancel => {
                if state.config_editing_field.is_some() {
                    state.config_editing_field = None;
                } else {
                    state.config_card_open = false;
                }
            }
        }
        return;
    }
    if edit_mode_exit_request {
        // Painel: se card aberto, Esc cancela (reverte), não sai do modo.
        if state.panel_settings_open {
            if let Some(snap) = state.panel_config_snapshot.take() {
                crate::taskbar::set_taskbar_height(snap.height);
                state.panel_config = snap;
            }
            state.panel_settings_open = false;
            state.panel_selected = false;
            return;
        }
        state.edit_mode = false;
        state.selected_widget_id = None;
        state.handle_confirm_mode = false;
        state.config_card_open = false;
        state.config_editing_field = None;
        state.panel_selected = false;
        state.panel_settings_open = false;
        tracing::info!("modo de edição: desligado via Esc");
        return;
    }
    // Fatia 2.3: nudge do widget selecionado por seta.
    if let Some(dir) = edit_nudge {
        if let Some(sel_id) = state.selected_widget_id.clone() {
            // Checa se é painel (troca posição ±1) ou desktop (±10px).
            let panel_idx = state.widget_ids.iter()
                .position(|id| *id == sel_id);
            if let Some(idx) = panel_idx {
                // Painel: esquerda/direita troca ordem, cima/baixo no-op.
                let n = state.widgets.len();
                let target = match dir {
                    NudgeDir::Left if idx > 0 => Some(idx - 1),
                    NudgeDir::Right if idx + 1 < n => Some(idx + 1),
                    _ => None,
                };
                if let Some(t) = target {
                    state.widgets.swap(idx, t);
                    state.widget_ids.swap(idx, t);
                    state.widget_last_ticks.swap(idx, t);
                    let _ = crate::widgets::save_panel_order(
                        &state.widget_ids, &state.widget_all_ids,
                    );
                }
            } else {
                // Desktop: ±10px.
                let delta = 10i32;
                let (dx, dy) = match dir {
                    NudgeDir::Left => (-delta, 0),
                    NudgeDir::Right => (delta, 0),
                    NudgeDir::Up => (0, -delta),
                    NudgeDir::Down => (0, delta),
                };
                let moved = {
                    let items: Vec<_> = state.space.elements()
                        .filter_map(|item| {
                            if let crate::space_item::SpaceItem::DesktopWidget(dw) = item {
                                if dw.id == sel_id {
                                    state.space.element_location(item).map(|loc| (dw.clone(), loc))
                                } else { None }
                            } else { None }
                        })
                        .collect();
                    items.into_iter().next()
                };
                if let Some((item, loc)) = moved {
                    let nx = loc.x + dx;
                    let ny = loc.y + dy;
                    state.space.map_element(
                        crate::space_item::SpaceItem::DesktopWidget(item.clone()),
                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((nx, ny)),
                        false,
                    );
                    let _ = crate::widgets::save_desktop_widget_position(
                        &sel_id, nx, ny, &state.widget_all_ids,
                    );
                }
            }
        }
        return;
    }
    if let Some(ev) = lock_event {
        match ev {
            LockKey::RevealOnly => {
                state.lock_revealed = true;
                state.lock_password.clear();
            }
            LockKey::Char(ch) => {
                state.lock_password.push(ch);
                state.lock_auth_error = None;
            }
            LockKey::Backspace => {
                state.lock_password.pop();
                state.lock_auth_error = None;
            }
            LockKey::Submit => {
                // H58 fatia 5: dispara PAM real numa thread.
                // Ignora se já tem auth em andamento ou campo vazio.
                if !state.lock_password.is_empty() && !state.lock_auth_in_flight {
                    let user = std::env::var("USER")
                        .unwrap_or_else(|_| "root".to_string());
                    let pw = state.lock_password.clone();
                    state.lock_auth_in_flight = true;
                    state.lock_auth_error = None;
                    crate::auth::authenticate_async(
                        state.lock_auth_outbox.clone(),
                        user, pw,
                    );
                }
            }
            LockKey::Cancel => {
                state.lock_revealed = false;
                state.lock_password.clear();
            }
        }
        return;
    }
    if unlock_request {
        state.locked = false;
        state.locked_at = None;
        state.lock_revealed = false;
        state.lock_password.clear();
        state.lock_surfaces.clear();
        tracing::info!("luma unlock via Esc (placeholder até PAM)");
        return;
    }

    if close_notif_panel {
        if let Ok(mut g) = state.notifications.lock() {
            crate::notifications::close_panel(&mut g);
        }
        return;
    }

    if close_applet_popup {
        state.applet_popup = None;
        return;
    }

    // H57.2: aplica evento de password do Wi-Fi.
    if let Some(ev) = wifi_pw_event {
        match ev {
            WifiPwKey::Char(ch) => {
                state.wifi_password_text.push(ch);
                state.wifi_password_cursor = state.wifi_password_text.len();
            }
            WifiPwKey::Backspace => {
                state.wifi_password_text.pop();
                state.wifi_password_cursor = state.wifi_password_text.len();
            }
            WifiPwKey::Submit => {
                if let Some(ssid) = state.wifi_password_for.clone() {
                    let pw = state.wifi_password_text.clone();
                    state.wifi_connecting = Some(ssid.clone());
                    state.wifi_error_for = None;
                    crate::wifi::connect_async(
                        state.wifi_connect_outbox.clone(),
                        ssid, Some(pw),
                    );
                }
            }
            WifiPwKey::Cancel => {
                state.wifi_password_for = None;
                state.wifi_password_text.clear();
                state.wifi_password_cursor = 0;
            }
        }
        return;
    }

    // H55: navegação por teclado no calendário.
    if let Some(nav) = cal_nav {
        if let Some(p) = state.applet_popup.as_mut() {
            match nav {
                CalNav::Prev => p.prev_month(),
                CalNav::Next => p.next_month(),
                CalNav::TabFwd => p.cycle_focus(true),
                CalNav::TabBack => p.cycle_focus(false),
                CalNav::Enter => p.activate_focused(),
            }
        }
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

    if clear_desktop_selection || clear_luma_clipboard {
        if clear_desktop_selection {
            state.desktop_selection.clear();
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_selection(std::collections::HashSet::new());
            }
        }
        if clear_luma_clipboard {
            state.luma_clipboard = None;
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_cut_paths(std::collections::HashSet::new());
            }
            tracing::info!("luma clipboard cleared (Esc)");
        }
        return;
    }

    if let Some(ev) = rename_event {
        match ev {
            RenameKey::Cancel => {
                state.desktop_renaming = None;
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_renaming(None);
                }
            }
            RenameKey::Commit => {
                if let Some(r) = state.desktop_renaming.take() {
                    commit_rename(state, r);
                }
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_renaming(None);
                }
            }
            RenameKey::Char(c) => {
                if let Some(r) = state.desktop_renaming.as_mut() {
                    r.insert_char(c);
                    if let Some(l) = state.desktop_icons.as_ref() {
                        l.set_renaming(Some(r.clone()));
                    }
                }
            }
            RenameKey::Backspace => {
                if let Some(r) = state.desktop_renaming.as_mut() {
                    r.backspace();
                    if let Some(l) = state.desktop_icons.as_ref() {
                        l.set_renaming(Some(r.clone()));
                    }
                }
            }
            RenameKey::Left => {
                if let Some(r) = state.desktop_renaming.as_mut() {
                    r.cursor_left();
                    if let Some(l) = state.desktop_icons.as_ref() {
                        l.set_renaming(Some(r.clone()));
                    }
                }
            }
            RenameKey::Right => {
                if let Some(r) = state.desktop_renaming.as_mut() {
                    r.cursor_right();
                    if let Some(l) = state.desktop_icons.as_ref() {
                        l.set_renaming(Some(r.clone()));
                    }
                }
            }
            RenameKey::Home => {
                if let Some(r) = state.desktop_renaming.as_mut() {
                    r.cursor_home();
                    if let Some(l) = state.desktop_icons.as_ref() {
                        l.set_renaming(Some(r.clone()));
                    }
                }
            }
            RenameKey::End => {
                if let Some(r) = state.desktop_renaming.as_mut() {
                    r.cursor_end();
                    if let Some(l) = state.desktop_icons.as_ref() {
                        l.set_renaming(Some(r.clone()));
                    }
                }
            }
        }
        return;
    }

    if let Some(dir) = desktop_arrow {
        if let Some(layer) = state.desktop_icons.as_ref() {
            // Single-selection (path mais lexicograficamente pequeno
            // entre os selecionados, pra determinismo).
            let current_path = state.desktop_selection.iter().min().cloned();
            if let Some(curr) = current_path {
                if let Some(target) = layer.neighbor_path(
                    &curr,
                    &state.desktop_positions,
                    dir,
                ) {
                    let mut sel = std::collections::HashSet::new();
                    sel.insert(target);
                    layer.set_selection(sel.clone());
                    state.desktop_selection = sel;
                }
            }
        }
        return;
    }

    if desktop_enter {
        let paths: Vec<std::path::PathBuf> =
            state.desktop_selection.iter().cloned().collect();
        for p in paths {
            tracing::info!("desktop: open {:?}", p);
            let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
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

/// Commit do rename: faz fs::rename, atualiza desktop_positions com
/// a key nova (H53 decisão 3), atualiza selection pra apontar pro
/// path novo. Nenhum caminho pode crashar — todo erro só loga warn.
pub(crate) fn commit_rename(
    state: &mut crate::state::LumaState,
    rename: crate::desktop_icons::DesktopRename,
) {
    let new_name = rename.text.trim().to_string();
    // Nome vazio OU igual ao original = cancel silencioso.
    if new_name.is_empty() || new_name == rename.original_name {
        return;
    }
    if new_name.contains('/') {
        tracing::warn!("rename: rejeitado — nome com '/'");
        return;
    }
    let new_path = rename.path.with_file_name(&new_name);
    if new_path.exists() {
        tracing::warn!("rename: ja existe {new_path:?}");
        return;
    }
    match std::fs::rename(&rename.path, &new_path) {
        Ok(()) => {
            tracing::info!("rename: {:?} -> {:?}", rename.path, new_path);
            // Atualiza positions: renomeia a key se existir.
            if let Some(pos) = state.desktop_positions.remove(&rename.original_name) {
                state
                    .desktop_positions
                    .insert(new_name.clone(), pos);
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_positions(state.desktop_positions.clone());
                }
                crate::desktop_icons::save_positions(&state.desktop_positions);
            }
            // Atualiza selection: troca old path por new path.
            if state.desktop_selection.remove(&rename.path) {
                state.desktop_selection.insert(new_path.clone());
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_selection(state.desktop_selection.clone());
                }
            }
        }
        Err(e) => tracing::warn!("rename: falhou ({e})"),
    }
}

/// H58 fatia 6: actions que rodam mesmo com lock ativo. Brilho, volume
/// e mídia — tanto via XF86 puro quanto via fallback Ctrl+Alt+X.
/// LockSession fica de fora (já está travado, no-op faria sentido mas
/// o handler resolve idempotência sozinho — só não vale a pena gastar
/// dispatch). Workspaces, menu, close, overview ficam BLOQUEADOS.
fn is_lock_whitelisted_action(action: KeyAction) -> bool {
    matches!(
        action,
        KeyAction::VolumeRaise
            | KeyAction::VolumeLower
            | KeyAction::VolumeMute
            | KeyAction::BrightnessRaise
            | KeyAction::BrightnessLower
            | KeyAction::MediaPlayPause
            | KeyAction::MediaNext
            | KeyAction::MediaPrev
            | KeyAction::MediaStop
    )
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
