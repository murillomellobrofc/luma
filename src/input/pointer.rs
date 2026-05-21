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
    let under = if state.menu_open { None } else { state.surface_under(pos) };

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

    if button_state == ButtonState::Pressed && !pointer.is_grabbed() {
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
                                    // Right click: deixa o app desenhar o menu.
                                    crate::sni::context_menu(
                                        &item.service, &item.object_path, cx, cy,
                                    );
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
                            // Confirmar — executa e fecha.
                            if let Some(action) = state.menu_new.pending_action {
                                crate::menu::session_bar::execute_action(action);
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
