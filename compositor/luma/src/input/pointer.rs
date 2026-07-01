use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend,
        PointerAxisEvent, PointerButtonEvent,
    },
    input::pointer::{AxisFrame, ButtonEvent, Focus, GrabStartData, MotionEvent},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
};

use crate::{
    input::grabs::{move_grab::MoveSurfaceGrab, resize_grab},
    window::LumaWindow,
};

use crate::state::LumaState;

pub fn handle_motion_absolute<I: InputBackend>(
    state: &mut LumaState,
    event: I::PointerMotionAbsoluteEvent,
) {
    // No motion absoluto não temos o `pos` ainda — o output transforma
    // o evento. Usa o primário como referência (multi-monitor real
    // precisará rotear baseado em qual janela winit emitiu o evento).
    let output = state.primary_output().clone();
    let Some(output_geo) = state.space.output_geometry(&output) else {
        return;
    };

    let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();
    state.pointer_location = pos;
    let serial = SERIAL_COUNTER.next_serial();
    let Some(pointer) = state.seat.get_pointer() else {
        return;
    };

    // H57 polish: hover sobre algum applet do tray — pinta com
    // applet_bg_hover atrás do ícone.
    {
        let mut h: Option<crate::tray::TrayHover> = None;
        // Ordem: bell, clock, bluetooth, wifi, battery, volume.
        // Cada um cacheia seu rect no render path.
        if state.notification_bell_rect.map_or(false, |(x, y, w, ht)| {
            pos.x >= x as f64 && pos.x < (x + w) as f64
                && pos.y >= y as f64 && pos.y < (y + ht) as f64
        }) {
            h = Some(crate::tray::TrayHover::Bell);
        } else if state.tray.last_clock_rect
            .map_or(false, |r| r.to_f64().contains(pos))
        {
            h = Some(crate::tray::TrayHover::Clock);
        } else if state.tray.last_bt_rect
            .map_or(false, |r| r.to_f64().contains(pos))
        {
            h = Some(crate::tray::TrayHover::Bluetooth);
        } else if state.tray.last_wifi_rect
            .map_or(false, |r| r.to_f64().contains(pos))
        {
            h = Some(crate::tray::TrayHover::Wifi);
        } else if state.tray.last_battery_rect
            .map_or(false, |r| r.to_f64().contains(pos))
        {
            h = Some(crate::tray::TrayHover::Battery);
        } else {
            // Volume usa tray_volume_zone (computado, não cacheado).
            let output_size = state
                .space
                .output_geometry(state.output_under_cursor(pos))
                .map(|g| g.size);
            if let Some(output) = output_size {
                if let Some(zone) = crate::taskbar::tray_volume_zone(output, &state.tray) {
                    if zone.to_f64().contains(pos) {
                        h = Some(crate::tray::TrayHover::Volume);
                    }
                }
            }
        }
        if state.tray.hover != h {
            state.tray.hover = h;
        }
    }

    // H54.P4: hover sobre algum action button — pinta com bg_hover.
    {
        let mut new_hover: Option<(u32, String)> = None;
        for (id, key, bx, by, bw, bh) in state.notification_button_rects.iter() {
            let inside = pos.x >= *bx as f64
                && pos.x < (*bx + *bw) as f64
                && pos.y >= *by as f64
                && pos.y < (*by + *bh) as f64;
            if inside {
                new_hover = Some((*id, key.clone()));
                break;
            }
        }
        if state.notification_button_hover != new_hover {
            state.notification_button_hover = new_hover;
        }
    }

    // H55 Fatia 4: hover do mouse sobre as setas do calendário.
    // Atualiza popup.calendar_hover pra o render pintar com seta_fg_hover.
    if let Some(p) = state.applet_popup.as_mut() {
        if p.kind == crate::popup::AppletKind::Calendar {
            let mut new_hover: Option<u8> = None;
            if state.tray.last_clock_rect.is_some() { /* gate visual */ }
            if state.calendar_prev_rect
                .map_or(false, |r| r.to_f64().contains(pos))
            {
                new_hover = Some(0);
            } else if state.calendar_next_rect
                .map_or(false, |r| r.to_f64().contains(pos))
            {
                new_hover = Some(2);
            }
            if p.calendar_hover != new_hover {
                p.calendar_hover = new_hover;
            }
        }
    }

    // H54.P5.fix: hover sobre item do histórico — usa os rects
    // cacheados pelo render path (altura por item é dinâmica, varia
    // com nº de linhas do body).
    {
        let mut new_hover: Option<u32> = None;
        for (id, ix, iy, iw, ih) in state.notification_history_item_rects.iter() {
            if pos.x >= *ix as f64
                && pos.x < (*ix + *iw) as f64
                && pos.y >= *iy as f64
                && pos.y < (*iy + *ih) as f64
            {
                new_hover = Some(*id);
                break;
            }
        }
        state.notification_history_hover = new_hover;
    }

    // Personalização contextual: hover do widget context menu.
    if let Some(wcm) = state.widget_context_menu.as_mut() {
        let t_wm = crate::theme::current().widget_menu.clone();
        let pad = t_wm.container_padding;
        let ih = t_wm.item_height;
        let menu_w = t_wm.min_width + pad * 2;
        let mx = wcm.origin.x;
        let my = wcm.origin.y;
        let mut iy = my + pad;
        let mut new_hover = None;
        for (i, item) in wcm.items.iter().enumerate() {
            if matches!(item, luma_widget::ContextMenuItem::Action { enabled: true, .. }) {
                let rect = smithay::utils::Rectangle::<i32, smithay::utils::Logical>::new(
                    smithay::utils::Point::from((mx + pad, iy)),
                    smithay::utils::Size::from((menu_w - pad * 2, ih)),
                );
                if rect.to_f64().contains(pos) {
                    new_hover = Some(i);
                }
            }
            iy += ih;
        }
        wcm.hovered = new_hover;
    }

    // H54 P3: hover sobre QUALQUER toast pausa o timer de TODOS.
    // Quando cursor sai de todos os rects, retoma — acumula
    // paused_total e zera pause_started_at em cada entry.
    {
        let any_hover = state
            .notification_rects
            .iter()
            .any(|(_, nx, ny, nw, nh)| {
                pos.x >= *nx as f64
                    && pos.x < (*nx + *nw) as f64
                    && pos.y >= *ny as f64
                    && pos.y < (*ny + *nh) as f64
            });
        if let Ok(mut g) = state.notifications.lock() {
            if any_hover && !g.hover_paused {
                g.hover_paused = true;
                let now = std::time::Instant::now();
                for e in g.visible.iter_mut() {
                    if e.pause_started_at.is_none() {
                        e.pause_started_at = Some(now);
                    }
                }
            } else if !any_hover && g.hover_paused {
                g.hover_paused = false;
                for e in g.visible.iter_mut() {
                    if let Some(t) = e.pause_started_at.take() {
                        e.paused_total += t.elapsed();
                    }
                }
            }
        }
    }

    // H50.2b.drag.1-2: atualiza state.desktop_drag se estiver
    // pendente/ativo. Threshold de 5px promove pra active; depois
    // disso, cada motion atualiza current_screen pra ghost seguir.
    if let Some(drag) = state.desktop_drag.as_mut() {
        drag.current_screen = pos;
        if !drag.active {
            let dx = pos.x - drag.start_screen.x;
            let dy = pos.y - drag.start_screen.y;
            if (dx * dx + dy * dy).sqrt()
                >= crate::desktop_icons::DRAG_THRESHOLD_PX
            {
                drag.active = true;
            }
        }
        if drag.active {
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_drag(Some(drag.clone()));
            }
        }
    }

    // H50.2c.rb.2: rubber-band — atualiza current enquanto o user
    // arrasta o mouse.
    if let Some(rb) = state.desktop_rubber_band.as_mut() {
        rb.current = pos;
        if let Some(l) = state.desktop_icons.as_ref() {
            l.set_rubber_band(Some(rb.clone()));
        }
    }

    // Atualiza hover dos headers + escolhe cursor_kind por contexto numa
    // única passagem pelas janelas do topo pro fundo.
    let mut hover_set = false;
    let mut chosen_kind: &'static str = "default";
    let window_items: Vec<_> = state
        .space
        .elements()
        .rev()
        .filter_map(|i| {
            let w = i.as_window()?;
            let loc = state.space.element_location(i)?;
            Some((w.clone(), loc))
        })
        .collect();
    for (window, loc) in &window_items {
        let width = window.window().geometry().size.w.max(1);
        let size = {
            use smithay::desktop::space::SpaceElement;
            SpaceElement::geometry(window).size
        };
        let relative = pos - loc.to_f64();
        let region = if !hover_set {
            crate::decoration::hit_test_header(width, relative)
        } else {
            None
        };
        if region.is_some() {
            hover_set = true;
        }
        crate::decoration::update_hover(window.window(), region);

        if chosen_kind == "default" && !hover_set {
            if let Some(edge) =
                crate::input::grabs::resize_grab::hit_test_resize_edge(relative, size)
            {
                chosen_kind = cursor_name_for_resize_edge(edge);
            }
        }
    }
    // Menu fullscreen aberto: cursor sempre default (seta). Sem isso
    // o hover em janela atrás setava cursor_kind pra resize edges /
    // text I-beam (qualquer cursor da janela embaixo).
    state.cursor_kind = if state.menu_open { "default" } else { chosen_kind };

    // Hover na taskbar: se está sobre um bloco de janela, marca esse índice;
    // senão (launcher, show-desktop, fora) limpa.
    let output_size = {
        let o = state.output_under_cursor(pos);
        state.space.output_geometry(o).map(|g| g.size)
    };
    if let Some(out) = output_size {
        let n_windows = state
            .space
            .elements()
            .filter(|i| i.as_window().is_some())
            .count();
        let ws_count = state.workspaces.count();
        let new_hover = match crate::taskbar::hit_test(out, pos, n_windows, ws_count) {
            Some(crate::taskbar::TaskbarHit::Window(idx)) => Some(idx),
            _ => None,
        };
        // H33.taskbar.6: reset timer do tooltip toda vez que o hovered_block
        // muda. Tooltip aparece quando passa `apps.tooltip_delay_ms` no
        // mesmo bloco.
        if state.taskbar.hovered_block != new_hover {
            state.taskbar.hover_started_at = new_hover.map(|_| std::time::Instant::now());
        }
        state.taskbar.hovered_block = new_hover;
        state.taskbar.show_desktop_hovered = matches!(
            crate::taskbar::hit_test(out, pos, n_windows, ws_count),
            Some(crate::taskbar::TaskbarHit::ShowDesktop)
        );
        // H36.9: hover do pager.
        state.pager.hovered = pager_hit(state, pos);
        // H32.10.fix: hover do menu fullscreen — session bar + grids
        // + search bar pill.
        if state.menu_open {
            let sb_origin = state.menu_fullscreen.zone_origin(out, 4);
            state.menu_new.session_hovered =
                crate::menu::session_bar::hit_test_buttons(pos, sb_origin);
            let sb_w = out.w
                - 2 * crate::menu::render::layout::SESSION_MARGIN_HORIZONTAL;
            let sb_h = crate::menu::render::layout::SESSION_H;
            let sb_size = smithay::utils::Size::<i32, smithay::utils::Logical>
                ::from((sb_w, sb_h));
            state.menu_new.place_hovered =
                crate::menu::session_bar::hit_test_places(pos, sb_origin, sb_size);
            // Link "TODOS OS APPS" / "← FAVORITOS" — hit-test zone[3].
            let l_origin = state.menu_fullscreen.zone_origin(out, 3);
            let l_w = crate::menu::render::layout::ALL_APPS_LINK_W;
            let l_h = crate::menu::render::layout::ALL_APPS_LINK_H;
            state.menu_new.link_hovered = pos.x >= l_origin.x as f64
                && pos.x < (l_origin.x + l_w) as f64
                && pos.y >= l_origin.y as f64
                && pos.y < (l_origin.y + l_h) as f64
                && state.menu_new.search_query.is_empty();
            // Search bar pill (zone[1]) — hover ativa borda âmbar.
            let search_origin = state.menu_fullscreen.zone_origin(out, 1);
            let s_theme = &crate::theme::current().menu_search_bar;
            state.menu_new.search_bar_hovered = pos.x >= search_origin.x as f64
                && pos.x < (search_origin.x + s_theme.width) as f64
                && pos.y >= search_origin.y as f64
                && pos.y < (search_origin.y + s_theme.height) as f64;
            let fav_origin = state.menu_fullscreen.zone_origin(out, 2);
            if !state.menu_new.search_query.is_empty() {
                state.menu_new.search_hovered = hit_test_search_results(
                    fav_origin, pos, &state.menu_new.search_results,
                );
                state.menu_new.fav_hovered = None;
                state.menu_new.all_hovered = None;
            } else if state.menu_new.mode == crate::menu::state::MenuMode::Favorites {
                state.menu_new.fav_hovered = hit_test_favorites(fav_origin, pos);
                state.menu_new.all_hovered = None;
                state.menu_new.search_hovered = None;
            } else {
                state.menu_new.all_hovered = hit_test_all_apps(
                    fav_origin, pos, state.menu_new.all_apps_page, state.apps.len(),
                );
                state.menu_new.fav_hovered = None;
                state.menu_new.search_hovered = None;
            }
        }
    }

    // H48.ctx.2: hover do menu de contexto. Atualiza hovered + abre/
    // fecha submenu conforme item-Submenu sob o cursor.
    if state.context_menu.is_some() {
        let output_size = state
            .space
            .output_geometry(state.output_under_cursor(pos))
            .map(|g| g.size);
        if let Some(out) = output_size {
            let tokens = crate::theme::current().context_menu.clone();
            let menu_clone = state.context_menu.clone();
            if let Some(snap) = menu_clone {
                // Hovers (sub + main) atualizados sempre.
                let sub_hover = snap.submenu_hit_test(&tokens, out, pos);
                let main_hover = snap.hit_test(&tokens, pos);
                // Região unificada: mantém o submenu aberto enquanto o
                // cursor estiver dentro do main rect OU do submenu rect.
                // Fecha só quando sai dos DOIS. Bug clássico: cursor
                // viajando na diagonal do item-pai pro submenu cruza
                // outros items (não-Submenu), e a versão antiga fechava
                // o submenu no caminho. Agora só fecha quando sai
                // mesmo, ou quando o cursor pousa em OUTRO item
                // Submenu-kind (troca).
                let in_main_rect = snap.rect(&tokens).to_f64().contains(pos);
                // H47fix++: ALARGA o sub rect 12px à esquerda pra incluir
                // o gap de 2px entre main e submenu + margem pro cursor
                // viajando. Sem isso, 1-2 frames com cursor dentro do
                // gap (vec, none-none) fecham o submenu permanentemente.
                let in_sub_rect = snap
                    .submenu_rect(&tokens, out)
                    .map(|r| {
                        // H56.fix: bounding box que cobre main ∪ sub
                        // ∪ qualquer gap entre eles. O submenu pode
                        // abrir à esquerda OU direita do main, e ficar
                        // clamped pra cima/baixo quando o menu encosta
                        // na borda. O bounding union garante que o
                        // cursor mantém o submenu aberto durante toda
                        // a viagem main↔sub, em qualquer eixo.
                        let main = snap.rect(&tokens);
                        let l = main.loc.x.min(r.loc.x);
                        let t = main.loc.y.min(r.loc.y);
                        let r_edge = (main.loc.x + main.size.w)
                            .max(r.loc.x + r.size.w);
                        let b = (main.loc.y + main.size.h)
                            .max(r.loc.y + r.size.h);
                        let unified = smithay::utils::Rectangle::<i32, smithay::utils::Logical>::new(
                            smithay::utils::Point::from((l, t)),
                            smithay::utils::Size::from((r_edge - l, b - t)),
                        );
                        // NÃO contains o ponto se estiver no rect main
                        // (esse caso já é tratado por in_main_rect com
                        // semântica diferente). Aqui só queremos saber
                        // se cursor está perto do submenu ou no gap.
                        unified.to_f64().contains(pos)
                    })
                    .unwrap_or(false);
                let new_open_submenu: Option<usize> = if let Some(idx) = main_hover {
                    let is_sub = snap
                        .items
                        .get(idx)
                        .map(|i| matches!(
                            i.kind,
                            crate::context_menu::ItemKind::Submenu(_)
                        ))
                        .unwrap_or(false);
                    if is_sub {
                        Some(idx) // troca/abre pro item Submenu hovered
                    } else {
                        // Item non-Submenu no main → mantém o atual.
                        // (User pode estar viajando pro submenu.)
                        snap.open_submenu
                    }
                } else if in_main_rect || in_sub_rect {
                    // Separator do main OU cursor sobre o próprio
                    // submenu → mantém atual.
                    snap.open_submenu
                } else {
                    // Fora dos dois rects → fecha.
                    None
                };
                if let Some(m) = state.context_menu.as_mut() {
                    m.submenu_hovered = sub_hover;
                    m.hovered = main_hover;
                    m.open_submenu = new_open_submenu;
                }
            }
        }
    }

    // H36.16: hot corner KDE-style. Trigger imediato ao tocar canto
    // absoluto (x<=5, y<=5). Toggle overview. Re-arma quando cursor
    // sai da zona, pra evitar re-trigger contínuo.
    const HOT_CORNER_PX: f64 = 5.0;
    let touching = pos.x <= HOT_CORNER_PX && pos.y <= HOT_CORNER_PX;
    let armed = state.hot_corner_dwell_start.is_some();
    if touching && armed {
        state.overview_open = !state.overview_open;
        state.hot_corner_dwell_start = None;
    } else if !touching {
        state.hot_corner_dwell_start = Some(std::time::Instant::now());
    }

    // Menu fullscreen aberto → não envia motion pra surface atrás
    // (senão cliente atrás pode mudar cursor pra I-beam, etc).
    // H58 fatia 6: lock ativo também suprime motion pra clientes —
    // só o lock UI recebe input enquanto travado.
    let under = if state.menu_open || state.locked {
        None
    } else {
        state.surface_under(pos)
    };

    pointer.motion(
        state,
        under,
        &MotionEvent {
            location: pos,
            serial,
            time: event.time_msec(),
        },
    );
    pointer.frame(state);
}

pub fn handle_button<I: InputBackend>(state: &mut LumaState, event: I::PointerButtonEvent) {
    let Some(pointer) = state.seat.get_pointer() else {
        return;
    };
    let Some(keyboard) = state.seat.get_keyboard() else {
        return;
    };
    let serial = SERIAL_COUNTER.next_serial();
    let button = event.button_code();
    let button_state = event.state();
    let pointer_pos = pointer.current_location();

    // H50.2c.rb.3: mouseup com rubber-band ativo → seleciona todos
    // os ícones cujas células intersectam o rect. Box muito pequena
    // (< threshold) = só fecha sem seleção (era click curto, não
    // drag real).
    if button_state == ButtonState::Released {
        if let Some(rb) = state.desktop_rubber_band.take() {
            let dx = rb.current.x - rb.start.x;
            let dy = rb.current.y - rb.start.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist >= crate::desktop_icons::DRAG_THRESHOLD_PX {
                if let Some(l) = state.desktop_icons.as_ref() {
                    let rect = rb.rect();
                    let paths = l.entries_in_rect(rect);
                    let mods = state
                        .seat
                        .get_keyboard()
                        .map(|kb| kb.modifier_state());
                    let ctrl = mods.map(|m| m.ctrl).unwrap_or(false);
                    let mut sel = if ctrl {
                        state.desktop_selection.clone()
                    } else {
                        std::collections::HashSet::new()
                    };
                    for p in paths {
                        sel.insert(p);
                    }
                    l.set_selection(sel.clone());
                    state.desktop_selection = sel;
                }
            }
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_rubber_band(None);
            }
        }
    }

    // H50.2b.drag.3: mouseup com drag ativo → snap pra célula mais
    // próxima + persiste. Drag sem active = só foi um click normal
    // (já tratado no Pressed), só limpa o pending.
    if button_state == ButtonState::Released {
        if let Some(drag) = state.desktop_drag.take() {
            if drag.active {
                let layer = state.desktop_icons.as_ref().cloned();
                if let Some(l) = layer.as_ref() {
                    l.set_drag(None);
                }
                if let Some(l) = layer.as_ref() {
                    // Top-left da célula = pointer - offset.
                    let target = smithay::utils::Point::<f64, smithay::utils::Logical>::from((
                        pointer_pos.x - drag.offset.x as f64,
                        pointer_pos.y - drag.offset.y as f64,
                    ));
                    let (col, row) = l.nearest_cell(target);
                    // Usa basename do path como chave.
                    if let Some(name) = drag
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                    {
                        // Se outro ícone já tem essa célula, swap.
                        let mut positions = state.desktop_positions.clone();
                        let mut to_evict: Option<String> = None;
                        for (k, v) in positions.iter() {
                            if k != name && *v == (col, row) {
                                to_evict = Some(k.clone());
                                break;
                            }
                        }
                        if let Some(evict_name) = to_evict {
                            // Swap: o vizinho herda a posição antiga
                            // do dragged (se tinha).
                            if let Some(prev) = positions.get(name).copied() {
                                positions.insert(evict_name, prev);
                            } else {
                                positions.remove(&evict_name);
                            }
                        }
                        positions.insert(name.to_string(), (col, row));
                        state.desktop_positions = positions.clone();
                        l.set_positions(positions.clone());
                        crate::desktop_icons::save_positions(&positions);
                        tracing::info!(
                            "desktop: moveu {:?} pra ({col}, {row})",
                            drag.path
                        );
                    }
                }
            }
        }
    }

    // Personalização contextual: click no widget context menu.
    if button_state == ButtonState::Pressed && state.widget_context_menu.is_some() {
        let wcm = state.widget_context_menu.as_ref().unwrap();
        if let Some(hover_idx) = wcm.hovered {
            let action_id = match &wcm.items[hover_idx] {
                luma_widget::ContextMenuItem::Action { action_id, enabled: true, .. } =>
                    Some(action_id.clone()),
                _ => None,
            };
            let widget_idx = wcm.widget_index;
            let is_desktop = wcm.is_desktop;
            state.widget_context_menu = None;
            if let Some(aid) = action_id {
                match aid.as_str() {
                    "move_left" if !is_desktop && widget_idx > 0 => {
                        state.widgets.swap(widget_idx, widget_idx - 1);
                        state.widget_last_ticks.swap(widget_idx, widget_idx - 1);
                    }
                    "move_right" if !is_desktop && widget_idx + 1 < state.widgets.len() => {
                        state.widgets.swap(widget_idx, widget_idx + 1);
                        state.widget_last_ticks.swap(widget_idx, widget_idx + 1);
                    }
                    "remove" => {
                        state.widget_remove_modal = Some(crate::state::WidgetRemoveModal {
                            widget_index: widget_idx,
                            is_desktop,
                            widget_name: "Widget".to_string(),
                        });
                    }
                    _ => {}
                }
            }
        } else {
            // Click fora do menu: fecha.
            state.widget_context_menu = None;
        }
        pointer.button(state, &ButtonEvent {
            button, state: button_state, serial, time: event.time_msec(),
        });
        pointer.frame(state);
        return;
    }

    if button_state == ButtonState::Pressed && !pointer.is_grabbed() {
        // H58: locked intercepta tudo. Primeiro click revela o auth
        // panel; click depois cai em no-op (campo de senha não recebe
        // input por click ainda — fatia 5/6 trabalha melhor).
        if state.locked {
            if !state.lock_revealed {
                state.lock_revealed = true;
                crate::auth::wipe_string(&mut state.lock_password);
            }
            pointer.button(state, &ButtonEvent {
                button, state: button_state, serial, time: event.time_msec(),
            });
            pointer.frame(state);
            return;
        }
        // Personalização contextual: right-click num widget do painel.
        // PRIORIDADE ALTA — antes de qualquer outro handler de taskbar.
        if button == 0x111 && !state.widget_rects.is_empty() {
            let widget_hit: Option<usize> = state
                .widget_rects
                .iter()
                .position(|r| r.to_f64().contains(pointer_pos));
            if let Some(idx) = widget_hit {
                let n = state.widgets.len();
                let mut items: Vec<luma_widget::ContextMenuItem> = Vec::new();
                let custom = state.widgets[idx].context_menu_items();
                if !custom.is_empty() {
                    items.extend(custom);
                    items.push(luma_widget::ContextMenuItem::Separator);
                }
                if idx > 0 {
                    items.push(luma_widget::ContextMenuItem::action("Mover pra esquerda", "move_left"));
                } else {
                    items.push(luma_widget::ContextMenuItem::disabled("Mover pra esquerda", "move_left"));
                }
                if idx + 1 < n {
                    items.push(luma_widget::ContextMenuItem::action("Mover pra direita", "move_right"));
                } else {
                    items.push(luma_widget::ContextMenuItem::disabled("Mover pra direita", "move_right"));
                }
                items.push(luma_widget::ContextMenuItem::Separator);
                items.push(luma_widget::ContextMenuItem::destructive("Remover widget", "remove"));
                let t_wm = crate::theme::current().widget_menu.clone();
                let est_h = t_wm.container_padding * 2 + items.len() as i32 * t_wm.item_height;
                let click_y = pointer_pos.y.round() as i32;
                let out_h = state.space.output_geometry(state.primary_output())
                    .map(|g| g.size.h).unwrap_or(800);
                let menu_y = if click_y + est_h > out_h { (click_y - est_h).max(0) } else { click_y };
                state.widget_context_menu = Some(crate::state::WidgetContextMenu {
                    widget_index: idx, is_desktop: false, items,
                    origin: smithay::utils::Point::from((pointer_pos.x.round() as i32, menu_y)),
                    hovered: None,
                });
                pointer.button(state, &ButtonEvent {
                    button, state: button_state, serial, time: event.time_msec(),
                });
                pointer.frame(state);
                return;
            }
        }
        // H54.P5: central de notificações — bell, panel toggle, DND,
        // clear all, item dismiss. Ordem de prioridade.
        let bell_hit = state.notification_bell_rect.map_or(false, |(x, y, w, h)| {
            pointer_pos.x >= x as f64
                && pointer_pos.x < (x + w) as f64
                && pointer_pos.y >= y as f64
                && pointer_pos.y < (y + h) as f64
        });
        if bell_hit {
            let now_open = match state.notifications.lock() {
                Ok(g) => g.panel_open,
                Err(_) => false,
            };
            if let Ok(mut g) = state.notifications.lock() {
                if now_open {
                    crate::notifications::close_panel(&mut g);
                } else {
                    crate::notifications::open_panel(&mut g);
                }
            }
            pointer.button(state, &ButtonEvent {
                button, state: button_state, serial, time: event.time_msec(),
            });
            pointer.frame(state);
            return;
        }
        // Painel aberto: testa rects internos primeiro.
        if state.notification_panel_rect.is_some() {
            // Dismiss item.
            let mut dismiss_id: Option<u32> = None;
            for (id, x, y, w, h) in state.notification_history_dismiss_rects.iter() {
                if pointer_pos.x >= *x as f64
                    && pointer_pos.x < (*x + *w) as f64
                    && pointer_pos.y >= *y as f64
                    && pointer_pos.y < (*y + *h) as f64
                {
                    dismiss_id = Some(*id);
                    break;
                }
            }
            if let Some(id) = dismiss_id {
                if let Ok(mut g) = state.notifications.lock() {
                    crate::notifications::dismiss_history_item(&mut g, id);
                }
                pointer.button(state, &ButtonEvent {
                    button, state: button_state, serial, time: event.time_msec(),
                });
                pointer.frame(state);
                return;
            }
            // DND toggle.
            let dnd_hit = state.notification_dnd_rect.map_or(false, |(x, y, w, h)| {
                pointer_pos.x >= x as f64
                    && pointer_pos.x < (x + w) as f64
                    && pointer_pos.y >= y as f64
                    && pointer_pos.y < (y + h) as f64
            });
            if dnd_hit {
                if let Ok(mut g) = state.notifications.lock() {
                    crate::notifications::toggle_dnd(&mut g);
                }
                pointer.button(state, &ButtonEvent {
                    button, state: button_state, serial, time: event.time_msec(),
                });
                pointer.frame(state);
                return;
            }
            // H54.P6: Sound toggle.
            let snd_hit = state.notification_sound_rect.map_or(false, |(x, y, w, h)| {
                pointer_pos.x >= x as f64
                    && pointer_pos.x < (x + w) as f64
                    && pointer_pos.y >= y as f64
                    && pointer_pos.y < (y + h) as f64
            });
            if snd_hit {
                if let Ok(mut g) = state.notifications.lock() {
                    crate::notifications::toggle_sound(&mut g);
                }
                pointer.button(state, &ButtonEvent {
                    button, state: button_state, serial, time: event.time_msec(),
                });
                pointer.frame(state);
                return;
            }
            // Clear all.
            let clear_hit = state.notification_clear_rect.map_or(false, |(x, y, w, h)| {
                pointer_pos.x >= x as f64
                    && pointer_pos.x < (x + w) as f64
                    && pointer_pos.y >= y as f64
                    && pointer_pos.y < (y + h) as f64
            });
            if clear_hit {
                if let Ok(mut g) = state.notifications.lock() {
                    crate::notifications::clear_history(&mut g);
                }
                pointer.button(state, &ButtonEvent {
                    button, state: button_state, serial, time: event.time_msec(),
                });
                pointer.frame(state);
                return;
            }
            // Click dentro do painel mas fora de qualquer botão: consome.
            let inside_panel = state.notification_panel_rect
                .map_or(false, |(x, y, w, h)| {
                    pointer_pos.x >= x as f64
                        && pointer_pos.x < (x + w) as f64
                        && pointer_pos.y >= y as f64
                        && pointer_pos.y < (y + h) as f64
                });
            if inside_panel {
                pointer.button(state, &ButtonEvent {
                    button, state: button_state, serial, time: event.time_msec(),
                });
                pointer.frame(state);
                return;
            }
            // Click fora do painel: fecha.
            if let Ok(mut g) = state.notifications.lock() {
                crate::notifications::close_panel(&mut g);
            }
            pointer.button(state, &ButtonEvent {
                button, state: button_state, serial, time: event.time_msec(),
            });
            pointer.frame(state);
            return;
        }

        // H54.P4: action buttons têm prioridade sobre o body.
        // Primeiro testa contra notification_button_rects; se hit,
        // emite ActionInvoked + fecha (a menos que resident=true).
        let mut button_hit: Option<(u32, String)> = None;
        for (id, key, bx, by, bw, bh) in state.notification_button_rects.iter() {
            let inside = pointer_pos.x >= *bx as f64
                && pointer_pos.x < (*bx + *bw) as f64
                && pointer_pos.y >= *by as f64
                && pointer_pos.y < (*by + *bh) as f64;
            if inside {
                button_hit = Some((*id, key.clone()));
                break;
            }
        }
        if let Some((id, key)) = button_hit {
            crate::notifications::emit_action_invoked(
                &state.notifications_conn,
                id,
                &key,
            );
            // Fecha a menos que seja resident.
            let mut should_close = true;
            if let Ok(g) = state.notifications.lock() {
                if let Some(e) = g.visible.iter().find(|e| e.id == id) {
                    if e.resident {
                        should_close = false;
                    }
                }
            }
            if should_close {
                if let Ok(mut g) = state.notifications.lock() {
                    crate::notifications::close_by_reason(&mut g, id, 2);
                }
            }
            pointer.button(
                state,
                &ButtonEvent {
                    button,
                    state: button_state,
                    serial,
                    time: event.time_msec(),
                },
            );
            pointer.frame(state);
            return;
        }
        // H54 P3+P4: click no body. Se a notif tem default_action,
        // emite ActionInvoked("default") + fecha; senão só dispensa.
        let mut hit_id: Option<u32> = None;
        for (id, nx, ny, nw, nh) in state.notification_rects.iter() {
            let inside = pointer_pos.x >= *nx as f64
                && pointer_pos.x < (*nx + *nw) as f64
                && pointer_pos.y >= *ny as f64
                && pointer_pos.y < (*ny + *nh) as f64;
            if inside {
                hit_id = Some(*id);
                break;
            }
        }
        if let Some(id) = hit_id {
            let mut default_key: Option<String> = None;
            let mut resident = false;
            if let Ok(g) = state.notifications.lock() {
                if let Some(e) = g.visible.iter().find(|e| e.id == id) {
                    default_key = e.default_action.clone();
                    resident = e.resident;
                }
            }
            if let Some(key) = default_key.as_ref() {
                crate::notifications::emit_action_invoked(
                    &state.notifications_conn,
                    id,
                    key,
                );
            }
            if !resident {
                if let Ok(mut g) = state.notifications.lock() {
                    crate::notifications::close_by_reason(&mut g, id, 2);
                }
            }
            pointer.button(
                state,
                &ButtonEvent {
                    button,
                    state: button_state,
                    serial,
                    time: event.time_msec(),
                },
            );
            pointer.frame(state);
            return;
        }

        // H53.rename.6: click com rename inline aberto = commit
        // (mesmo padrão que Enter). Comporta-se igual KDE/GNOME.
        // Único caso onde não commita: nada a fazer aqui — o click
        // se propaga normal pelo restante do handler (pode abrir
        // outro menu, selecionar outro ícone, etc).
        if state.desktop_renaming.is_some() {
            if let Some(r) = state.desktop_renaming.take() {
                crate::input::keyboard::commit_rename(state, r);
            }
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_renaming(None);
            }
            // Não return — deixa o click prosseguir pra hit-tests
            // normais (rubber band, menu, seleção, etc).
        }

        // H48.ctx.1: context menu aberto tem prioridade absoluta.
        // Click dentro do menu/submenu = ação; click fora = fecha.
        if state.context_menu.is_some() {
            let output_size = state
                .space
                .output_geometry(state.output_under_cursor(pointer_pos))
                .map(|g| g.size);
            if let Some(output) = output_size {
                let tokens = crate::theme::current().context_menu.clone();
                let menu_snapshot = state.context_menu.clone();
                let inside_main = menu_snapshot
                    .as_ref()
                    .map(|m| m.rect(&tokens).to_f64().contains(pointer_pos))
                    .unwrap_or(false);
                let inside_sub = menu_snapshot
                    .as_ref()
                    .and_then(|m| m.submenu_rect(&tokens, output))
                    .map(|r| r.to_f64().contains(pointer_pos))
                    .unwrap_or(false);
                if !inside_main && !inside_sub {
                    state.context_menu = None;
                } else {
                    // Dispatch ação se hit num item habilitado. ctx.5-7
                    // ligam as ações de fato — por enquanto só fecha.
                    if let Some(menu) = menu_snapshot.as_ref() {
                        if inside_sub {
                            if let Some(sub_idx) =
                                menu.submenu_hit_test(&tokens, output, pointer_pos)
                            {
                                if let Some(parent) = menu
                                    .open_submenu
                                    .and_then(|i| menu.items.get(i))
                                {
                                    if let crate::context_menu::ItemKind::Submenu(entries) =
                                        &parent.kind
                                    {
                                        if let Some(sub) = entries.get(sub_idx) {
                                            if sub.interactive() {
                                                dispatch_context_action(state, sub);
                                                state.context_menu = None;
                                            }
                                        }
                                    }
                                }
                            }
                        } else if let Some(idx) = menu.hit_test(&tokens, pointer_pos) {
                            if let Some(item) = menu.items.get(idx) {
                                if item.interactive() {
                                    dispatch_context_action(state, item);
                                    state.context_menu = None;
                                }
                            }
                        }
                    }
                    pointer.button(
                        state,
                        &ButtonEvent {
                            button,
                            state: button_state,
                            serial,
                            time: event.time_msec(),
                        },
                    );
                    return;
                }
            }
        }

        // H46.media.6: media OSD intercepta click SÓ se cair em um dos
        // 3 botões de controle (prev/play/next). Resto da área do card
        // é click-through — passa pra baixo, honrando "no click block".
        if state.osd_media.is_some() {
            let output_size = state
                .space
                .output_geometry(state.output_under_cursor(pointer_pos))
                .map(|g| g.size);
            if let Some(output) = output_size {
                let t_media = crate::theme::current().osd_media.clone();
                let layout = crate::osd::MediaOsdLayout::compute(output.w, output.h, &t_media);
                if let Some(btn) =
                    layout.hit_test_button(pointer_pos.x, pointer_pos.y)
                {
                    match btn {
                        crate::osd::MediaButton::Previous => {
                            crate::media::previous();
                        }
                        crate::osd::MediaButton::PlayPause => {
                            crate::media::play_pause();
                        }
                        crate::osd::MediaButton::Next => {
                            crate::media::next();
                        }
                    }
                    // Re-fetch + reset timer pra refletir o novo estado
                    // sem esperar o próximo poll de 500ms.
                    let info = crate::media::read_info();
                    if let Some(m) = state.osd_media.as_mut() {
                        m.info = info;
                        m.last_poll = std::time::Instant::now();
                        m.touch();
                    }
                    // Consume — não dispatcha pra surface debaixo.
                    pointer.button(
                        state,
                        &ButtonEvent {
                            button,
                            state: button_state,
                            serial,
                            time: event.time_msec(),
                        },
                    );
                    return;
                }
                // Click fora dos botões: NÃO consome (passa pra baixo),
                // NÃO fecha o OSD (timer dele resolve).
            }
        }
        // H47a.tray.2: setinha de overflow + painel. Click na setinha
        // toggla; click DENTRO do painel é consumido (interações virão
        // na H47b); click FORA da setinha+painel fecha.
        {
            let output_size = state
                .space
                .output_geometry(state.output_under_cursor(pointer_pos))
                .map(|g| g.size);
            if let Some(output) = output_size {
                let arrow_zone = crate::taskbar::tray_overflow_zone(output, &state.tray);
                let arrow_hit = arrow_zone
                    .map(|r| r.to_f64().contains(pointer_pos))
                    .unwrap_or(false);
                if arrow_hit {
                    state.tray.overflow_open = !state.tray.overflow_open;
                    pointer.button(
                        state,
                        &ButtonEvent {
                            button,
                            state: button_state,
                            serial,
                            time: event.time_msec(),
                        },
                    );
                    return;
                }
                if state.tray.overflow_open {
                    let panel_zone = crate::taskbar::tray_overflow_panel_zone(
                        output, &state.tray,
                    );
                    let inside_panel = panel_zone
                        .map(|r| r.to_f64().contains(pointer_pos))
                        .unwrap_or(false);
                    if !inside_panel {
                        // Click fora: fecha o painel, sem consumir o click
                        // (deixa propagar pra surface debaixo).
                        state.tray.overflow_open = false;
                    } else {
                        // H47b.tray.12-13: click num item dispara Activate
                        // (left) ou ContextMenu (right). Left = 0x110,
                        // Right = 0x111.
                        let sni_snapshot: Vec<crate::sni::SniItem> = state
                            .sni_list
                            .lock()
                            .map(|g| g.clone())
                            .unwrap_or_default();
                        let hit = crate::taskbar::tray_sni_button_hit(
                            output, &state.tray, sni_snapshot.len(), pointer_pos,
                        );
                        if let Some(idx) = hit {
                            if let Some(item) = sni_snapshot.get(idx) {
                                let cx = pointer_pos.x.round() as i32;
                                let cy = pointer_pos.y.round() as i32;
                                if button == 0x111 {
                                    // H47c.fix: query DBusMenu ASYNC pra
                                    // não bloquear o main loop. Cria
                                    // pending_dbus_menu; render polla
                                    // o slot cada frame e abre o menu
                                    // quando chegar. Timeout 3s. Sem
                                    // menu_path = delegate imediato.
                                    if item.menu_path.is_empty() {
                                        crate::sni::context_menu(
                                            &item.service, &item.object_path, cx, cy,
                                        );
                                    } else {
                                        let result = crate::sni::get_menu_layout_async(
                                            &item.service,
                                            &item.menu_path,
                                        );
                                        state.pending_dbus_menu =
                                            Some(crate::sni::PendingDBusMenu {
                                                service: item.service.clone(),
                                                menu_path: item.menu_path.clone(),
                                                cursor_x: cx,
                                                cursor_y: cy,
                                                started_at: std::time::Instant::now(),
                                                result,
                                            });
                                        // Fecha o painel do tray.
                                        state.tray.overflow_open = false;
                                    }
                                } else {
                                    crate::sni::activate(
                                        &item.service, &item.object_path, cx, cy,
                                    );
                                }
                            }
                        }
                        // Click dentro do painel sempre consome — sem
                        // botão atingido = sem ação, mas painel não
                        // fecha (igual menu lateral).
                        pointer.button(
                            state,
                            &ButtonEvent {
                                button,
                                state: button_state,
                                serial,
                                time: event.time_msec(),
                            },
                        );
                        return;
                    }
                }
            }
        }
        // H55: click no relógio toggle do calendário. Roda ANTES do
        // popup-outside handler pra evitar o "fechou + reabriu" bug
        // (click fora fecharia, depois esse handler reabriria).
        if let Some(clock_zone) = state.tray.last_clock_rect {
            if clock_zone.to_f64().contains(pointer_pos) {
                let was_calendar = state.applet_popup.as_ref().map(|p| p.kind)
                    == Some(crate::popup::AppletKind::Calendar);
                if was_calendar {
                    state.applet_popup = None;
                } else {
                    // Anchor = borda direita do relógio. Popup cresce
                    // pra esquerda.
                    let anchor_right = clock_zone.loc.x + clock_zone.size.w;
                    state.applet_popup = Some(crate::popup::AppletPopup::new(
                        crate::popup::AppletKind::Calendar,
                        anchor_right,
                    ));
                }
                tracing::info!(
                    "calendar popup={}",
                    state.applet_popup.is_some()
                );
                pointer.button(
                    state,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(state);
                return;
            }
        }
        // H44.volume.1-2: popup de applet tem prioridade. Click DENTRO
        // do popup é consumido pelo popup (e pode interagir com tabs);
        // click FORA fecha.
        if state.applet_popup.is_some() {
            let output_size = state
                .space
                .output_geometry(state.output_under_cursor(pointer_pos))
                .map(|g| g.size);
            if let Some(output) = output_size {
                let popup_clone = state.applet_popup.clone();
                let inside = popup_clone
                    .as_ref()
                    .map(|p| p.contains(output, pointer_pos))
                    .unwrap_or(false);
                if !inside {
                    state.applet_popup = None;
                } else {
                    // H55: dispatch interno do calendário (setas
                    // anterior/próximo, botão Hoje). Só ativo quando
                    // popup é Calendar — rects podem estar stale do
                    // popup anterior, gate por kind impede aleatório.
                    let is_calendar = popup_clone
                        .as_ref()
                        .map(|p| p.kind == crate::popup::AppletKind::Calendar)
                        .unwrap_or(false);
                    if is_calendar {
                        let prev_hit = state.calendar_prev_rect
                            .map_or(false, |r| r.to_f64().contains(pointer_pos));
                        let next_hit = state.calendar_next_rect
                            .map_or(false, |r| r.to_f64().contains(pointer_pos));
                        let hoje_hit = state.calendar_hoje_rect
                            .map_or(false, |r| r.to_f64().contains(pointer_pos));
                        if prev_hit || next_hit || hoje_hit {
                            if let Some(p) = state.applet_popup.as_mut() {
                                if prev_hit {
                                    p.prev_month();
                                } else if next_hit {
                                    p.next_month();
                                } else {
                                    p.reset_to_today();
                                }
                            }
                            pointer.button(state, &ButtonEvent {
                                button, state: button_state, serial,
                                time: event.time_msec(),
                            });
                            pointer.frame(state);
                            return;
                        }
                        // Click dentro do popup Calendar fora dos
                        // botões: consome (não cai no dispatch de
                        // volume).
                        pointer.button(state, &ButtonEvent {
                            button, state: button_state, serial,
                            time: event.time_msec(),
                        });
                        pointer.frame(state);
                        return;
                    }
                    // H57.3: dispatch interno do Bluetooth popup.
                    let is_bt = popup_clone
                        .as_ref()
                        .map(|p| p.kind == crate::popup::AppletKind::Bluetooth)
                        .unwrap_or(false);
                    if is_bt {
                        // 1. Toggle do header.
                        if state.bt_toggle_rect
                            .map_or(false, |r| r.to_f64().contains(pointer_pos))
                        {
                            let new_state = !state.bt_state.enabled;
                            crate::bluetooth::set_enabled(new_state);
                            state.bt_state.enabled = new_state;
                            if !new_state {
                                state.bt_state.devices.clear();
                                state.bt_pair_mode = false;
                            }
                            pointer.button(state, &ButtonEvent {
                                button, state: button_state, serial,
                                time: event.time_msec(),
                            });
                            pointer.frame(state);
                            return;
                        }
                        // 2. Botão "Parear novo dispositivo" → entra
                        // no pair mode + scan on.
                        if state.bt_pair_btn_rect
                            .map_or(false, |r| r.to_f64().contains(pointer_pos))
                        {
                            state.bt_pair_mode = true;
                            crate::bluetooth::scan_on();
                            state.bt_state = crate::bluetooth::read_state(true);
                            state.bt_last_refresh = Some(std::time::Instant::now());
                            pointer.button(state, &ButtonEvent {
                                button, state: button_state, serial,
                                time: event.time_msec(),
                            });
                            pointer.frame(state);
                            return;
                        }
                        // 3. Botão "Voltar" → sai do pair mode + scan off.
                        if state.bt_back_btn_rect
                            .map_or(false, |r| r.to_f64().contains(pointer_pos))
                        {
                            crate::bluetooth::scan_off();
                            state.bt_pair_mode = false;
                            state.bt_state = crate::bluetooth::read_state(false);
                            state.bt_last_refresh = Some(std::time::Instant::now());
                            pointer.button(state, &ButtonEvent {
                                button, state: button_state, serial,
                                time: event.time_msec(),
                            });
                            pointer.frame(state);
                            return;
                        }
                        // 4. Click numa row de device.
                        let row_hit: Option<String> = state
                            .bt_row_rects
                            .iter()
                            .find(|(_, r)| r.to_f64().contains(pointer_pos))
                            .map(|(m, _)| m.clone());
                        if let Some(mac) = row_hit {
                            let dev = state
                                .bt_state
                                .devices
                                .iter()
                                .find(|d| d.mac == mac)
                                .cloned();
                            if let Some(dev) = dev {
                                state.bt_op_in_flight = Some(mac.clone());
                                state.bt_error_for = None;
                                if !dev.paired {
                                    // Just Works pair.
                                    crate::bluetooth::pair_async(
                                        state.bt_op_outbox.clone(), mac,
                                    );
                                } else if dev.connected {
                                    crate::bluetooth::disconnect_async(
                                        state.bt_op_outbox.clone(), mac,
                                    );
                                } else {
                                    crate::bluetooth::connect_async(
                                        state.bt_op_outbox.clone(), mac,
                                    );
                                }
                            }
                            pointer.button(state, &ButtonEvent {
                                button, state: button_state, serial,
                                time: event.time_msec(),
                            });
                            pointer.frame(state);
                            return;
                        }
                        // Click dentro do popup BT fora dos hits: consome.
                        pointer.button(state, &ButtonEvent {
                            button, state: button_state, serial,
                            time: event.time_msec(),
                        });
                        pointer.frame(state);
                        return;
                    }
                    // H57.2: dispatch interno do Wifi popup.
                    let is_wifi = popup_clone
                        .as_ref()
                        .map(|p| p.kind == crate::popup::AppletKind::Wifi)
                        .unwrap_or(false);
                    if is_wifi {
                        // 1. Toggle do header (chave mestra).
                        if state.wifi_toggle_rect
                            .map_or(false, |r| r.to_f64().contains(pointer_pos))
                        {
                            let new_state = !state.wifi_state.enabled;
                            crate::wifi::set_enabled(new_state);
                            state.wifi_state.enabled = new_state;
                            // Recarrega lista vazia se desligou.
                            if !new_state {
                                state.wifi_state.networks.clear();
                                state.wifi_password_for = None;
                            }
                            pointer.button(state, &ButtonEvent {
                                button, state: button_state, serial,
                                time: event.time_msec(),
                            });
                            pointer.frame(state);
                            return;
                        }
                        // 2. Botão Conectar (modo password expandido).
                        if state.wifi_connect_btn_rect
                            .map_or(false, |r| r.to_f64().contains(pointer_pos))
                        {
                            if let Some(ssid) = state.wifi_password_for.clone() {
                                let pw = state.wifi_password_text.clone();
                                state.wifi_connecting = Some(ssid.clone());
                                state.wifi_error_for = None;
                                crate::wifi::connect_async(
                                    state.wifi_connect_outbox.clone(),
                                    ssid, Some(pw),
                                );
                            }
                            pointer.button(state, &ButtonEvent {
                                button, state: button_state, serial,
                                time: event.time_msec(),
                            });
                            pointer.frame(state);
                            return;
                        }
                        // 3. Click numa row de rede.
                        let row_hit: Option<String> = state
                            .wifi_row_rects
                            .iter()
                            .find(|(_, r)| r.to_f64().contains(pointer_pos))
                            .map(|(s, _)| s.clone());
                        if let Some(ssid) = row_hit {
                            // Acha o net na lista.
                            let net = state
                                .wifi_state
                                .networks
                                .iter()
                                .find(|n| n.ssid == ssid)
                                .cloned();
                            if let Some(net) = net {
                                if net.in_use {
                                    // Click na rede já conectada: no-op
                                    // (no futuro mostra ações como
                                    // desconectar; fora de escopo H57).
                                } else if !net.is_protected() {
                                    state.wifi_connecting = Some(ssid.clone());
                                    state.wifi_error_for = None;
                                    crate::wifi::connect_async(
                                        state.wifi_connect_outbox.clone(),
                                        ssid, None,
                                    );
                                } else {
                                    // Tenta connect com perfil salvo
                                    // primeiro. Se falhar, expande
                                    // password inline. Por simplicidade
                                    // pra v1: sempre expande password
                                    // (perfis salvos antigos não detec
                                    // perfeito sem listar connections).
                                    if state.wifi_password_for.as_deref() == Some(&ssid) {
                                        // Mesma row clicada de novo:
                                        // colapsa.
                                        state.wifi_password_for = None;
                                        state.wifi_password_text.clear();
                                        state.wifi_password_cursor = 0;
                                    } else {
                                        state.wifi_password_for = Some(ssid);
                                        state.wifi_password_text.clear();
                                        state.wifi_password_cursor = 0;
                                        state.wifi_error_for = None;
                                    }
                                }
                            }
                            pointer.button(state, &ButtonEvent {
                                button, state: button_state, serial,
                                time: event.time_msec(),
                            });
                            pointer.frame(state);
                            return;
                        }
                        // Click dentro do popup Wifi fora dos hits:
                        // consome.
                        pointer.button(state, &ButtonEvent {
                            button, state: button_state, serial,
                            time: event.time_msec(),
                        });
                        pointer.frame(state);
                        return;
                    }
                    // H57: dispatch interno do battery — slider de modo.
                    let is_battery = popup_clone
                        .as_ref()
                        .map(|p| p.kind == crate::popup::AppletKind::Battery)
                        .unwrap_or(false);
                    if is_battery {
                        // Hit-test nas 3 zonas do slider.
                        let zones = state.battery_mode_zones.clone();
                        let mut profile_hit: Option<String> = None;
                        for (name, rect) in zones.iter() {
                            if rect.to_f64().contains(pointer_pos) {
                                profile_hit = Some(name.clone());
                                break;
                            }
                        }
                        if let Some(name) = profile_hit {
                            // PPD set + refresh snapshot. Falha silent.
                            if crate::power::set_active(&name) {
                                state.power_profiles = crate::power::read_state();
                            }
                        }
                        // Click dentro do popup Battery: consome (fora
                        // dos hits cai no consume genérico abaixo).
                        pointer.button(state, &ButtonEvent {
                            button, state: button_state, serial,
                            time: event.time_msec(),
                        });
                        pointer.frame(state);
                        return;
                    }
                    // H44.volume.2: tab click muda aba ativa.
                    if let Some(tab) = popup_clone
                        .as_ref()
                        .and_then(|p| p.tab_hit(output, pointer_pos))
                    {
                        if let Some(p) = state.applet_popup.as_mut() {
                            p.volume_tab = tab;
                        }
                    }
                    // H44.volume.15: toggle de boost (canto inferior).
                    if let Some(popup) = popup_clone.as_ref() {
                        let toggle = popup.boost_toggle_rect(output);
                        if toggle.to_f64().contains(pointer_pos) {
                            if let Some(p) = state.applet_popup.as_mut() {
                                p.boost = !p.boost;
                            }
                            // Consume o click — não cai no dispatch de devices.
                            pointer.button(
                                state,
                                &ButtonEvent {
                                    button,
                                    state: button_state,
                                    serial,
                                    time: event.time_msec(),
                                },
                            );
                            pointer.frame(state);
                            return;
                        }
                    }
                    // H44.volume.6-13: dispatch dentro do popup.
                    if let Some(popup) = popup_clone.as_ref() {
                        match popup.volume_tab {
                            crate::popup::VolumeTab::Devices => {
                                let n_sinks = state.audio_sinks.len();
                                let n_sources = state.audio_sources.len();
                                if let Some(hit) = crate::popup::hit_test_devices_full(
                                    popup, output, n_sinks, n_sources, pointer_pos,
                                ) {
                                    let id = match hit.list {
                                        crate::popup::DeviceList::Sinks => {
                                            state.audio_sinks[hit.idx].id
                                        }
                                        crate::popup::DeviceList::Sources => {
                                            state.audio_sources[hit.idx].id
                                        }
                                    };
                                    match hit.region {
                                        crate::popup::DeviceRegion::Slider(v) => {
                                            crate::audio::set_volume(id, (v * 100.0) as u8);
                                        }
                                        crate::popup::DeviceRegion::MuteIcon => {
                                            crate::audio::toggle_mute(id);
                                        }
                                        crate::popup::DeviceRegion::Label => {
                                            crate::audio::set_default(id);
                                        }
                                    }
                                    let (sinks, sources) =
                                        crate::audio::list_devices();
                                    state.audio_sinks = sinks;
                                    state.audio_sources = sources;
                                }
                            }
                            crate::popup::VolumeTab::Applications => {
                                let n = state.audio_streams.len();
                                if let Some(hit) = crate::popup::hit_test_streams(
                                    popup, output, n, pointer_pos,
                                ) {
                                    let id = state.audio_streams[hit.idx].id;
                                    match hit.region {
                                        crate::popup::DeviceRegion::Slider(v) => {
                                            crate::audio::set_volume(id, (v * 100.0) as u8);
                                        }
                                        crate::popup::DeviceRegion::MuteIcon => {
                                            crate::audio::toggle_mute(id);
                                        }
                                        crate::popup::DeviceRegion::Label => {
                                            // sem set-default pra streams
                                        }
                                    }
                                    state.audio_streams = crate::audio::list_streams();
                                }
                            }
                        }
                    }
                }
                pointer.button(
                    state,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(state);
                return;
            }
        }
        // Taskbar tem prioridade absoluta — fica visualmente sobre as
        // janelas, então clicar nela ativa o foco em vez de cair na janela
        // que está embaixo.
        // Menu aberto: prioridade absoluta — click em item executa, click
        // H32.10.14+15+fix.modal: menu fullscreen híbrido — early
        // hit-test session bar + modal de confirmação.
        if state.menu_open {
            if let Some(output) = {
                let o = state.output_under_cursor(pointer_pos);
                state.space.output_geometry(o).map(|g| g.size)
            } {
                // 1) Se modal ativo, hit-test só nos 2 botões dele.
                if state.menu_new.pending_action.is_some() {
                    match crate::menu::confirm_modal::hit_test_modal(pointer_pos, output) {
                        Some(0) => {
                            // Cancelar.
                            state.menu_new.pending_action = None;
                        }
                        Some(1) => {
                            // Confirmar — executa e fecha. H58: Lock vai
                            // pelo trigger interno, não loginctl.
                            if let Some(action) = state.menu_new.pending_action {
                                if action == crate::menu::state::SessionButton::Lock {
                                    state.locked = true;
                                    state.secure_lock = true;
                                    state.locked_at = Some(std::time::Instant::now());
                                    tracing::info!("luma lock: requested via confirm modal");
                                } else {
                                    crate::menu::session_bar::execute_action(action);
                                }
                            }
                            state.menu_open = false;
                            state.menu_new.reset();
                        }
                        _ => {} // click fora dos botões: ignora (não fecha)
                    }
                    pointer.frame(state);
                    return;
                }

                let sb_origin = state.menu_fullscreen
                    .zone_origin(output, 4);
                let sb_w = output.w
                    - 2 * crate::menu::render::layout::SESSION_MARGIN_HORIZONTAL;
                let sb_h = crate::menu::render::layout::SESSION_H;
                let sb_size = smithay::utils::Size::<i32, smithay::utils::Logical>
                    ::from((sb_w, sb_h));

                if let Some(btn) = crate::menu::session_bar::hit_test_buttons(
                    pointer_pos, sb_origin,
                ) {
                    if crate::menu::confirm_modal::requires_confirmation(btn) {
                        // Abre modal em vez de executar direto.
                        state.menu_new.pending_action = Some(btn);
                    } else if btn == crate::menu::state::SessionButton::Lock {
                        // H58 fatia 2: Lock dispara o lock interno do
                        // Luma — NÃO `loginctl lock-session` que pegaria
                        // a sessão host (KDE) quando rodando aninhado.
                        state.locked = true;
                        state.secure_lock = true;
                        state.locked_at = Some(std::time::Instant::now());
                        state.menu_open = false;
                        state.menu_new.reset();
                        tracing::info!("luma lock: requested via session menu");
                    } else {
                        crate::menu::session_bar::execute_action(btn);
                        state.menu_open = false;
                        state.menu_new.reset();
                    }
                    pointer.frame(state);
                    return;
                }
                if let Some(idx) = crate::menu::session_bar::hit_test_places(
                    pointer_pos, sb_origin, sb_size,
                ) {
                    crate::menu::session_bar::execute_place_action(idx);
                    state.menu_open = false;
                    state.menu_new.reset();
                    pointer.frame(state);
                    return;
                }

                // Click na pill da search bar (zone[1]) foca ela e ativa
                // o cursor "|" piscante. Click em outra zona desliga.
                let s_origin_z = state.menu_fullscreen.zone_origin(output, 1);
                let s_th = &crate::theme::current().menu_search_bar;
                let on_search_bar = pointer_pos.x >= s_origin_z.x as f64
                    && pointer_pos.x < (s_origin_z.x + s_th.width) as f64
                    && pointer_pos.y >= s_origin_z.y as f64
                    && pointer_pos.y < (s_origin_z.y + s_th.height) as f64;
                if on_search_bar {
                    state.menu_new.focused_zone =
                        crate::menu::state::FocusedZone::Search;
                    state.menu_new.search_bar_active = true;
                    pointer.frame(state);
                    return;
                } else {
                    // Click fora da bar desliga o "click-active" mas
                    // mantém active enquanto houver query (pra continuar
                    // editando depois).
                    if state.menu_new.search_query.is_empty() {
                        state.menu_new.search_bar_active = false;
                    }
                }

                // Link zone[3] — toggla mode (Favorites ↔ AllApps).
                // "TODOS OS APPS →" no fav mode; "← FAVORITOS" no all.
                if state.menu_new.search_query.is_empty() {
                    let l_origin = state.menu_fullscreen.zone_origin(output, 3);
                    let l_w = crate::menu::render::layout::ALL_APPS_LINK_W;
                    let l_h = crate::menu::render::layout::ALL_APPS_LINK_H;
                    let lx = l_origin.x as f64;
                    let ly = l_origin.y as f64;
                    if pointer_pos.x >= lx && pointer_pos.x < lx + l_w as f64
                        && pointer_pos.y >= ly && pointer_pos.y < ly + l_h as f64
                    {
                        use crate::menu::state::{FocusedZone, MenuMode};
                        state.menu_new.mode = match state.menu_new.mode {
                            MenuMode::Favorites => MenuMode::AllApps,
                            MenuMode::AllApps => MenuMode::Favorites,
                        };
                        // Volta foco pra Search (não pro grid) — assim
                        // keyboard selected não aparece com glow até
                        // user apertar uma seta. Reseta os cursores
                        // pra ficar coerente.
                        state.menu_new.focused_zone = FocusedZone::Search;
                        state.menu_new.fav_selected = 0;
                        state.menu_new.all_apps_selected = (0, 0);
                        tracing::info!("click link → mode={:?}", state.menu_new.mode);
                        pointer.frame(state);
                        return;
                    }
                }

                // Hit-test do grid central — favoritos, AllApps ou
                // search results conforme o estado.
                let fav_origin = state.menu_fullscreen.zone_origin(output, 2);
                if !state.menu_new.search_query.is_empty() {
                    // Search results: lista vertical.
                    if let Some(app_idx) = hit_test_search_results(
                        fav_origin, pointer_pos, &state.menu_new.search_results,
                    ) {
                        if let Some(app) = state.apps.get(app_idx).cloned() {
                            tracing::info!("click search: {}", app.name);
                            crate::apps::launch(&app.exec);
                            state.menu_open = false;
                            state.menu_new.reset();
                            pointer.frame(state);
                            return;
                        }
                    }
                } else if state.menu_new.mode == crate::menu::state::MenuMode::Favorites {
                    if let Some(slot) = hit_test_favorites(fav_origin, pointer_pos) {
                        if let Some(fav) = state.menu_new.fav_items.get(slot) {
                            // Marca pressed pra feedback visual (vai sumir
                            // após o launch + reset).
                            state.menu_new.fav_pressed = Some(slot);
                            launch_curated_favorite(state, fav.clone(), pointer);
                            return;
                        }
                    }
                } else {
                    let hit = hit_test_all_apps(
                        fav_origin, pointer_pos,
                        state.menu_new.all_apps_page, state.apps.len(),
                    );
                    tracing::info!(
                        "click all_apps: mode={:?} pos={:?} fav_origin={:?} hit={:?}",
                        state.menu_new.mode, pointer_pos, fav_origin, hit,
                    );
                    if let Some(app_idx) = hit {
                        if let Some(app) = state.apps.get(app_idx).cloned() {
                            tracing::info!("click all: {}", app.name);
                            crate::apps::launch(&app.exec);
                            state.menu_open = false;
                            state.menu_new.reset();
                            pointer.frame(state);
                            return;
                        }
                    }
                }
            }
            // Menu fullscreen é input-exclusive: clicks em área "vazia"
            // do backdrop não passam pra janelas atrás. Sem isso o user
            // conseguia interagir com o app embaixo (drag, focus, etc)
            // através do menu — bizarro.
            pointer.frame(state);
            return;
        }


        // H36 Bloco D: overview tem prioridade absoluta quando aberto.
        if state.overview_open {
            let output = state.output_under_cursor(pointer_pos);
            if let Some(out_size) = state.space.output_geometry(output).map(|g| g.size) {
                let layout = crate::overview::thumb_layout(
                    out_size,
                    state.workspaces.count(),
                );
                match crate::overview::hit_test(pointer_pos, &layout) {
                    Some(crate::overview::OverviewHit::Workspace(idx)) => {
                        tracing::info!("overview: switch pra workspace {}", idx + 1);
                        state.switch_workspace(idx);
                    }
                    Some(crate::overview::OverviewHit::NewWorkspace) => {
                        tracing::info!("overview: novo workspace");
                        state.workspaces.create();
                        state.refresh_workspace_visibility();
                    }
                    None => {} // click no backdrop só fecha
                }
                state.overview_open = false;
            }
            pointer.frame(state);
            return;
        }
        // H36.9: click no pager (entre launcher e blocks). Tem prioridade
        // sobre taskbar_hit pra não confundir com Launcher.
        if let Some(idx) = pager_hit(state, pointer_pos) {
            tracing::info!("pager clicado: workspace {}", idx + 1);
            state.switch_workspace(idx);
            pointer.frame(state);
            return;
        }
        // H44.volume.1: click no ícone de volume abre/fecha o popup.
        if let Some(output_size) = state
            .space
            .output_geometry(state.output_under_cursor(pointer_pos))
            .map(|g| g.size)
        {
            if let Some(zone) = crate::taskbar::tray_volume_zone(output_size, &state.tray) {
                if zone.to_f64().contains(pointer_pos) {
                    let was_open = state.applet_popup.is_some()
                        && state.applet_popup.as_ref().unwrap().kind
                            == crate::popup::AppletKind::Volume;
                    if was_open {
                        state.applet_popup = None;
                    } else {
                        // H44.volume.5+12: refresh devices E streams ao abrir.
                        let (sinks, sources) = crate::audio::list_devices();
                        state.audio_sinks = sinks;
                        state.audio_sources = sources;
                        state.audio_streams = crate::audio::list_streams();
                        state.applet_popup = Some(crate::popup::AppletPopup::new(
                            crate::popup::AppletKind::Volume,
                            zone.loc.x + zone.size.w / 2,
                        ));
                    }
                    tracing::info!(
                        "volume popup={} sinks={}",
                        state.applet_popup.is_some(),
                        state.audio_sinks.len()
                    );
                    pointer.frame(state);
                    return;
                }
            }
            // H57.3: click no ícone bluetooth abre/fecha o popup.
            if let Some(zone) = state.tray.last_bt_rect {
                if zone.to_f64().contains(pointer_pos) {
                    let was_open = state.applet_popup.is_some()
                        && state.applet_popup.as_ref().unwrap().kind
                            == crate::popup::AppletKind::Bluetooth;
                    if was_open {
                        // Sai do pair mode se estivesse.
                        if state.bt_pair_mode {
                            crate::bluetooth::scan_off();
                            state.bt_pair_mode = false;
                        }
                        state.applet_popup = None;
                    } else {
                        state.bt_pair_mode = false;
                        state.bt_state = crate::bluetooth::read_state(false);
                        state.bt_last_refresh = Some(std::time::Instant::now());
                        state.bt_error_for = None;
                        state.applet_popup = Some(crate::popup::AppletPopup::new(
                            crate::popup::AppletKind::Bluetooth,
                            zone.loc.x + zone.size.w / 2,
                        ));
                    }
                    pointer.frame(state);
                    return;
                }
            }
            // H57.2: click no ícone wifi abre/fecha o popup.
            if let Some(zone) = state.tray.last_wifi_rect {
                if zone.to_f64().contains(pointer_pos) {
                    let was_open = state.applet_popup.is_some()
                        && state.applet_popup.as_ref().unwrap().kind
                            == crate::popup::AppletKind::Wifi;
                    if was_open {
                        state.applet_popup = None;
                    } else {
                        state.wifi_state = crate::wifi::read_state();
                        state.wifi_last_refresh = Some(std::time::Instant::now());
                        state.wifi_password_for = None;
                        state.wifi_password_text.clear();
                        state.wifi_password_cursor = 0;
                        state.wifi_error_for = None;
                        if state.wifi_state.enabled {
                            crate::wifi::request_scan();
                        }
                        state.applet_popup = Some(crate::popup::AppletPopup::new(
                            crate::popup::AppletKind::Wifi,
                            zone.loc.x + zone.size.w / 2,
                        ));
                    }
                    pointer.frame(state);
                    return;
                }
            }
            // H57: click no bloco da bateria abre/fecha o popup.
            if let Some(zone) = state.tray.last_battery_rect {
                if zone.to_f64().contains(pointer_pos) {
                    let was_open = state.applet_popup.is_some()
                        && state.applet_popup.as_ref().unwrap().kind
                            == crate::popup::AppletKind::Battery;
                    if was_open {
                        state.applet_popup = None;
                    } else {
                        // Refresh do PPD state ao abrir.
                        state.power_profiles = crate::power::read_state();
                        state.applet_popup = Some(crate::popup::AppletPopup::new(
                            crate::popup::AppletKind::Battery,
                            zone.loc.x + zone.size.w / 2,
                        ));
                    }
                    tracing::info!(
                        "battery popup={} ppd={:?}",
                        state.applet_popup.is_some(),
                        state.power_profiles.as_ref().map(|p| p.active.clone()),
                    );
                    pointer.frame(state);
                    return;
                }
            }
        }
        if let Some(hit) = taskbar_hit(state, pointer_pos) {
            // Pra Window(idx), o índice se refere apenas às janelas reais
            // (sem wallpaper) — assim a taskbar mostra só windows.
            match hit {
                crate::taskbar::TaskbarHit::Launcher => {
                    state.menu_open = !state.menu_open;
                    if !state.menu_open {
                        state.menu_new.reset();
                    }
                    tracing::info!("launcher clicado (menu_open={})", state.menu_open);
                }
                crate::taskbar::TaskbarHit::ShowDesktop => {
                    // H33.taskbar.F.17: lógica compartilhada com Super+D.
                    state.toggle_show_desktop();
                }
                crate::taskbar::TaskbarHit::Window(idx) => {
                    // Acha a janela pelo surface registrado na ordem estável
                    // da taskbar, não pelo índice no z-order do Space.
                    let target_surface = state.taskbar.order.get(idx).cloned();
                    let window_opt = target_surface.and_then(|surf| {
                        state
                            .space
                            .elements()
                            .filter_map(|i| i.as_window())
                            .find(|w| {
                                w.window()
                                    .toplevel()
                                    .map(|t| t.wl_surface() == &surf)
                                    .unwrap_or(false)
                            })
                            .cloned()
                    });
                    if let Some(window) = window_opt {
                        crate::decoration::with_decoration(window.window(), |d| {
                            d.minimized = false
                        });
                        state
                            .space
                            .raise_element(&crate::space_item::SpaceItem::Window(window.clone()), true);
                        let surface =
                            window.window().toplevel().unwrap().wl_surface().clone();
                        keyboard.set_focus(state, Some(surface), serial);
                        let toplevels: Vec<_> = state
                            .space
                            .elements()
                            .filter_map(|i| i.as_window())
                            .filter_map(|w| w.window().toplevel().cloned())
                            .collect();
                        for t in toplevels {
                            t.send_pending_configure();
                        }
                    }
                }
            }
            pointer.button(
                state,
                &ButtonEvent {
                    button,
                    state: button_state,
                    serial,
                    time: event.time_msec(),
                },
            );
            pointer.frame(state);
            return;
        }

        // Borda de resize tem prioridade sobre header (canto inferior-direito
        // sobrepõe). Hit-test resize primeiro.
        if let Some((window, edge)) = resize_edge_hit_for(state, pointer_pos) {
            state
                .space
                .raise_element(&crate::space_item::SpaceItem::Window(window.clone()), true);
            let surface = window.window().toplevel().unwrap().wl_surface().clone();
            keyboard.set_focus(state, Some(surface.clone()), serial);

            let initial_size = {
                use smithay::desktop::space::SpaceElement;
                SpaceElement::geometry(&window).size
            };
            let initial_loc = state
                .space
                .element_location(&crate::space_item::SpaceItem::Window(window.clone()))
                .unwrap_or_else(|| smithay::utils::Point::from((0, 0)));
            let start_data = GrabStartData {
                focus: Some((surface, initial_loc.to_f64())),
                button,
                location: pointer_pos,
            };
            let grab = resize_grab::ResizeSurfaceGrab::start(
                start_data,
                window.clone(),
                edge,
                smithay::utils::Rectangle::new(initial_loc, initial_size),
            );
            pointer.set_grab(state, grab, serial, Focus::Clear);
            pointer.frame(state);
            return;
        }

        // Hit-test no header antes do click-to-focus, pra capturar cliques
        // nos botões e iniciar drag-to-move pelo background.
        if let Some((window, region)) = header_hit_for(state, pointer_pos) {
            use crate::decoration::HeaderRegion;
            state
                .space
                .raise_element(&crate::space_item::SpaceItem::Window(window.clone()), true);
            let surface = window.window().toplevel().unwrap().wl_surface().clone();
            keyboard.set_focus(state, Some(surface.clone()), serial);

            match region {
                HeaderRegion::Close => {
                    tracing::info!("click no botão fechar");
                    window.window().toplevel().unwrap().send_close();
                }
                HeaderRegion::Background => {
                    let item = crate::space_item::SpaceItem::Window(window.clone());
                    if let Some(initial) = state.space.element_location(&item) {
                        let start_data = GrabStartData {
                            focus: Some((surface, initial.to_f64())),
                            button,
                            location: pointer_pos,
                        };
                        let grab = MoveSurfaceGrab {
                            start_data,
                            window: window.clone(),
                            initial_window_location: initial,
                            pending_snap: None,
                        };
                        pointer.set_grab(state, grab, serial, Focus::Clear);
                        pointer.frame(state);
                        return;
                    }
                }
                HeaderRegion::Maximize => {
                    tracing::info!("click no botão maximizar");
                    state.toggle_maximize(&window);
                }
                HeaderRegion::Minimize => {
                    tracing::info!("click no botão minimizar");
                    crate::decoration::with_decoration(window.window(), |d| {
                        d.minimized = true
                    });
                    if let Some(kb) = state.seat.get_keyboard() {
                        kb.set_focus(state, None::<WlSurface>, serial);
                    }
                }
            }
            pointer.button(
                state,
                &ButtonEvent {
                    button,
                    state: button_state,
                    serial,
                    time: event.time_msec(),
                },
            );
            pointer.frame(state);
            return;
        }

        // H50.2a.sel.2: cursor sobre uma célula do grid de ícones do
        // desktop? Espaço sob o cursor pode ser o `DesktopIconsLayer`
        // (z=5) quando nenhuma janela cobre essa área. Dispatch
        // seleção e consome o click. Right-click sobre ícone também
        // consume (não dispara context menu do desktop).
        let icon_hit: Option<usize> = state
            .desktop_icons
            .as_ref()
            .and_then(|l| l.hit_test_cell(pointer_pos));
        if let Some(idx) = icon_hit {
            let entries = state
                .desktop_icons
                .as_ref()
                .map(|l| l.entries_snapshot())
                .unwrap_or_default();
            if let Some(entry) = entries.get(idx) {
                if button == 0x110 {
                    // H50.2b.drag.1: registra pending drag (ainda não
                    // ativo, só ativa depois de threshold em motion).
                    let theme = crate::theme::current();
                    let t = &theme.desktop_icons;
                    let (col, row) = {
                        let positions = state
                            .desktop_icons
                            .as_ref()
                            .map(|l| {
                                l.entries_snapshot();
                                state.desktop_positions.clone()
                            })
                            .unwrap_or_default();
                        let rows_per_col = {
                            let output_h = state
                                .space
                                .output_geometry(state.output_under_cursor(pointer_pos))
                                .map(|g| g.size.h)
                                .unwrap_or(0);
                            let usable = output_h
                                - t.grid_margin
                                - crate::taskbar::taskbar_height()
                                - 8;
                            (usable / t.cell_height).max(1)
                        };
                        let layout = crate::desktop_icons::compute_layout(
                            &entries,
                            &positions,
                            rows_per_col,
                        );
                        layout.get(idx).copied().unwrap_or((0, 0))
                    };
                    let cell_x = t.grid_margin + col * t.cell_width;
                    let cell_y = t.grid_margin + row * t.cell_height;
                    let offset = smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                        pointer_pos.x.round() as i32 - cell_x,
                        pointer_pos.y.round() as i32 - cell_y,
                    ));
                    state.desktop_drag = Some(crate::desktop_icons::DesktopDrag {
                        path: entry.path.clone(),
                        start_screen: pointer_pos,
                        offset,
                        current_screen: pointer_pos,
                        active: false,
                    });
                    // H50.2a.sel.4: detecta double-click (<=500ms,
                    // mesmo ícone) → abre via xdg-open. NÃO Ctrl+click.
                    let now = std::time::Instant::now();
                    let mods = state
                        .seat
                        .get_keyboard()
                        .map(|kb| kb.modifier_state());
                    let ctrl = mods.map(|m| m.ctrl).unwrap_or(false);
                    let double_click = !ctrl
                        && state
                            .desktop_last_click
                            .as_ref()
                            .map(|(t, p)| {
                                p == &entry.path
                                    && now.duration_since(*t)
                                        < std::time::Duration::from_millis(500)
                            })
                            .unwrap_or(false);
                    if double_click {
                        tracing::info!("desktop: open {:?}", entry.path);
                        let _ = std::process::Command::new("xdg-open")
                            .arg(&entry.path)
                            .spawn();
                        state.desktop_last_click = None;
                    } else {
                        state.desktop_last_click = Some((now, entry.path.clone()));
                    }
                    // Left click: substitui seleção; Ctrl+click toggla.
                    let mut sel = state
                        .desktop_icons
                        .as_ref()
                        .map(|l| l.selection_snapshot())
                        .unwrap_or_default();
                    if ctrl {
                        if !sel.insert(entry.path.clone()) {
                            sel.remove(&entry.path);
                        }
                    } else {
                        sel.clear();
                        sel.insert(entry.path.clone());
                    }
                    if let Some(l) = state.desktop_icons.as_ref() {
                        l.set_selection(sel.clone());
                    }
                    state.desktop_selection = sel;
                } else if button == 0x111 {
                    // H52: right-click sobre ícone seleciona + abre
                    // o menu per-icon (Abrir / Copiar caminho /
                    // Renomear / Mover pra Lixeira).
                    let mut sel = std::collections::HashSet::new();
                    sel.insert(entry.path.clone());
                    if let Some(l) = state.desktop_icons.as_ref() {
                        l.set_selection(sel.clone());
                    }
                    state.desktop_selection = sel;
                    let output_size = state
                        .space
                        .output_geometry(state.output_under_cursor(pointer_pos))
                        .map(|g| g.size);
                    if let Some(out) = output_size {
                        let tokens = crate::theme::current().context_menu.clone();
                        let mut menu = crate::context_menu::ContextMenu::for_icon(
                            smithay::utils::Point::from((
                                pointer_pos.x.round() as i32,
                                pointer_pos.y.round() as i32,
                            )),
                            entry.path.clone(),
                        );
                        menu.clamp_into(out, &tokens);
                        state.context_menu = Some(menu);
                    }
                }
            }
            pointer.button(
                state,
                &ButtonEvent {
                    button,
                    state: button_state,
                    serial,
                    time: event.time_msec(),
                },
            );
            pointer.frame(state);
            return;
        }

        match state
            .space
            .element_under(pointer_pos)
            .and_then(|(i, l)| i.as_window().map(|w| (w.clone(), l)))
        { Some((window, _loc)) => {
            state
                .space
                .raise_element(&crate::space_item::SpaceItem::Window(window.clone()), true);
            let surface = window.window().toplevel().unwrap().wl_surface().clone();
            keyboard.set_focus(state, Some(surface), serial);
            let toplevels: Vec<_> = state
                .space
                .elements()
                .filter_map(|i| i.as_window())
                .filter_map(|w| w.window().toplevel().cloned())
                .collect();
            for t in toplevels {
                t.send_pending_configure();
            }
        } _ => {
            // H50.2a.sel.3: left click em área vazia do desktop
            // (sem janela e sem ícone sob o cursor) deseleciona tudo.
            if button == 0x110 && !state.desktop_selection.is_empty() {
                state.desktop_selection.clear();
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_selection(std::collections::HashSet::new());
                }
            }
            // H50.2c.rb.1: left mousedown em área vazia inicia
            // rubber-band (caixa de seleção). Motion atualiza,
            // mouseup escolhe os ícones que cruzam o rect.
            // Right-click segue abrindo o context menu (lógica abaixo).
            if button == 0x110 {
                let rb = crate::desktop_icons::RubberBand {
                    start: pointer_pos,
                    current: pointer_pos,
                };
                state.desktop_rubber_band = Some(rb.clone());
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_rubber_band(Some(rb));
                }
            }
            // (widget right-click handler movido pra alta prioridade acima)
            // Personalização contextual: right-click no painel abre
            // menu do painel (estilo Mint). 3c.4 religou Adicionar
            // widget + Personalizar desktop. "Configurar painel"
            // segue Disabled (fora do escopo da Fatia 3).
            if button == 0x111 {
                let output_size = state
                    .space
                    .output_geometry(state.output_under_cursor(pointer_pos))
                    .map(|g| g.size);
                if let Some(out) = output_size {
                    let bar_top = out.h - crate::taskbar::taskbar_height();
                    let on_taskbar = pointer_pos.y >= bar_top as f64;
                    if on_taskbar {
                        let tokens = crate::theme::current().context_menu.clone();
                        let mut menu = crate::context_menu::ContextMenu {
                            origin: smithay::utils::Point::from((
                                pointer_pos.x.round() as i32,
                                pointer_pos.y.round() as i32,
                            )),
                            items: vec![
                                crate::context_menu::ContextItem {
                                    label: "Adicionar widget ao painel".into(),
                                    icon: None,
                                    kind: crate::context_menu::ItemKind::Action(
                                        crate::context_menu::ContextAction::OpenConfigAddPanel,
                                    ),
                                    enabled: true,
                                    shortcut: None,
                                },
                                crate::context_menu::ContextItem {
                                    label: "Configurar painel".into(),
                                    icon: None,
                                    kind: crate::context_menu::ItemKind::Action(
                                        crate::context_menu::ContextAction::Disabled,
                                    ),
                                    enabled: false,
                                    shortcut: None,
                                },
                                crate::context_menu::ContextItem {
                                    label: "".into(),
                                    icon: None,
                                    kind: crate::context_menu::ItemKind::Separator,
                                    enabled: false,
                                    shortcut: None,
                                },
                                crate::context_menu::ContextItem {
                                    label: "Personalizar desktop".into(),
                                    icon: None,
                                    kind: crate::context_menu::ItemKind::Action(
                                        crate::context_menu::ContextAction::OpenConfigCustomize,
                                    ),
                                    enabled: true,
                                    shortcut: None,
                                },
                            ],
                            hovered: None,
                            open_submenu: None,
                            submenu_hovered: None,
                            target: None,
                            dbus_menu: None,
                        };
                        menu.clamp_into(out, &tokens);
                        state.context_menu = Some(menu);
                        pointer.button(state, &ButtonEvent {
                            button, state: button_state, serial, time: event.time_msec(),
                        });
                        pointer.frame(state);
                        return;
                    }
                }
            }
            // H48.ctx.1: right-click no desktop (área vazia, sem
            // janela sob o cursor) abre menu de contexto. MAS não quando o
            // cursor está sobre uma layer surface de cliente (popup/menu da
            // barra) — aí o direito tem que chegar no cliente (menu de tray).
            // Reabre no ponto novo mesmo por cima da surface do próprio ctx
            // menu (DESKTOP_MENU_OPEN), senão is_over_client_layer bloquearia.
            if button == 0x111
                && (!state.is_over_client_layer(pointer_pos)
                    || crate::state::DESKTOP_MENU_OPEN
                        .load(std::sync::atomic::Ordering::Relaxed))
            {
                let output_size = state
                    .space
                    .output_geometry(state.output_under_cursor(pointer_pos))
                    .map(|g| g.size);
                if let Some(out) = output_size {
                    let bar_top = out.h - crate::taskbar::taskbar_height();
                    let on_taskbar = pointer_pos.y >= bar_top as f64;
                    if !on_taskbar {
                        // Context menu do desktop agora é CLIENTE (luma-menu):
                        // emite o sinal com a posição do clique + tamanho do
                        // output (pro cliente ancorar no ponto + proteger a
                        // borda). O menu velho do compositor segue dormente.
                        let x = pointer_pos.x.round() as i32;
                        let y = pointer_pos.y.round() as i32;
                        state.desktop_menu_tx.send_modify(|v| {
                            v.0 = v.0.wrapping_add(1);
                            v.1 = x;
                            v.2 = y;
                            v.3 = out.w;
                            v.4 = out.h;
                        });
                        crate::state::DESKTOP_MENU_OPEN
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        pointer.frame(state);
                        return;
                    }
                }
            }
            let windows: Vec<_> = state
                .space
                .elements()
                .filter_map(|i| i.as_window().cloned())
                .collect();
            for w in &windows {
                w.window().set_activated(false);
                if let Some(t) = w.window().toplevel() {
                    t.send_pending_configure();
                }
            }
            keyboard.set_focus(state, Option::<WlSurface>::None, serial);
        }}
    }

    pointer.button(
        state,
        &ButtonEvent {
            button,
            state: button_state,
            serial,
            time: event.time_msec(),
        },
    );
    pointer.frame(state);
}

/// Dispatcha ação clicada num item do context menu. H48.ctx.5-7.
/// Wallpaper bloqueia até o user fechar o file picker — aceitável
/// porque o menu é modal de qualquer jeito.
/// Roda uma ação do context menu do desktop pedida pelo CLIENTE (D-Bus):
/// organizar, colar, criar, wallpaper. Sintetiza um `ContextItem` e reusa o
/// `dispatch_context_action`. Sem `context_menu` do compositor aberto, as
/// actions per-ícone ficam None (essas são de desktop, ok).
pub(crate) fn run_desktop_action(state: &mut LumaState, action: crate::context_menu::ContextAction) {
    let item = crate::context_menu::ContextItem {
        label: String::new(),
        icon: None,
        kind: crate::context_menu::ItemKind::Action(action),
        enabled: true,
        shortcut: None,
    };
    dispatch_context_action(state, &item);
}

pub(crate) fn dispatch_context_action(
    state: &mut LumaState,
    item: &crate::context_menu::ContextItem,
) {
    use crate::context_menu::{ContextAction, ItemKind};
    let action = match &item.kind {
        ItemKind::Action(a) => *a,
        _ => return,
    };
    // H52: actions per-icon precisam do target path. Capture aqui
    // antes de fechar o menu (o fluxo de fora limpa state.context_menu
    // após o dispatch).
    let icon_target = state
        .context_menu
        .as_ref()
        .and_then(|m| m.target.clone());
    // H56 parte 2: âncora do clique pra posicionar novo ícone.
    let click_anchor = state
        .context_menu
        .as_ref()
        .map(|m| m.origin);
    match action {
        ContextAction::OpenTerminal => crate::context_menu::open_terminal(),
        ContextAction::ChangeWallpaperPick => {
            // H51 → H47c.fix: file picker async — thread faz kdialog
            // + copy + reload. Main loop livre durante o file picker.
            crate::context_menu::change_wallpaper_async(
                state.wallpaper_arc.clone(),
            );
        }
        ContextAction::SetWallpaper(name) => {
            // H51: galeria shipada — copia + hot-reload sem dialog.
            if let Some(dst) = crate::context_menu::apply_shipped_wallpaper(name) {
                if let Some(wp) = state.wallpaper_arc.as_ref() {
                    wp.reload_from_path(&dst);
                }
            }
        }
        ContextAction::OpenLumaSettings => crate::context_menu::open_luma_settings(),
        // 3c.4: 3 entry points pro app luma-config com contexto.
        ContextAction::OpenConfigAddPanel => {
            crate::context_menu::open_luma_config(&["--tab", "widgets", "--target", "panel"]);
        }
        ContextAction::OpenConfigAddDesktop => {
            crate::context_menu::open_luma_config(&["--tab", "widgets", "--target", "desktop"]);
        }
        ContextAction::OpenConfigCustomize => {
            crate::context_menu::open_luma_config(&["--tab", "widgets"]);
        }
        ContextAction::ConfigureDisplay => {
            // Dim na UI; este arm cobre eventual desync (item enabled
            // mas action ConfigureDisplay). No-op.
        }
        ContextAction::OpenTarget => {
            if let Some(p) = icon_target.as_ref() {
                tracing::info!("desktop: open {:?}", p);
                let _ = std::process::Command::new("xdg-open").arg(p).spawn();
            }
        }
        ContextAction::CopyPath => {
            if let Some(p) = icon_target.as_ref() {
                crate::context_menu::copy_to_clipboard(&p.to_string_lossy());
            }
        }
        ContextAction::Rename => {
            // H53: renomear inline (sem kdialog). Seta state + sincroniza
            // o layer pra o render desenhar o campo. Foco de teclado vai
            // pra None pro filter interceptar tudo que for digitado.
            if let Some(p) = icon_target.as_ref() {
                let r = crate::desktop_icons::DesktopRename::new(p.clone());
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_renaming(Some(r.clone()));
                }
                state.desktop_renaming = Some(r);
                if let Some(kb) = state.seat.get_keyboard() {
                    let serial2 = smithay::utils::SERIAL_COUNTER.next_serial();
                    kb.set_focus(state, Option::<WlSurface>::None, serial2);
                }
            }
        }
        ContextAction::MoveToTrash => {
            if let Some(p) = icon_target.as_ref() {
                crate::context_menu::move_to_trash(p);
                // Limpa seleção desse path (vai sumir no próximo poll).
                state.desktop_selection.remove(p);
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_selection(state.desktop_selection.clone());
                }
            }
        }
        ContextAction::DBusMenuClick(id) => {
            // H47c: dispatch do click pra app via DBusMenu.Event.
            let target = state
                .context_menu
                .as_ref()
                .and_then(|m| m.dbus_menu.clone());
            if let Some((service, menu_path)) = target {
                crate::sni::send_menu_event(&service, &menu_path, id);
            }
        }
        ContextAction::ArrangeIcons(kind) => {
            // H56 parte 1: reorganiza desktop icons pelo critério.
            // Zero ícones → no-op silencioso. Já ordenado → render
            // redundante, sem erro (idempotente).
            let entries = state.desktop_entries.clone();
            if entries.is_empty() {
                return;
            }
            let theme = crate::theme::current();
            let t_di = &theme.desktop_icons;
            // Calcula rows do mesmo jeito que o render path.
            let output = state
                .space
                .output_geometry(state.primary_output())
                .map(|g| g.size);
            let Some(output) = output else { return; };
            let taskbar_h = crate::taskbar::taskbar_height();
            let usable_h = output.h - t_di.grid_margin - taskbar_h - 8;
            let rows = (usable_h / t_di.cell_height).max(1);
            // Snapshot das positions antes (pra source da animação).
            let prev_positions = state.desktop_positions.clone();
            let prev_layout = crate::desktop_icons::compute_layout(
                &entries, &prev_positions, rows,
            );
            let mut source: std::collections::HashMap<String, (i32, i32)> =
                std::collections::HashMap::with_capacity(entries.len());
            for (e, pos) in entries.iter().zip(prev_layout.iter()) {
                source.insert(e.name.clone(), *pos);
            }
            // Sort + target positions column-major.
            let sorted = crate::desktop_icons::sort_entries(&entries, kind);
            let target = crate::desktop_icons::column_major_positions(
                &sorted, rows,
            );
            // Atualiza state + layer + persiste no disco
            // imediatamente (a animação é só visual).
            state.desktop_positions = target.clone();
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_positions(target.clone());
            }
            crate::desktop_icons::save_positions(&target);
            // Reduce motion respect: pula a animação se LUMA_REDUCE_MOTION
            // = 1 (ou similar) E o token autoriza respeitá-lo.
            let reduce = theme.accessibility.reduce_motion_respect
                && std::env::var("LUMA_REDUCE_MOTION")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
            if !reduce {
                let anim = crate::desktop_icons::ArrangeAnim {
                    started_at: std::time::Instant::now(),
                    duration_ms: t_di.arrange_anim_duration_ms,
                    source,
                    target,
                };
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_arrange_anim(Some(anim));
                }
            } else {
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_arrange_anim(None);
                }
            }
        }
        ContextAction::ClipboardPaste => {
            // H56 parte 3 (Colar): cola o conteúdo do clipboard
            // interno no ~/Desktop. Copy = duplica e clipboard
            // continua. Cut = move e clipboard esvazia depois.
            let Some(clipboard) = state.luma_clipboard.clone() else {
                return;
            };
            let created = crate::desktop_icons::paste_to_desktop(&clipboard);
            if created.is_empty() {
                // Tudo falhou (ex: cut + source já não existe).
                // Limpa o clipboard pra não ficar pendurado e o
                // ícone translúcido voltar ao normal na próxima
                // tela.
                if matches!(clipboard.op, crate::desktop_icons::LumaClipOp::Cut) {
                    state.luma_clipboard = None;
                    if let Some(l) = state.desktop_icons.as_ref() {
                        l.set_cut_paths(std::collections::HashSet::new());
                    }
                }
                return;
            }
            // Refresh imediato pra new entries aparecerem sem
            // esperar polling.
            let fresh = crate::desktop_icons::scan();
            let theme = crate::theme::current();
            let t_di = &theme.desktop_icons;
            let Some(output) = state
                .space
                .output_geometry(state.primary_output())
                .map(|g| g.size)
            else {
                return;
            };
            let taskbar_h = crate::taskbar::taskbar_height();
            let usable_h = output.h - t_di.grid_margin - taskbar_h - 8;
            let rows = (usable_h / t_di.cell_height).max(1);
            // Computa células ocupadas pelas entries EXISTENTES
            // (excluindo as recém-criadas).
            let created_basenames: std::collections::HashSet<String> = created
                .iter()
                .filter_map(|p| {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                })
                .collect();
            let prev_entries: Vec<_> = fresh
                .iter()
                .filter(|e| !created_basenames.contains(&e.name))
                .cloned()
                .collect();
            let prev_layout = crate::desktop_icons::compute_layout(
                &prev_entries,
                &state.desktop_positions,
                rows,
            );
            let mut used: std::collections::HashSet<(i32, i32)> =
                prev_layout.iter().cloned().collect();
            // Âncora do clique (origin do menu).
            let (mut anchor_col, mut anchor_row) = if let Some(p) = click_anchor {
                let col = ((p.x - t_di.grid_margin) as f32
                    / t_di.cell_width as f32)
                    .round() as i32;
                let row = ((p.y - t_di.grid_margin) as f32
                    / t_di.cell_height as f32)
                    .round() as i32;
                (col.max(0), row.max(0))
            } else {
                (0, 0)
            };
            // Placa sequencial: cada novo entry pega a primeira
            // célula livre a partir da âncora; âncora avança
            // column-major pra o próximo.
            let mut new_selection: std::collections::HashSet<std::path::PathBuf> =
                std::collections::HashSet::with_capacity(created.len());
            for path in created.iter() {
                let basename = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let cell = crate::desktop_icons::find_free_cell_near(
                    &used,
                    (anchor_col, anchor_row),
                    rows,
                );
                used.insert(cell);
                state.desktop_positions.insert(basename, cell);
                new_selection.insert(path.clone());
                // Avança âncora pra próxima célula column-major.
                let (mut c, mut r) = cell;
                r += 1;
                if r >= rows {
                    r = 0;
                    c += 1;
                }
                anchor_col = c;
                anchor_row = r;
            }
            crate::desktop_icons::save_positions(&state.desktop_positions);
            state.desktop_entries = fresh.clone();
            state.desktop_last_scan = Some(std::time::Instant::now());
            // Cut: esvazia clipboard agora (depois do primeiro paste).
            // Copy: mantém pra colar de novo.
            if matches!(clipboard.op, crate::desktop_icons::LumaClipOp::Cut) {
                state.luma_clipboard = None;
                if let Some(l) = state.desktop_icons.as_ref() {
                    l.set_cut_paths(std::collections::HashSet::new());
                }
            }
            // Seleciona só os novos entries.
            state.desktop_selection = new_selection;
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_entries(fresh);
                l.set_positions(state.desktop_positions.clone());
                l.set_selection(state.desktop_selection.clone());
            }
        }
        ContextAction::ClipboardCopy | ContextAction::ClipboardCut => {
            // H56 parte 3 (pre-Colar): popula o clipboard interno
            // com o target + qualquer outra seleção. Substitui slot
            // anterior (slot único).
            let op = matches!(action, ContextAction::ClipboardCut)
                .then_some(crate::desktop_icons::LumaClipOp::Cut)
                .unwrap_or(crate::desktop_icons::LumaClipOp::Copy);
            let paths: Vec<std::path::PathBuf> = if let Some(t) = icon_target.as_ref() {
                if state.desktop_selection.contains(t) && state.desktop_selection.len() > 1 {
                    state.desktop_selection.iter().cloned().collect()
                } else {
                    vec![t.clone()]
                }
            } else {
                return;
            };
            state.luma_clipboard = Some(crate::desktop_icons::LumaClipboard {
                paths,
                op,
            });
            // Sincroniza cut_paths com o layer pra o render aplicar
            // a opacidade. Copy = clear (sem visual).
            let cut_set: std::collections::HashSet<std::path::PathBuf> =
                if op == crate::desktop_icons::LumaClipOp::Cut {
                    state
                        .luma_clipboard
                        .as_ref()
                        .map(|c| c.paths.iter().cloned().collect())
                        .unwrap_or_default()
                } else {
                    std::collections::HashSet::new()
                };
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_cut_paths(cut_set);
            }
            tracing::info!(
                "luma clipboard <- {:?} ({} paths)",
                op,
                state.luma_clipboard.as_ref().map(|c| c.paths.len()).unwrap_or(0),
            );
        }
        ContextAction::CreateNew(kind) => {
            // H56 parte 2: cria pasta/arquivo no ~/Desktop com nome
            // default único, ancora perto do clique, abre rename
            // inline pra o user digitar por cima.
            let Some(path) = crate::desktop_icons::create_new_in_desktop(kind)
            else {
                return;
            };
            // Força refresh imediato do estado pra o novo entry
            // aparecer sem esperar o polling de 2s.
            let fresh = crate::desktop_icons::scan();
            // Calcula rows + cells ocupadas atualmente.
            let theme = crate::theme::current();
            let t_di = &theme.desktop_icons;
            let Some(output) = state
                .space
                .output_geometry(state.primary_output())
                .map(|g| g.size)
            else {
                return;
            };
            let taskbar_h = crate::taskbar::taskbar_height();
            let usable_h = output.h - t_di.grid_margin - taskbar_h - 8;
            let rows = (usable_h / t_di.cell_height).max(1);
            // Layout das entries (excluindo a recém-criada).
            // Constrói "used" pulando o entry novo.
            let basename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let prev_entries: Vec<_> = fresh
                .iter()
                .filter(|e| e.name != basename)
                .cloned()
                .collect();
            let prev_layout = crate::desktop_icons::compute_layout(
                &prev_entries,
                &state.desktop_positions,
                rows,
            );
            let mut used: std::collections::HashSet<(i32, i32)> =
                std::collections::HashSet::with_capacity(prev_layout.len());
            for pos in prev_layout.iter() {
                used.insert(*pos);
            }
            // Âncora = nearest_cell do clique. Usa o origin do menu
            // (já clamped). Logical → pixel direto: usa as fórmulas
            // que o layer.nearest_cell usa.
            let (anchor_col, anchor_row) = if let Some(p) = click_anchor {
                let col = ((p.x - t_di.grid_margin) as f32
                    / t_di.cell_width as f32)
                    .round() as i32;
                let row = ((p.y - t_di.grid_margin) as f32
                    / t_di.cell_height as f32)
                    .round() as i32;
                (col.max(0), row.max(0))
            } else {
                (0, 0)
            };
            let target_cell = crate::desktop_icons::find_free_cell_near(
                &used,
                (anchor_col, anchor_row),
                rows,
            );
            // Salva position.
            state
                .desktop_positions
                .insert(basename.clone(), target_cell);
            crate::desktop_icons::save_positions(&state.desktop_positions);
            // Atualiza state + layer com entries frescas + positions.
            state.desktop_entries = fresh.clone();
            state.desktop_last_scan = Some(std::time::Instant::now());
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_entries(fresh);
                l.set_positions(state.desktop_positions.clone());
            }
            // Seleção apenas no novo entry.
            state.desktop_selection.clear();
            state.desktop_selection.insert(path.clone());
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_selection(state.desktop_selection.clone());
            }
            // Ativa rename inline com o nome default já preenchido +
            // todo selecionado (DesktopRename::new faz isso).
            let r = crate::desktop_icons::DesktopRename::new(path);
            if let Some(l) = state.desktop_icons.as_ref() {
                l.set_renaming(Some(r.clone()));
            }
            state.desktop_renaming = Some(r);
            if let Some(kb) = state.seat.get_keyboard() {
                let serial2 = smithay::utils::SERIAL_COUNTER.next_serial();
                kb.set_focus(state, Option::<WlSurface>::None, serial2);
            }
        }
        ContextAction::Disabled => {}
    }
}

fn cursor_name_for_resize_edge(edge: crate::input::grabs::resize_grab::ResizeEdge) -> &'static str {
    use crate::input::grabs::resize_grab::ResizeEdge;
    let top = edge.intersects(ResizeEdge::TOP);
    let bottom = edge.intersects(ResizeEdge::BOTTOM);
    let left = edge.intersects(ResizeEdge::LEFT);
    let right = edge.intersects(ResizeEdge::RIGHT);
    match (top, bottom, left, right) {
        (true, false, true, false) => "nw-resize",
        (true, false, false, true) => "ne-resize",
        (false, true, true, false) => "sw-resize",
        (false, true, false, true) => "se-resize",
        (true, false, false, false) => "n-resize",
        (false, true, false, false) => "s-resize",
        (false, false, true, false) => "w-resize",
        (false, false, false, true) => "e-resize",
        _ => "default",
    }
}

fn taskbar_hit(
    state: &LumaState,
    pos: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> Option<crate::taskbar::TaskbarHit> {
    let output = state.output_under_cursor(pos);
    let output_size = state.space.output_geometry(output).map(|g| g.size)?;
    let n = state.space.elements().count();
    crate::taskbar::hit_test(output_size, pos, n, state.workspaces.count())
}

/// Hit-test do pager. Retorna idx do workspace clicado. H36.9.
fn pager_hit(
    state: &LumaState,
    pos: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> Option<usize> {
    let count = state.workspaces.count();
    if !crate::pager::is_visible(count) {
        return None;
    }
    let output = state.output_under_cursor(pos);
    let output_size = state.space.output_geometry(output).map(|g| g.size)?;
    let lw = crate::taskbar::launcher_width();
    let launcher_loc = crate::taskbar::launcher_location(output_size);
    let launcher_end_x = launcher_loc.x + lw;
    let pager_start_x = crate::pager::pager_start_x(launcher_end_x);
    let taskbar_h = crate::taskbar::taskbar_height();
    let taskbar_y = output_size.h - taskbar_h;
    let layout = crate::pager::item_layout(
        pager_start_x,
        taskbar_y,
        taskbar_h,
        count,
        state.workspaces.current,
    );
    crate::pager::hit_test(pos, &layout, taskbar_y, taskbar_h)
}

fn resize_edge_hit_for(
    state: &LumaState,
    pos: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> Option<(LumaWindow, crate::input::grabs::resize_grab::ResizeEdge)> {
    state.space.elements().rev().find_map(|item| {
        let window = item.as_window()?;
        let loc = state.space.element_location(item)?;
        let size = {
            use smithay::desktop::space::SpaceElement;
            SpaceElement::geometry(window).size
        };
        let relative = pos - loc.to_f64();
        let edge = resize_grab::hit_test_resize_edge(relative, size)?;
        Some((window.clone(), edge))
    })
}

fn header_hit_for(
    state: &LumaState,
    pos: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> Option<(LumaWindow, crate::decoration::HeaderRegion)> {
    // Itera do topo da pilha pro fundo (rev) pra respeitar z-order.
    state.space.elements().rev().find_map(|item| {
        let window = item.as_window()?;
        let loc = state.space.element_location(item)?;
        let width = window.window().geometry().size.w.max(1);
        let relative = pos - loc.to_f64();
        let region = crate::decoration::hit_test_header(width, relative)?;
        Some((window.clone(), region))
    })
}

pub fn handle_axis<I: InputBackend>(state: &mut LumaState, event: I::PointerAxisEvent) {
    let source = event.source();

    let horizontal = event.amount(Axis::Horizontal).unwrap_or_else(|| {
        event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0
    });
    let vertical = event.amount(Axis::Vertical).unwrap_or_else(|| {
        event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0
    });
    let horizontal_v120 = event.amount_v120(Axis::Horizontal);
    let vertical_v120 = event.amount_v120(Axis::Vertical);

    let mut frame = AxisFrame::new(event.time_msec()).source(source);
    if horizontal != 0.0 {
        frame = frame.value(Axis::Horizontal, horizontal);
        if let Some(d) = horizontal_v120 {
            frame = frame.v120(Axis::Horizontal, d as i32);
        }
    }
    if vertical != 0.0 {
        frame = frame.value(Axis::Vertical, vertical);
        if let Some(d) = vertical_v120 {
            frame = frame.v120(Axis::Vertical, d as i32);
        }
    }

    if source == AxisSource::Finger {
        if event.amount(Axis::Horizontal) == Some(0.0) {
            frame = frame.stop(Axis::Horizontal);
        }
        if event.amount(Axis::Vertical) == Some(0.0) {
            frame = frame.stop(Axis::Vertical);
        }
    }

    let Some(pointer) = state.seat.get_pointer() else {
        return;
    };
    pointer.axis(state, frame);
    pointer.frame(state);
}

// ─────────────────────────────────────────────────────────────────────
// Hit tests do menu fullscreen híbrido (H32.10).
// Os layouts são reproduzidos a partir das constantes do `menu::render`
// e dos helpers de cada zona. Posições devem casar com o render.
// ─────────────────────────────────────────────────────────────────────

fn hit_test_favorites(
    fav_origin: smithay::utils::Point<i32, smithay::utils::Logical>,
    point: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> Option<usize> {
    let f = &crate::theme::current().menu_favorites;
    let grid_w = f.columns * f.icon_size + (f.columns - 1) * f.gap_horizontal;
    let grid_offset_x = (crate::menu::render::layout::FAV_GRID_W - grid_w).max(0) / 2;
    let anchor = smithay::utils::Point::<i32, smithay::utils::Logical>::from((
        fav_origin.x + grid_offset_x,
        fav_origin.y,
    ));
    for (i, slot) in crate::menu::favorites_grid::slot_origins(anchor).iter().enumerate() {
        let x = slot.x as f64;
        let y = slot.y as f64;
        let w = f.icon_size as f64;
        let h = f.icon_size as f64;
        if point.x >= x && point.x < x + w && point.y >= y && point.y < y + h {
            return Some(i);
        }
    }
    None
}

fn hit_test_all_apps(
    zone_origin: smithay::utils::Point<i32, smithay::utils::Logical>,
    point: smithay::utils::Point<f64, smithay::utils::Logical>,
    page: usize,
    total_apps: usize,
) -> Option<usize> {
    let a = &crate::theme::current().menu_all_apps;
    let gw = crate::menu::all_apps_grid::grid_width();
    let anchor_x = zone_origin.x
        + (crate::menu::render::layout::FAV_GRID_W - gw).max(0) / 2;
    let anchor = smithay::utils::Point::<i32, smithay::utils::Logical>::from((
        anchor_x, zone_origin.y,
    ));
    let total_per_page = (a.columns * a.rows_per_page) as usize;
    let start = page * total_per_page;
    let end = (start + total_per_page).min(total_apps);
    for (slot_idx, app_idx) in (start..end).enumerate() {
        let slot = crate::menu::all_apps_grid::slot_origins(anchor)[slot_idx];
        let x = slot.x as f64;
        let y = slot.y as f64;
        let w = a.icon_size as f64;
        let h = a.icon_size as f64;
        if point.x >= x && point.x < x + w && point.y >= y && point.y < y + h {
            return Some(app_idx);
        }
    }
    None
}

fn hit_test_search_results(
    zone_origin: smithay::utils::Point<i32, smithay::utils::Logical>,
    point: smithay::utils::Point<f64, smithay::utils::Logical>,
    results: &[usize],
) -> Option<usize> {
    let item_w = crate::menu::search_results::item_width() as f64;
    let item_h = crate::menu::search_results::item_height() as f64;
    for (i, origin) in crate::menu::search_results::item_origins(zone_origin, results.len()).iter().enumerate() {
        let x = origin.x as f64;
        let y = origin.y as f64;
        if point.x >= x && point.x < x + item_w && point.y >= y && point.y < y + item_h {
            return results.get(i).copied();
        }
    }
    None
}

fn launch_curated_favorite(
    state: &mut LumaState,
    fav: crate::menu::state::CuratedFavorite,
    pointer: smithay::input::pointer::PointerHandle<LumaState>,
) {
    // Resolve `desktop_ids` candidatos contra state.apps. Primeiro
    // match vence. Compara case-insensitive contra Name/exec.
    let resolved = state.apps.iter().find(|app| {
        let name_low = app.name.to_lowercase();
        let exec_low = app.exec.to_lowercase();
        fav.desktop_ids.iter().any(|id| {
            let id_low = id.to_lowercase();
            name_low.contains(&id_low) || exec_low.contains(&id_low)
        })
    });
    if let Some(app) = resolved.cloned() {
        tracing::info!("click favorito '{}' → launch {}", fav.label, app.name);
        crate::apps::launch(&app.exec);
    } else {
        tracing::warn!("click favorito '{}' — nenhum app instalado dos candidatos: {:?}",
            fav.label, fav.desktop_ids);
    }
    state.menu_open = false;
    state.menu_new.reset();
    pointer.frame(state);
}
