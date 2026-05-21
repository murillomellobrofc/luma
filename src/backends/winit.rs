use std::time::Duration;

use std::sync::Mutex;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker,
            element::{
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
                surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
                Kind,
            },
            gles::GlesRenderer,
        },
        winit::{self, WinitEvent},
    },
    input::pointer::{CursorImageAttributes, CursorImageStatus},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
    utils::{Rectangle, Scale, Transform},
    wayland::compositor::with_states,
};
use tracing::{error, info, warn};

use crate::{render::LumaRenderElement, state::CalloopData};

/// Abreviação pt-BR de mês (3 letras lowercase). Usado na linha 2 do
/// clock pra mostrar "14 mai" em vez de "14 May" do locale en_US do
/// chrono. H32.5.10.
fn with_alpha_color(
    c: smithay::backend::renderer::Color32F,
    alpha: f32,
) -> smithay::backend::renderer::Color32F {
    smithay::backend::renderer::Color32F::new(c.r(), c.g(), c.b(), alpha)
}

/// Trunca string em N chars adicionando "…" se passou. H44.volume.5.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn month_pt_br(m: u32) -> &'static str {
    match m {
        1 => "jan",
        2 => "fev",
        3 => "mar",
        4 => "abr",
        5 => "mai",
        6 => "jun",
        7 => "jul",
        8 => "ago",
        9 => "set",
        10 => "out",
        11 => "nov",
        12 => "dez",
        _ => "?",
    }
}

/// Abrev pt-BR do dia da semana (3 chars). H33.taskbar.E.14.
fn weekday_pt_br(d: u32) -> &'static str {
    match d {
        1 => "seg",
        2 => "ter",
        3 => "qua",
        4 => "qui",
        5 => "sex",
        6 => "sáb",
        7 => "dom",
        _ => "?",
    }
}

/// Strftime localizado pt-BR. Substitui %a (weekday) e %b (month) pelos
/// abrevs pt-BR; resto passa pelo chrono. H33.taskbar.E.14.
fn strftime_pt_br(fmt: &str, now: &chrono::DateTime<chrono::Local>) -> String {
    use chrono::Datelike;
    let mut out = String::with_capacity(fmt.len());
    let mut iter = fmt.chars().peekable();
    while let Some(c) = iter.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match iter.next() {
            Some('a') => out.push_str(weekday_pt_br(now.weekday().number_from_monday())),
            Some('b') => out.push_str(month_pt_br(now.month())),
            Some('H') => out.push_str(&now.format("%H").to_string()),
            Some('M') => out.push_str(&now.format("%M").to_string()),
            Some('d') => out.push_str(&now.format("%d").to_string()),
            Some('m') => out.push_str(&now.format("%m").to_string()),
            Some('Y') => out.push_str(&now.format("%Y").to_string()),
            Some('y') => out.push_str(&now.format("%y").to_string()),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

pub fn init_winit(
    event_loop: &mut EventLoop<'static, CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    // H35.3: abre 2 windows winit, cada uma representa um output virtual.
    // Window 1 (primary): renderiza tudo (taskbar, menu, janelas).
    // Window 2 (secondary): existe na infra (state.outputs) mas o render
    // ainda não envia conteúdo pra ela. Próximos commits expandem.
    use smithay::reexports::winit::{dpi::LogicalSize, window::WindowAttributes};
    let attrs = vec![
        WindowAttributes::default()
            .with_inner_size(LogicalSize::new(1920.0, 1080.0))
            .with_title("Luma — Monitor 1"),
        WindowAttributes::default()
            .with_inner_size(LogicalSize::new(1920.0, 1080.0))
            .with_title("Luma — Monitor 2"),
    ];
    let gl_attrs = smithay::backend::egl::context::GlAttributes {
        version: (3, 0),
        profile: None,
        debug: cfg!(debug_assertions),
        vsync: false,
    };
    let (mut backends, winit) =
        winit::init_multi::<GlesRenderer>(attrs, gl_attrs)?;
    info!(
        "backend winit pronto, {} janelas abertas",
        backends.len()
    );
    let mut backend = backends.remove(0);
    let secondary_backend = if !backends.is_empty() {
        Some(backends.remove(0))
    } else {
        None
    };

    let mode = Mode {
        size: backend.window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Luma".into(),
            model: "Winit".into(),
            serial_number: "Unknown".into(),
        },
    );
    let _output_global = output.create_global::<crate::state::LumaState>(&data.state.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    data.state.space.map_output(&output, (0, 0));
    // Registra também no Vec<Output> do state (H34.1).
    data.state.outputs.push(output.clone());

    // H35.3: secondary output (Monitor 2) à direita do primary.
    if let Some(sec_backend) = &secondary_backend {
        let sec_mode = Mode {
            size: sec_backend.window_size(),
            refresh: 60_000,
        };
        let sec_output = Output::new(
            "winit-2".to_string(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Luma".into(),
                model: "Winit-2".into(),
                serial_number: "Unknown".into(),
            },
        );
        let _sec_global =
            sec_output.create_global::<crate::state::LumaState>(&data.state.display_handle);
        sec_output.change_current_state(
            Some(sec_mode),
            Some(Transform::Flipped180),
            None,
            Some((backend.window_size().w, 0).into()),
        );
        sec_output.set_preferred(sec_mode);
        data.state
            .space
            .map_output(&sec_output, (backend.window_size().w, 0));
        data.state.outputs.push(sec_output);
        info!("H35.3: secondary output registrado em state.outputs");
    }
    // Output focado inicia no primário (H34.6). Em multi-monitor real,
    // muda quando o cursor cruza pra outro monitor.
    data.state.focused_output_name = Some(output.name());
    // Vincula a taskbar única atual ao output (H34.3). Hoje 1 taskbar
    // total; multi-monitor futuro vai ter 1 por output.
    data.state.taskbar.attach_to_output(&output);

    // Wallpaper como SpaceItem com z_index=0 (atrás de tudo). Atualizamos
    // o output_size dele toda vez que a winit redimensiona.
    if let Some(wp) = data.state.wallpaper.take() {
        let initial_size = smithay::utils::Size::<i32, smithay::utils::Logical>::from((
            backend.window_size().w,
            backend.window_size().h,
        ));
        wp.set_output_size(initial_size);
        wp.attach_to_output(&output);
        let arc = std::sync::Arc::new(wp);
        data.state.space.map_element(
            crate::space_item::SpaceItem::Wallpaper(arc.clone()),
            smithay::utils::Point::<i32, smithay::utils::Logical>::from((0, 0)),
            false,
        );
        data.state.wallpaper_arc = Some(arc);
    }

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    event_loop
        .handle()
        .insert_source(winit, move |event, _, data| match event {
            WinitEvent::Resized { size, scale_factor } => {
                info!("janela redimensionada pra {size:?} (scale {scale_factor})");
                output.change_current_state(
                    Some(Mode {
                        size,
                        refresh: 60_000,
                    }),
                    None,
                    None,
                    None,
                );
                if let Some(wp) = data.state.wallpaper_arc.as_ref() {
                    wp.set_output_size(smithay::utils::Size::from((size.w, size.h)));
                }
            }
            WinitEvent::Input(event) => data.state.process_input_event(event),
            WinitEvent::Focus(_) => {}
            WinitEvent::Redraw => {
                let size = backend.window_size();
                let damage = Rectangle::from_size(size);

                // Decorações agora vivem dentro do AsRenderElements de cada
                // LumaWindow, então o space::render_output cuida do z-order
                // automaticamente. Aqui só sobra o cursor como custom element.

                // Atualiza títulos das janelas (re-rasteriza só quando muda).
                if let Some(text) = data.state.text.as_ref() {
                    for item in data.state.space.elements() {
                        if let Some(window) = item.as_window() {
                            crate::decoration::update_title(window.window(), text);
                        }
                    }
                }

                // Taskbar: dimensão do output em logical (winit usa scale 1).
                let output_size_logical = smithay::utils::Size::<i32, smithay::utils::Logical>::from(
                    (size.w, size.h),
                );
                // Coleta surfaces das janelas existentes (independente da
                // ordem do z-order) e sincroniza a ordem estável da taskbar.
                // H36.fix.3: filtra baseado na posicao — janelas de outros
                // workspaces estao em x=-1_000_000 graças ao refresh_workspace
                // _visibility. Mais robusto que comparar WlSurface direto.
                let current_surfaces: Vec<_> = data
                    .state
                    .space
                    .elements()
                    .filter(|item| {
                        if item.as_window().is_none() {
                            return false;
                        }
                        match data.state.space.element_location(item) {
                            Some(loc) => loc.x > -100_000,
                            None => false,
                        }
                    })
                    .filter_map(|i| i.as_window())
                    .filter_map(|w| w.window().toplevel().map(|t| t.wl_surface().clone()))
                    .collect();
                data.state.taskbar.sync_order(&current_surfaces);
                let window_surfaces: Vec<Option<_>> = data
                    .state
                    .taskbar
                    .order
                    .iter()
                    .cloned()
                    .map(Some)
                    .collect();
                let n_windows = window_surfaces.len();
                // Resolve app_id + ícone de cada janela na taskbar. H33.taskbar.4:
                // app_id também coletado pra gerar fallback colorido por hash
                // quando não tem ícone do `.desktop`.
                let taskbar_app_ids: Vec<String> = window_surfaces
                    .iter()
                    .map(|opt| {
                        let Some(surface) = opt.as_ref() else { return String::new() };
                        smithay::wayland::compositor::with_states(surface, |states| {
                            states
                                .data_map
                                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                                .and_then(|d| {
                                    let g = d.lock().unwrap();
                                    g.app_id.clone().or_else(|| g.title.clone())
                                })
                        })
                        .unwrap_or_default()
                    })
                    .collect();
                let taskbar_icons: Vec<Option<crate::apps::AppIcon>> = taskbar_app_ids
                    .iter()
                    .map(|app_id| {
                        if app_id.is_empty() {
                            return None;
                        }
                        crate::apps::find_icon_for_app_id(
                            &data.state.apps,
                            &data.state.app_icons,
                            app_id,
                        )
                        .cloned()
                    })
                    .collect();
                let focused_surface = data
                    .state
                    .seat
                    .get_keyboard()
                    .and_then(|kb| kb.current_focus());
                // H32.6.3: marca cada janela como focada ou não pra
                // recolorir a borda externa (amber focada / cream sem foco).
                for item in data.state.space.elements() {
                    if let Some(luma_win) = item.as_window() {
                        let win = luma_win.window();
                        let is_focused = match (
                            win.toplevel().map(|t| t.wl_surface().clone()),
                            focused_surface.as_ref(),
                        ) {
                            (Some(a), Some(b)) => &a == b,
                            _ => false,
                        };
                        crate::decoration::set_focused(win, is_focused);
                    }
                }
                if let Some(text) = data.state.text.as_ref() {
                    let now = chrono::Local::now();
                    let t_clock = &crate::theme::current().clock;
                    let time_str = strftime_pt_br(&t_clock.time_format, &now);
                    let date_str = strftime_pt_br(&t_clock.date_format, &now);
                    data.state.taskbar.ensure_clock(text, &time_str, &date_str);
                    data.state.taskbar.ensure_title_labels(text);
                    data.state.pager.ensure_labels(text, 9);
                    data.state.overview.ensure_labels(text);
                    crate::decoration::ensure_button_glyphs(text);
                    // H33.taskbar.6: pre-rasteriza tooltip do app sob hover
                    // se já passou `tooltip_delay_ms` desde o início do hover.
                    let delay_ms =
                        crate::theme::current().taskbar.apps.tooltip_delay_ms as u128;
                    let should_show = matches!(
                        (data.state.taskbar.hovered_block, data.state.taskbar.hover_started_at),
                        (Some(_), Some(t)) if t.elapsed().as_millis() >= delay_ms
                    );
                    if should_show {
                        if let Some(idx) = data.state.taskbar.hovered_block {
                            let label = taskbar_app_ids
                                .get(idx)
                                .cloned()
                                .filter(|s| !s.is_empty())
                                .or_else(|| {
                                    data.state
                                        .taskbar
                                        .title_labels
                                        .get(idx)
                                        .and_then(|t| t.as_ref())
                                        .map(|tc| tc.text.clone())
                                })
                                .unwrap_or_default();
                            if !label.is_empty() {
                                crate::taskbar::ensure_tooltip(
                                    &mut data.state.taskbar,
                                    &label,
                                    text,
                                );
                            }
                        }
                    }
                }
                // Clone do cache de títulos (Arc no buffer, cheap).
                let taskbar_titles: Vec<Option<crate::taskbar::TitleCache>> =
                    data.state.taskbar.title_labels.clone();
                // H32.5.5: pílulas pré-renderizadas (3 cores compartilhadas
                // por todos os blocos). Decidimos o estado de cada bloco aqui
                // (Inactive/Hover/Active) e clonamos as texturas pra evitar
                // borrow conflict com renderer no render.
                let block_inactive = data.state.taskbar.block_inactive.clone();
                let block_hover = data.state.taskbar.block_hover.clone();
                let block_active = data.state.taskbar.block_active.clone();
                let block_minimized = data.state.taskbar.block_minimized.clone();
                // Resolve estado minimizado por bloco (paralelo a window_surfaces).
                let minimized: Vec<bool> = window_surfaces
                    .iter()
                    .map(|opt| {
                        opt.as_ref()
                            .and_then(|surf| {
                                data.state
                                    .space
                                    .elements()
                                    .filter_map(|i| i.as_window())
                                    .find(|w| {
                                        w.window().toplevel().map(|t| t.wl_surface()) == Some(surf)
                                    })
                                    .map(|w| crate::decoration::is_minimized(w.window()))
                            })
                            .unwrap_or(false)
                    })
                    .collect();
                let block_states: Vec<crate::taskbar::BlockState> = (0..n_windows)
                    .map(|i| {
                        crate::taskbar::block_state_at(
                            &data.state.taskbar,
                            i,
                            &window_surfaces,
                            focused_surface.as_ref(),
                            &minimized,
                        )
                    })
                    .collect();
                let workspace_count = data.state.workspaces.count();
                let taskbar_solids = crate::taskbar::render_solid_elements(
                    &mut data.state.taskbar,
                    output_size_logical,
                    n_windows,
                    &window_surfaces,
                    focused_surface.as_ref(),
                    &minimized,
                    workspace_count,
                );
                // H33.taskbar.4: pre-gera buffers de fallback (quadrado
                // colorido por hash do nome) pros apps sem ícone do
                // `.desktop` resolvido. Cacheia no TaskbarState pra reuso.
                let taskbar_fallbacks: Vec<MemoryRenderBuffer> = taskbar_app_ids
                    .iter()
                    .map(|app_id| {
                        let key: &str = if app_id.is_empty() { "?" } else { app_id };
                        crate::taskbar::fallback_icon_for(&mut data.state.taskbar, key)
                    })
                    .collect();
                // H33.taskbar.polish.1: pre-gera a INICIAL (letra) pra cada
                // fallback. Sobreposta no centro do quadrado pra dar
                // contexto visual mesmo sem ícone real do .desktop.
                // Split borrow explícito de `text` e `taskbar` (campos
                // distintos de LumaState).
                let taskbar_initials: Vec<Option<(MemoryRenderBuffer, smithay::utils::Size<i32, smithay::utils::Logical>)>> = {
                    let crate::state::LumaState {
                        text, taskbar, ..
                    } = &mut data.state;
                    match text.as_ref() {
                        Some(text_r) => taskbar_app_ids
                            .iter()
                            .map(|app_id| {
                                if app_id.is_empty() {
                                    return None;
                                }
                                crate::taskbar::ensure_fallback_initial(
                                    taskbar, app_id, text_r,
                                )
                            })
                            .collect(),
                        None => vec![None; taskbar_app_ids.len()],
                    }
                };
                // H33.taskbar.6: tooltip pronto pra render (se delay passou).
                // Resolve idx + label do hovered + clone do cache em uma
                // pass só, antes do bind do renderer.
                let tooltip_render: Option<(crate::taskbar::TooltipCache, usize)> = {
                    let delay_ms =
                        crate::theme::current().taskbar.apps.tooltip_delay_ms as u128;
                    let elapsed_ok = matches!(
                        (data.state.taskbar.hovered_block, data.state.taskbar.hover_started_at),
                        (Some(_), Some(t)) if t.elapsed().as_millis() >= delay_ms
                    );
                    let idx = data.state.taskbar.hovered_block;
                    if elapsed_ok {
                        idx.and_then(|i| {
                            let label = taskbar_app_ids
                                .get(i)
                                .cloned()
                                .filter(|s| !s.is_empty())
                                .or_else(|| {
                                    taskbar_titles
                                        .get(i)
                                        .and_then(|t| t.as_ref())
                                        .map(|tc| tc.text.clone())
                                })
                                .unwrap_or_default();
                            data.state
                                .taskbar
                                .tooltip_cache
                                .get(&label)
                                .cloned()
                                .map(|tc| (tc, i))
                        })
                    } else {
                        None
                    }
                };
                let _ = &taskbar_titles; // usado acima dentro do closure
                // Clone do gradient bg pra evitar borrow conflict com
                // renderer durante o render (Arc internamente, cheap).
                let taskbar_bg = data.state.taskbar.background.clone();
                let launcher_tex = data.state.taskbar.launcher.clone();
                let clock_time_label = data
                    .state
                    .taskbar
                    .clock_time_label
                    .as_ref()
                    .map(|(b, s)| (b.clone(), *s));
                let clock_date_label = data
                    .state
                    .taskbar
                    .clock_date_label
                    .as_ref()
                    .map(|(b, s)| (b.clone(), *s));
                // H33.taskbar.F.16: carrega SVG do show desktop conforme
                // estado (normal/active). Bg colorido vem do hover/active.
                let show_desktop_render: Option<(
                    MemoryRenderBuffer,
                    smithay::utils::Size<i32, smithay::utils::Logical>,
                )> = {
                    let t_sd = crate::theme::current().show_desktop.clone();
                    let name = if data.state.desktop_visible {
                        &t_sd.icon_active_name
                    } else {
                        &t_sd.icon_normal_name
                    };
                    crate::tray::ensure_icon_sized(
                        &mut data.state.tray, name, t_sd.icon_size,
                    )
                };
                let show_desktop_hovered = data.state.taskbar.show_desktop_hovered;
                let desktop_visible = data.state.desktop_visible;
                // H33.taskbar.9: battery icon + label %, gerados aqui pra
                // evitar borrow conflict no render. Posicionamento final
                // (grupo direito colado ao clock) vem em D.12.
                let battery_render: Option<(
                    MemoryRenderBuffer,
                    smithay::utils::Size<i32, smithay::utils::Logical>,
                    MemoryRenderBuffer,
                    smithay::utils::Size<i32, smithay::utils::Logical>,
                )> = {
                    // H33.taskbar.SVG: refresh dos sensores se >= 3s + load
                    // dos SVGs corretos pra cada estado.
                    crate::tray::refresh_if_stale(&mut data.state.tray);
                    let info = data.state.tray.battery_info;
                    match (info, data.state.text.as_ref()) {
                        (Some(b), Some(text_r)) => {
                            let name = crate::tray::battery_icon_name(b);
                            let icon = crate::tray::ensure_icon(&mut data.state.tray, name);
                            let label = crate::tray::ensure_pct_label(
                                &mut data.state.tray, b.capacity, text_r,
                            );
                            match (icon, label) {
                                (Some((ib, isz)), Some((lb, lsz))) => Some((ib, isz, lb, lsz)),
                                _ => None,
                            }
                        }
                        _ => None,
                    }
                };
                // H33.taskbar.SVG + 10.eth: rede (ethernet OU wifi) +
                // bluetooth + volume + bell (todos SVG).
                let net_name = crate::tray::network_icon_name(data.state.tray.network);
                let wifi_render = crate::tray::ensure_icon(&mut data.state.tray, net_name);
                let bt_name = crate::tray::bluetooth_icon_name(data.state.tray.bt_state);
                let bt_render = crate::tray::ensure_icon(&mut data.state.tray, bt_name);
                let volume_name = crate::tray::volume_icon_name(data.state.tray.volume_state);
                let volume_render = crate::tray::ensure_icon(&mut data.state.tray, volume_name);
                let notif_count = data.state.tray.notif_count;
                let bell_name = crate::tray::bell_icon_name(notif_count);
                let bell_render = crate::tray::ensure_icon(&mut data.state.tray, bell_name);
                let notif_dot_render: Option<MemoryRenderBuffer> = None; // H40 conecta


                match backend.bind() {
                    Ok((renderer, mut framebuffer)) => {
                        // Cursor element exige &mut renderer pra importar o
                        // texture do MemoryRenderBuffer, então é construído
                        // só agora dentro do escopo do bind().
                        let mut custom_elements: Vec<LumaRenderElement<GlesRenderer>> = Vec::new();
                        let pointer_loc = data.state.pointer_location.to_i32_round();

                        match &data.state.cursor_status {
                            CursorImageStatus::Hidden => {}
                            CursorImageStatus::Surface(surface) => {
                                let hotspot = with_states(surface, |states| {
                                    states
                                        .data_map
                                        .get::<Mutex<CursorImageAttributes>>()
                                        .map(|m| m.lock().unwrap().hotspot)
                                        .unwrap_or_default()
                                });
                                let cursor_physical = (pointer_loc - hotspot).to_physical(1);
                                let surface_elements: Vec<
                                    WaylandSurfaceRenderElement<GlesRenderer>,
                                > = render_elements_from_surface_tree(
                                    renderer,
                                    surface,
                                    cursor_physical,
                                    Scale::from(1.0),
                                    1.0,
                                    Kind::Cursor,
                                );
                                custom_elements.extend(
                                    surface_elements.into_iter().map(LumaRenderElement::Surface),
                                );
                            }
                            CursorImageStatus::Named(_) => {
                                // Escolhe o cursor adequado ao contexto
                                // (default ou resize). Fallback pra "default"
                                // se o nome ativo não foi carregado.
                                let entry = data
                                    .state
                                    .cursor_buffers
                                    .get(data.state.cursor_kind)
                                    .or_else(|| data.state.cursor_buffers.get("default"));
                                if let Some((buffer, hotspot)) = entry {
                                    let cursor_physical =
                                        (pointer_loc - *hotspot).to_physical(1);
                                    match MemoryRenderBufferRenderElement::from_buffer(
                                        renderer,
                                        cursor_physical.to_f64(),
                                        buffer,
                                        None,
                                        None,
                                        None,
                                        Kind::Cursor,
                                    ) {
                                        Ok(e) => custom_elements.push(LumaRenderElement::Memory(e)),
                                        Err(err) => warn!("erro montando cursor named: {err}"),
                                    }
                                }
                            }
                        }
                        // H32.10.2/.5: menu fullscreen híbrido, se aberto.
                        // Renderiza por cima de tudo (acima inclusive do
                        // overview, que fica atrás se ambos abrirem). Push
                        // antes do overview pra ficar na frente.
                        // Fade-in/out via menu_anim.alpha() multiplicado na
                        // cor de cada elemento (SolidColorRenderElement::from_buffer
                        // já aceita alpha como 4º parâmetro).
                        data.state.menu_anim.tick(data.state.menu_open);
                        if data.state.menu_anim.is_visible() {
                            data.state.menu_fullscreen.resize(output_size_logical);
                            let anim_alpha = data.state.menu_anim.alpha();
                            let push_solid = |buf, px, py| {
                                let loc = smithay::utils::Point::<i32, smithay::utils::Logical>::from((px, py))
                                    .to_physical_precise_round(1.0);
                                LumaRenderElement::Solid(
                                    smithay::backend::renderer::element::solid::SolidColorRenderElement::from_buffer(
                                        buf, loc, 1.0, anim_alpha, Kind::Unspecified,
                                    ),
                                )
                            };
                            // Wireframe placeholders todos saíram. H32.10.13
                            // substituiu zone[3]/[4] pelos elementos reais.

                            // Greeting corner (H32.10.6): avatar gradient
                            // Honey→Rust + inicial M + "Murillo" + saudação.
                            let user = std::env::var("USER")
                                .unwrap_or_else(|_| "Lumina".to_string());
                            if let Some(text) = data.state.text.as_ref() {
                                data.state.greeting_corner.ensure_labels(text, &user);
                                let now_hour = chrono::Local::now()
                                    .format("%H").to_string()
                                    .parse::<u32>().unwrap_or(12);
                                data.state.greeting_corner.update_greeting(text, now_hour);
                            }
                            let g_theme = &crate::theme::current().menu_greeting;
                            let g_origin = data.state.menu_fullscreen
                                .zone_origin(output_size_logical, 0);
                            // Push memory helper inline pra reuso. Macro
                            // em vez de closure pra não capturar `renderer`
                            // exclusivamente (precisamos usar renderer em
                            // outros from_buffer no mesmo escopo, ex:
                            // ícones reais escalados do .desktop).
                            macro_rules! push_mem {
                                ($buf:expr_2021, $px:expr_2021, $py:expr_2021 $(,)?) => {{
                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>
                                            ::from(($px, $py))
                                            .to_physical_precise_round(1.0);
                                    MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, loc.to_f64(), $buf, Some(anim_alpha),
                                        None, None, Kind::Unspecified,
                                    ).ok().map(LumaRenderElement::Memory)
                                }};
                            }
                            // Letra "M" PRIMEIRO (= front no Smithay), depois
                            // o avatar circle atrás. Fontdue retorna buffer
                            // com altura = font ascent + descent (espaço
                            // abaixo do baseline pra descender — vazio em
                            // glyphs sem descender tipo "M"). Pra centralizar
                            // visualmente, anchora pelo cap height (~70% do
                            // size.h) em vez do buffer height total.
                            if let Some((buf, size)) = data.state.greeting_corner.avatar_letter.as_ref() {
                                let cx = g_origin.x + (g_theme.avatar_size - size.w) / 2;
                                // Offset manual: -size.h/5 (~20% pra cima)
                                // pra compensar o descender vazio do "M".
                                let cy = g_origin.y + (g_theme.avatar_size - size.h) / 2
                                    - size.h / 5;
                                if let Some(e) = push_mem!(buf, cx, cy) {
                                    custom_elements.push(e);
                                }
                            }
                            // Avatar 44×44 atrás da letra.
                            if let Some(e) = push_mem!(
                                &data.state.greeting_corner.avatar,
                                g_origin.x, g_origin.y,
                            ) {
                                custom_elements.push(e);
                            }
                            // Nome à direita do avatar, alinhado topo.
                            let text_x = g_origin.x + g_theme.avatar_size + g_theme.avatar_text_gap;
                            let name_y = g_origin.y + 2;
                            let mut greet_y = name_y + (g_theme.username_size as i32) + g_theme.name_greeting_gap;
                            if let Some((buf, size)) = data.state.greeting_corner.username.as_ref() {
                                if let Some(e) = push_mem!(buf, text_x, name_y) {
                                    custom_elements.push(e);
                                }
                                greet_y = name_y + size.h + g_theme.name_greeting_gap;
                            }
                            // Saudação dinâmica abaixo do nome.
                            if let Some((buf, _)) = data.state.greeting_corner.greeting.as_ref() {
                                if let Some(e) = push_mem!(buf, text_x, greet_y) {
                                    custom_elements.push(e);
                                }
                            }

                            // Search bar pill (H32.10.7): centro horizontal,
                            // 500×64 corner 32. Bg + lupa + placeholder + chip.
                            // Smithay: 1º push = front. Pushar lupa/placeholder/
                            // chip ANTES do bg pra ficarem na frente.
                            if let Some(text) = data.state.text.as_ref() {
                                data.state.menu_search_bar.ensure_labels(text);
                                data.state.menu_search_bar.update_query(
                                    text, &data.state.menu_new.search_query,
                                );
                            }
                            let s_theme = &crate::theme::current().menu_search_bar;
                            let s_origin = data.state.menu_fullscreen
                                .zone_origin(output_size_logical, 1);
                            // 1º (= front): chip label.
                            let chip_x = s_origin.x + s_theme.width
                                - s_theme.padding_horizontal - s_theme.chip_width;
                            let chip_y = s_origin.y + (s_theme.height - s_theme.chip_height) / 2;
                            if let Some((buf, size)) = data.state.menu_search_bar.chip_label.as_ref() {
                                let tx = chip_x + (s_theme.chip_width - size.w) / 2;
                                let ty = chip_y + (s_theme.chip_height - size.h) / 2;
                                if let Some(e) = push_mem!(buf, tx, ty) {
                                    custom_elements.push(e);
                                }
                            }
                            // 2º: chip bg.
                            if let Some(e) = push_mem!(
                                &data.state.menu_search_bar.chip_bg,
                                chip_x, chip_y,
                            ) {
                                custom_elements.push(e);
                            }
                            // 3º: texto digitado OU placeholder (se vazio
                            // E search NÃO focada — quando focada, placeholder
                            // some pra cursor "|" ocupar o espaço limpo).
                            let icon_y = s_origin.y + (s_theme.height - s_theme.icon_size) / 2;
                            let icon_x = s_origin.x + s_theme.padding_horizontal;
                            let search_active = data.state.menu_new.search_bar_active;
                            let text_label = if !data.state.menu_new.search_query.is_empty() {
                                data.state.menu_search_bar.query_label.as_ref()
                            } else if !search_active {
                                data.state.menu_search_bar.placeholder.as_ref()
                            } else {
                                None
                            };
                            let text_px = icon_x + s_theme.icon_size + s_theme.icon_text_gap;
                            let text_w = text_label.map(|(_, s)| s.w).unwrap_or(0);
                            if let Some((buf, size)) = text_label {
                                let py = s_origin.y + (s_theme.height - size.h) / 2;
                                if let Some(e) = push_mem!(buf, text_px, py) {
                                    custom_elements.push(e);
                                }
                            }
                            // Cursor "|" piscante após o texto quando
                            // search_bar_active. Pisca via elapsed millis
                            // % 1000 < 500 = visível.
                            if data.state.menu_new.search_bar_active {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis())
                                    .unwrap_or(0);
                                if (now % 1000) < 500 {
                                    if let Some((buf, size)) = data.state.menu_search_bar.cursor_label.as_ref() {
                                        // Cursor aparece DEPOIS do texto +
                                        // se vazio, logo após o gap.
                                        let cx = if data.state.menu_new.search_query.is_empty() {
                                            text_px
                                        } else {
                                            text_px + text_w + 2
                                        };
                                        let cy = s_origin.y + (s_theme.height - size.h) / 2;
                                        if let Some(e) = push_mem!(buf, cx, cy) {
                                            custom_elements.push(e);
                                        }
                                    }
                                }
                            }
                            // 4º: lupa.
                            if let Some(e) = push_mem!(
                                &data.state.menu_search_bar.magnifier,
                                icon_x, icon_y,
                            ) {
                                custom_elements.push(e);
                            }
                            // 5º: bg pill.
                            if let Some(e) = push_mem!(
                                &data.state.menu_search_bar.bg_focused,
                                s_origin.x, s_origin.y,
                            ) {
                                custom_elements.push(e);
                            }
                            // 6º (= mais atrás): borda âmbar quando hover.
                            if data.state.menu_new.search_bar_hovered {
                                let bw = s_theme.border_width;
                                if let Some(e) = push_mem!(
                                    &data.state.menu_search_bar.border_pill,
                                    s_origin.x - bw, s_origin.y - bw,
                                ) {
                                    custom_elements.push(e);
                                }
                            }

                            // Grid de favoritos curados 5×2 (H32.10.8):
                            // 10 quadrados coloridos com label embaixo.
                            if let Some(text) = data.state.text.as_ref() {
                                data.state.favorites_grid.ensure_labels(
                                    text,
                                    &data.state.menu_new.fav_items,
                                );
                            }
                            let fav_theme = &crate::theme::current().menu_favorites;
                            let fav_origin = data.state.menu_fullscreen
                                .zone_origin(output_size_logical, 2);
                            // Centraliza o grid no zone[2]: o `slot_origins`
                            // ancora no top-left, vamos calcular o offset
                            // pra deixar centro horizontal no zone.
                            let grid_w = fav_theme.columns * fav_theme.icon_size
                                + (fav_theme.columns - 1) * fav_theme.gap_horizontal;
                            let grid_offset_x = (crate::menu::render::layout::FAV_GRID_W - grid_w) / 2;
                            let grid_anchor = smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                fav_origin.x + grid_offset_x.max(0),
                                fav_origin.y,
                            ));
                            let origins = crate::menu::favorites_grid::slot_origins(grid_anchor);
                            let fav_selected = data.state.menu_new.fav_selected;
                            let fav_zone_is_focused = matches!(
                                data.state.menu_new.focused_zone,
                                crate::menu::state::FocusedZone::FavGrid,
                            );
                            // H32.10.16: search ativa esconde os 2 grids
                            // e mostra lista vertical de resultados.
                            let search_active = !data.state.menu_new.search_query.is_empty();
                            let render_fav_grid = !search_active && matches!(
                                data.state.menu_new.mode,
                                crate::menu::state::MenuMode::Favorites,
                            );
                            let render_all_grid = !search_active && !render_fav_grid;
                            // H32.10.11: render grid 8×4 quando AllApps.
                            if render_all_grid {
                                if let Some(text) = data.state.text.as_ref() {
                                    data.state.all_apps_grid.ensure_labels(text, &data.state.apps);
                                    let total_pages = crate::menu::all_apps_grid::page_count(
                                        data.state.apps.len(),
                                    );
                                    data.state.all_apps_grid.update_page_label(
                                        text, data.state.menu_new.all_apps_page, total_pages,
                                    );
                                }
                                let a_theme = &crate::theme::current().menu_all_apps;
                                let total_per_page = (a_theme.columns * a_theme.rows_per_page) as usize;
                                let page = data.state.menu_new.all_apps_page;
                                let start = page * total_per_page;
                                let end = (start + total_per_page).min(data.state.apps.len());
                                // Centraliza grid no zone[2].
                                let gw = crate::menu::all_apps_grid::grid_width();
                                let anchor_x = fav_origin.x
                                    + (crate::menu::render::layout::FAV_GRID_W - gw).max(0) / 2;
                                let anchor = smithay::utils::Point::<i32, smithay::utils::Logical>
                                    ::from((anchor_x, fav_origin.y));
                                let origins = crate::menu::all_apps_grid::slot_origins(anchor);
                                for (slot_idx, app_idx) in (start..end).enumerate() {
                                    if slot_idx >= origins.len() {
                                        break;
                                    }
                                    let slot = origins[slot_idx];
                                    let is_hovered_app = data.state.menu_new.all_hovered == Some(app_idx);
                                    // Tenta ícone real do .desktop primeiro
                                    // (escalado pra icon_size); fallback é
                                    // o quadrado Coffee único.
                                    let real_icon = data.state.app_icons
                                        .get(app_idx).and_then(|i| i.as_ref());
                                    if let Some((icon_buf, icon_src)) = real_icon {
                                        let iloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>
                                                ::from((slot.x, slot.y))
                                                .to_physical_precise_round(1.0);
                                        let src_rect = smithay::utils::Rectangle::<f64, smithay::utils::Logical>::from_size(
                                            smithay::utils::Size::<f64, smithay::utils::Logical>
                                                ::from((icon_src.w as f64, icon_src.h as f64)),
                                        );
                                        if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer,
                                            iloc.to_f64(),
                                            icon_buf,
                                            Some(anim_alpha),
                                            Some(src_rect),
                                            Some(smithay::utils::Size::from((
                                                a_theme.icon_size, a_theme.icon_size,
                                            ))),
                                            Kind::Unspecified,
                                        ) {
                                            custom_elements.push(LumaRenderElement::Memory(e));
                                        }
                                    } else if let Some(e) = push_mem!(
                                        &data.state.all_apps_grid.fallback_icon,
                                        slot.x, slot.y,
                                    ) {
                                        custom_elements.push(e);
                                    }
                                    // Glow âmbar atrás do ícone hovered.
                                    if is_hovered_app {
                                        let pad = data.state.all_apps_grid.hover_glow_pad;
                                        if let Some(e) = push_mem!(
                                            &data.state.all_apps_grid.hover_glow,
                                            slot.x - pad, slot.y - pad,
                                        ) {
                                            custom_elements.push(e);
                                        }
                                    }
                                    if let Some((buf, size)) = data.state.all_apps_grid.labels
                                        .get(app_idx).and_then(|l| l.as_ref())
                                    {
                                        let lx = slot.x + (a_theme.icon_size - size.w) / 2;
                                        let ly = slot.y + a_theme.icon_size + a_theme.label_margin_top;
                                        if let Some(e) = push_mem!(buf, lx, ly) {
                                            custom_elements.push(e);
                                        }
                                    }
                                }
                                // Page dots + "Página X de Y" abaixo do grid.
                                let total_pages = crate::menu::all_apps_grid::page_count(
                                    data.state.apps.len(),
                                );
                                let dots = crate::menu::all_apps_grid::dot_origins(anchor, total_pages);
                                let current_page = data.state.menu_new.all_apps_page;
                                for (i, dot_pos) in dots.iter().enumerate() {
                                    let buf = if i == current_page {
                                        &data.state.all_apps_grid.dot_active
                                    } else {
                                        &data.state.all_apps_grid.dot_inactive
                                    };
                                    if let Some(e) = push_mem!(buf, dot_pos.x, dot_pos.y) {
                                        custom_elements.push(e);
                                    }
                                }
                                // "Página X de Y" centralizado abaixo dos dots.
                                if let (Some((buf, size)), Some(last_dot)) =
                                    (data.state.all_apps_grid.page_label.as_ref(), dots.last())
                                {
                                    let lx = anchor.x + (crate::menu::all_apps_grid::grid_width() - size.w) / 2;
                                    let ly = last_dot.y + a_theme.page_dot_size + 8;
                                    if let Some(e) = push_mem!(buf, lx, ly) {
                                        custom_elements.push(e);
                                    }
                                }
                            }
                            for (i, slot_origin) in origins.iter().enumerate() {
                                if !render_fav_grid {
                                    break;
                                }
                                if i >= data.state.favorites_grid.icons.len() {
                                    break;
                                }
                                // Highlight: mouse hover tem prioridade
                                // sobre keyboard selected. Quando mouse
                                // está em algum item, o keyboard selected
                                // some pra não duplicar glow em 2 lugares.
                                let mouse_active_in_grid =
                                    data.state.menu_new.fav_hovered.is_some();
                                let is_keyboard_selected = !mouse_active_in_grid
                                    && fav_zone_is_focused
                                    && i == fav_selected;
                                let is_hovered = data.state.menu_new.fav_hovered == Some(i);
                                let is_pressed = data.state.menu_new.fav_pressed == Some(i);
                                let is_selected = is_keyboard_selected || is_hovered || is_pressed;
                                // Glow âmbar atrás (= push DEPOIS porque
                                // Smithay 1º push = front; ordem real:
                                // ícone primeiro, glow depois pra ficar atrás).
                                // Ícone quadrado externo.
                                if let Some(e) = push_mem!(
                                    &data.state.favorites_grid.icons[i],
                                    slot_origin.x, slot_origin.y,
                                ) {
                                    custom_elements.push(e);
                                }
                                if is_selected {
                                    let gx = slot_origin.x - fav_theme.selected_glow_padding;
                                    let gy = slot_origin.y - fav_theme.selected_glow_padding;
                                    if let Some(e) = push_mem!(
                                        &data.state.favorites_grid.glow, gx, gy,
                                    ) {
                                        custom_elements.push(e);
                                    }
                                }
                                // Label centralizado abaixo (âmbar se selected).
                                let label = if is_selected {
                                    data.state.favorites_grid.labels_selected[i].as_ref()
                                } else {
                                    data.state.favorites_grid.labels[i].as_ref()
                                };
                                if let Some((buf, size)) = label {
                                    let lx = slot_origin.x
                                        + (fav_theme.icon_size - size.w) / 2;
                                    let ly = slot_origin.y
                                        + fav_theme.icon_size
                                        + fav_theme.label_margin_top;
                                    if let Some(e) = push_mem!(buf, lx, ly) {
                                        custom_elements.push(e);
                                    }
                                }
                            }

                            // Search results — lista vertical de até 6
                            // apps. Substitui os grids quando query != "".
                            if search_active {
                                if let Some(text) = data.state.text.as_ref() {
                                    data.state.all_apps_grid.ensure_labels(text, &data.state.apps);
                                    data.state.search_results_layer.ensure_enter_glyph(text);
                                }
                                let sr_origin = data.state.menu_fullscreen
                                    .zone_origin(output_size_logical, 2);
                                let results = data.state.menu_new.search_results.clone();
                                let sel = data.state.menu_new.search_selected;
                                let item_origins = crate::menu::search_results::item_origins(
                                    sr_origin, results.len(),
                                );
                                let item_w = crate::menu::search_results::item_width();
                                let item_h = crate::menu::search_results::item_height();
                                let icon_size = crate::menu::search_results::ICON_SIZE;
                                let pad = crate::menu::search_results::ITEM_PADDING;
                                for (i, item_origin) in item_origins.iter().enumerate() {
                                    let Some(&app_idx) = results.get(i) else { continue };
                                    let hovered = data.state.menu_new.search_hovered == results.get(i).copied();
                                    // Mouse tem prioridade — keyboard selected
                                    // some quando mouse hover ativo.
                                    let mouse_active = data.state.menu_new.search_hovered.is_some();
                                    let kb_selected = !mouse_active && i == sel;
                                    let selected = kb_selected || hovered;
                                    // Push label "Enter" + label nome + ícone
                                    // antes do bg (1º = front).
                                    if selected {
                                        if let Some((buf, size)) = data.state.search_results_layer.enter_glyph.as_ref() {
                                            let ex = item_origin.x + item_w - pad - size.w;
                                            let ey = item_origin.y + (item_h - size.h) / 2;
                                            if let Some(e) = push_mem!(buf, ex, ey) {
                                                custom_elements.push(e);
                                            }
                                        }
                                    }
                                    if let Some((buf, size)) = data.state.all_apps_grid.labels
                                        .get(app_idx).and_then(|l| l.as_ref())
                                    {
                                        let nx = item_origin.x + pad * 2 + icon_size;
                                        let ny = item_origin.y + (item_h - size.h) / 2;
                                        if let Some(e) = push_mem!(buf, nx, ny) {
                                            custom_elements.push(e);
                                        }
                                    }
                                    // Ícone real do .desktop à esquerda (40px
                                    // escalado), fallback Coffee se sem ícone.
                                    let ix = item_origin.x + pad;
                                    let iy = item_origin.y + (item_h - icon_size) / 2;
                                    let real_icon = data.state.app_icons
                                        .get(app_idx).and_then(|i| i.as_ref());
                                    if let Some((icon_buf, icon_src)) = real_icon {
                                        let iloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>
                                                ::from((ix, iy))
                                                .to_physical_precise_round(1.0);
                                        let src_rect = smithay::utils::Rectangle::<f64, smithay::utils::Logical>::from_size(
                                            smithay::utils::Size::<f64, smithay::utils::Logical>
                                                ::from((icon_src.w as f64, icon_src.h as f64)),
                                        );
                                        if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, iloc.to_f64(), icon_buf,
                                            Some(anim_alpha), Some(src_rect),
                                            Some(smithay::utils::Size::from((icon_size, icon_size))),
                                            Kind::Unspecified,
                                        ) {
                                            custom_elements.push(LumaRenderElement::Memory(e));
                                        }
                                    } else if let Some(e) = push_mem!(
                                        &data.state.search_results_layer.fallback_icon,
                                        ix, iy,
                                    ) {
                                        custom_elements.push(e);
                                    }
                                    // Barra âmbar 4×24 à esquerda do selected.
                                    if selected {
                                        if let Some(e) = push_mem!(
                                            &data.state.search_results_layer.left_bar,
                                            item_origin.x, item_origin.y + 6,
                                        ) {
                                            custom_elements.push(e);
                                        }
                                    }
                                    // Bg do item (= mais atrás): só selected.
                                    if selected {
                                        if let Some(e) = push_mem!(
                                            &data.state.search_results_layer.item_bg,
                                            item_origin.x, item_origin.y,
                                        ) {
                                            custom_elements.push(e);
                                        }
                                    }
                                }
                            }

                            // Link zone[3] (H32.10.13): mostra "TODOS OS
                            // APPS →" no mode Favorites + "← FAVORITOS"
                            // no mode AllApps (breadcrumb pra voltar).
                            // Some quando search ativa.
                            if !search_active {
                                if let Some(text) = data.state.text.as_ref() {
                                    data.state.all_apps_link.ensure_labels(text);
                                }
                                let l_origin = data.state.menu_fullscreen
                                    .zone_origin(output_size_logical, 3);
                                let l_w = crate::menu::render::layout::ALL_APPS_LINK_W;
                                let l_theme = &crate::theme::current().menu_all_apps_link;
                                let (main_label, hint_label) = if render_fav_grid {
                                    (
                                        data.state.all_apps_link.label.as_ref(),
                                        data.state.all_apps_link.hint.as_ref(),
                                    )
                                } else {
                                    (
                                        data.state.all_apps_link.back_label.as_ref(),
                                        data.state.all_apps_link.back_hint.as_ref(),
                                    )
                                };
                                let mut next_y = l_origin.y;
                                if let Some((buf, size)) = main_label {
                                    let lx = l_origin.x + (l_w - size.w) / 2;
                                    if let Some(e) = push_mem!(buf, lx, next_y) {
                                        custom_elements.push(e);
                                    }
                                    next_y += size.h + l_theme.label_hint_gap;
                                }
                                if let Some((buf, size)) = hint_label {
                                    let lx = l_origin.x + (l_w - size.w) / 2;
                                    if let Some(e) = push_mem!(buf, lx, next_y) {
                                        custom_elements.push(e);
                                    }
                                }
                                // Bg sutil amber atrás do link quando hover.
                                if data.state.menu_new.link_hovered {
                                    if let Some(bg) = data.state.all_apps_link.hover_bg.as_ref() {
                                        if let Some(e) = push_mem!(bg, l_origin.x, l_origin.y) {
                                            custom_elements.push(e);
                                        }
                                    }
                                }
                            }

                            // Session bar bottom (zona 4, H32.10.13):
                            // bg + 5 placeholders dos botões + hint + 4 places.
                            // Smithay 1º push = front, então pusha conteúdo
                            // antes do bg.
                            if let Some(text) = data.state.text.as_ref() {
                                data.state.menu_session_bar.ensure_labels(text);
                            }
                            let sb_origin = data.state.menu_fullscreen
                                .zone_origin(output_size_logical, 4);
                            let sb_w = output_size_logical.w
                                - 2 * crate::menu::render::layout::SESSION_MARGIN_HORIZONTAL;
                            let sb_h = crate::menu::render::layout::SESSION_H;
                            let sb_size = smithay::utils::Size::<i32, smithay::utils::Logical>
                                ::from((sb_w, sb_h));
                            data.state.menu_session_bar.resize(sb_size);

                            // 5 botões esquerda. Power (idx 4) = Coral bg
                            // por trás do ícone branco. Outros = só ícone
                            // (sem bg, default transparent). Hover = bg
                            // colorido (Honey/Coffee+/Sage/Amber/Amber).
                            let button_origins = crate::menu::session_bar::button_origins(sb_origin);
                            let hovered_idx = match data.state.menu_new.session_hovered {
                                Some(crate::menu::state::SessionButton::Lock) => Some(0),
                                Some(crate::menu::state::SessionButton::Logout) => Some(1),
                                Some(crate::menu::state::SessionButton::Sleep) => Some(2),
                                Some(crate::menu::state::SessionButton::Restart) => Some(3),
                                Some(crate::menu::state::SessionButton::Power) => Some(4),
                                None => None,
                            };
                            for (i, btn_origin) in button_origins.iter().enumerate() {
                                // Ícone SVG primeiro (= front), depois bg.
                                if let Some(icon_buf) = data.state.menu_session_bar.icons[i].as_ref() {
                                    if let Some(e) = push_mem!(icon_buf, btn_origin.x, btn_origin.y) {
                                        custom_elements.push(e);
                                    }
                                } else {
                                    let buf = if i == 4 {
                                        &data.state.menu_session_bar.power_placeholder
                                    } else {
                                        &data.state.menu_session_bar.button_placeholder
                                    };
                                    if let Some(e) = push_mem!(buf, btn_origin.x, btn_origin.y) {
                                        custom_elements.push(e);
                                    }
                                }
                                // Hover GLOW colorido ao redor do botão
                                // (ícone tem bg integrado 64×64 que cobre
                                // hover_bg do mesmo tamanho; glow maior
                                // aparece como halo ao redor).
                                if hovered_idx == Some(i) {
                                    let pad = data.state.menu_session_bar.hover_glow_pad;
                                    if let Some(e) = push_mem!(
                                        &data.state.menu_session_bar.hover_glows[i],
                                        btn_origin.x - pad, btn_origin.y - pad,
                                    ) {
                                        custom_elements.push(e);
                                    }
                                } else if i == 4 {
                                    // Power sem hover: Coral default.
                                    if let Some(e) = push_mem!(
                                        &data.state.menu_session_bar.power_placeholder,
                                        btn_origin.x, btn_origin.y,
                                    ) {
                                        custom_elements.push(e);
                                    }
                                }
                            }

                            // Hint centro.
                            if let Some((buf, size)) = data.state.menu_session_bar.hint_label.as_ref() {
                                let hx = sb_origin.x + (sb_w - size.w) / 2;
                                let hy = sb_origin.y + (sb_h - size.h) / 2;
                                if let Some(e) = push_mem!(buf, hx, hy) {
                                    custom_elements.push(e);
                                }
                            }

                            // 4 mini-places direita: folder/download/image/drive.
                            // Hover usa versão âmbar do ícone.
                            let place_origins = crate::menu::session_bar::place_origins(sb_origin, sb_size);
                            for (i, p) in place_origins.iter().enumerate() {
                                let hovered = data.state.menu_new.place_hovered == Some(i);
                                let buf = if hovered {
                                    data.state.menu_session_bar.places_hover[i].as_ref()
                                } else {
                                    data.state.menu_session_bar.places[i].as_ref()
                                };
                                let buf = buf.unwrap_or(&data.state.menu_session_bar.place_placeholder);
                                if let Some(e) = push_mem!(buf, p.x, p.y) {
                                    custom_elements.push(e);
                                }
                            }

                            // Por último (= atrás): bg da session bar.
                            if let Some(e) = push_mem!(
                                &data.state.menu_session_bar.bg,
                                sb_origin.x, sb_origin.y,
                            ) {
                                custom_elements.push(e);
                            }

                            // Modal de confirmação (H32.10.fix.modal):
                            // se pending_action.is_some, render overlay
                            // dark + caixa central + 2 botões. Vai no
                            // TOPO da pilha de elementos do menu.
                            if let Some(action) = data.state.menu_new.pending_action {
                                if let Some(text) = data.state.text.as_ref() {
                                    data.state.confirm_modal.ensure_labels(text, action);
                                }
                                let m_origin = crate::menu::confirm_modal::modal_origin(
                                    output_size_logical,
                                );
                                let cancel_origin = crate::menu::confirm_modal::cancel_button_origin(m_origin);
                                let confirm_origin = crate::menu::confirm_modal::confirm_button_origin(m_origin);
                                let use_coral = matches!(action, crate::menu::state::SessionButton::Power);
                                let confirm_bg = if use_coral {
                                    &data.state.confirm_modal.confirm_bg_coral
                                } else {
                                    &data.state.confirm_modal.confirm_bg_amber
                                };
                                // Ordem inversa (1º push = front):
                                // labels primeiro, depois bg dos botões,
                                // depois caixa, depois backdrop.
                                if let Some((buf, size)) = data.state.confirm_modal.confirm_label.as_ref() {
                                    let cx = confirm_origin.x
                                        + (crate::menu::confirm_modal::BTN_WIDTH - size.w) / 2;
                                    let cy = confirm_origin.y
                                        + (crate::menu::confirm_modal::BTN_HEIGHT - size.h) / 2;
                                    if let Some(e) = push_mem!(buf, cx, cy) {
                                        custom_elements.push(e);
                                    }
                                }
                                if let Some((buf, size)) = data.state.confirm_modal.cancel_label.as_ref() {
                                    let cx = cancel_origin.x
                                        + (crate::menu::confirm_modal::BTN_WIDTH - size.w) / 2;
                                    let cy = cancel_origin.y
                                        + (crate::menu::confirm_modal::BTN_HEIGHT - size.h) / 2;
                                    if let Some(e) = push_mem!(buf, cx, cy) {
                                        custom_elements.push(e);
                                    }
                                }
                                if let Some(e) = push_mem!(
                                    confirm_bg,
                                    confirm_origin.x, confirm_origin.y,
                                ) {
                                    custom_elements.push(e);
                                }
                                if let Some(e) = push_mem!(
                                    &data.state.confirm_modal.cancel_bg,
                                    cancel_origin.x, cancel_origin.y,
                                ) {
                                    custom_elements.push(e);
                                }
                                // Título centrado em cima dos botões.
                                if let Some((buf, size)) = data.state.confirm_modal.title_label.as_ref() {
                                    let tx = m_origin.x + (crate::menu::confirm_modal::MODAL_WIDTH - size.w) / 2;
                                    let ty = m_origin.y + 50;
                                    if let Some(e) = push_mem!(buf, tx, ty) {
                                        custom_elements.push(e);
                                    }
                                }
                                if let Some(e) = push_mem!(
                                    &data.state.confirm_modal.box_bg,
                                    m_origin.x, m_origin.y,
                                ) {
                                    custom_elements.push(e);
                                }
                                // Backdrop escurece o resto.
                                data.state.confirm_modal.backdrop.resize(
                                    (output_size_logical.w, output_size_logical.h),
                                );
                                custom_elements.push(push_solid(
                                    &data.state.confirm_modal.backdrop,
                                    0, 0,
                                ));
                            }

                            // 2º: linha âmbar topo.
                            custom_elements.push(push_solid(
                                &data.state.menu_fullscreen.top_accent,
                                0, 0,
                            ));
                            // 3º: backdrop fullscreen Coffee@0.94 (atrás).
                            custom_elements.push(push_solid(
                                &data.state.menu_fullscreen.backdrop,
                                0, 0,
                            ));
                        }
                        // H36 Bloco D: overview screen, se aberto. Renderiza
                        // por cima de tudo exceto cursor — push antes de
                        // tudo (Smithay 1º push = front).
                        if data.state.overview_open {
                            let layout = crate::overview::thumb_layout(
                                output_size_logical,
                                data.state.workspaces.count(),
                            );
                            let current = data.state.workspaces.current;
                            // Borders do thumb ativo PRIMEIRO (= front).
                            for (id, x, y, w, h) in &layout {
                                let active = matches!(id, Some(idx) if *idx == current);
                                if !active {
                                    continue;
                                }
                                // Top + bottom + left + right.
                                let push = |buf, px, py| {
                                    let loc = smithay::utils::Point::<i32, smithay::utils::Logical>::from((px, py))
                                        .to_physical_precise_round(1.0);
                                    LumaRenderElement::Solid(
                                        smithay::backend::renderer::element::solid::SolidColorRenderElement::from_buffer(
                                            buf, loc, 1.0, 1.0, Kind::Unspecified,
                                        ),
                                    )
                                };
                                custom_elements.push(push(&data.state.overview.border_top, *x, *y));
                                custom_elements.push(push(&data.state.overview.border_bottom, *x, y + h - 2));
                                custom_elements.push(push(&data.state.overview.border_left, *x, *y));
                                custom_elements.push(push(&data.state.overview.border_right, x + w - 2, *y));
                            }
                            // Title + subtitle no topo do overview. H36.spec.
                            let title_cloned = data.state.overview.title_label.as_ref().map(|(b, s)| (b.clone(), *s));
                            let subtitle_cloned = data.state.overview.subtitle_label.as_ref().map(|(b, s)| (b.clone(), *s));
                            if let Some((buf, size)) = title_cloned {
                                let tx = (output_size_logical.w - size.w) / 2;
                                let ty = 50;
                                let tloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((tx, ty))
                                        .to_physical_precise_round(1.0);
                                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, tloc.to_f64(), &buf, None, None, None, Kind::Unspecified,
                                ) {
                                    custom_elements.push(LumaRenderElement::Memory(e));
                                }
                            }
                            if let Some((buf, size)) = subtitle_cloned {
                                let tx = (output_size_logical.w - size.w) / 2;
                                let ty = 84;
                                let tloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((tx, ty))
                                        .to_physical_precise_round(1.0);
                                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, tloc.to_f64(), &buf, None, None, None, Kind::Unspecified,
                                ) {
                                    custom_elements.push(LumaRenderElement::Memory(e));
                                }
                            }
                            // Plus icon no centro do botao "+" (Memory, frente do bg).
                            // H36.spec.
                            for (id, x, y, w, h) in &layout {
                                if id.is_some() {
                                    continue;
                                }
                                let psize = data.state.overview.plus_icon_size;
                                let px = x + (w - psize.w) / 2;
                                let py = y + (h - psize.h) / 2;
                                let ploc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                        px, py,
                                    ))
                                    .to_physical_precise_round(1.0);
                                if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer,
                                    ploc.to_f64(),
                                    &data.state.overview.plus_icon,
                                    None,
                                    None,
                                    None,
                                    Kind::Unspecified,
                                ) {
                                    custom_elements.push(LumaRenderElement::Memory(e));
                                }
                            }
                            // Mini-cards com ICONE do app pra cada janela.
                            // H36.fix.minis.5: render_elements com Scale nao
                            // escala corretamente no Smithay master pinned —
                            // janelas saiam em tamanho normal. Volta pra
                            // abordagem de mini-cards com icone (robusto).
                            let mini_color = with_alpha_color(
                                crate::theme::current().colors.taskbar_bg_top,
                                0.95,
                            );
                            for (id, tx, ty, tw, th) in &layout {
                                let Some(ws_idx) = id else { continue };
                                let Some(windows) = data
                                    .state
                                    .workspaces
                                    .workspaces
                                    .get(*ws_idx)
                                    .map(|w| w.windows.clone())
                                else {
                                    continue;
                                };
                                let n = windows.len();
                                if n == 0 {
                                    continue;
                                }
                                let pad = 12;
                                let usable_w = tw - 2 * pad;
                                let usable_h = th - 2 * pad;
                                let cell_w = (usable_w / n as i32).max(28);
                                let mini_icon_size = 20i32;
                                for (i, surf) in windows.iter().enumerate() {
                                    let mx = tx + pad + i as i32 * cell_w;
                                    let my = ty + pad;
                                    let mw = (cell_w - 4).max(16);
                                    let mh = usable_h.max(16);
                                    let app_id = smithay::wayland::compositor::with_states(
                                        surf,
                                        |states| {
                                            states
                                                .data_map
                                                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                                                .and_then(|d| d.lock().unwrap().app_id.clone())
                                        },
                                    );
                                    if let Some(aid) = app_id.as_deref() {
                                        if let Some((icon_buf, icon_src)) =
                                            crate::apps::find_icon_for_app_id(
                                                &data.state.apps,
                                                &data.state.app_icons,
                                                aid,
                                            )
                                        {
                                            let ix = mx + (mw - mini_icon_size) / 2;
                                            let iy = my + (mh - mini_icon_size) / 2;
                                            let iloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((ix, iy))
                                                    .to_physical_precise_round(1.0);
                                            let src_rect = smithay::utils::Rectangle::<f64, smithay::utils::Logical>::from_size(
                                                smithay::utils::Size::<f64, smithay::utils::Logical>::from((
                                                    icon_src.w as f64,
                                                    icon_src.h as f64,
                                                )),
                                            );
                                            if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer,
                                                iloc.to_f64(),
                                                icon_buf,
                                                None,
                                                Some(src_rect),
                                                Some(smithay::utils::Size::from((mini_icon_size, mini_icon_size))),
                                                Kind::Unspecified,
                                            ) {
                                                custom_elements.push(LumaRenderElement::Memory(e));
                                            }
                                        }
                                    }
                                    // Bg do mini-card (depois do icone = atras).
                                    let buf = smithay::backend::renderer::element::solid::SolidColorBuffer::new(
                                        (mw, mh),
                                        mini_color,
                                    );
                                    let mloc = smithay::utils::Point::<i32, smithay::utils::Logical>::from((mx, my))
                                        .to_physical_precise_round(1.0);
                                    custom_elements.push(LumaRenderElement::Solid(
                                        smithay::backend::renderer::element::solid::SolidColorRenderElement::from_buffer(
                                            &buf, mloc, 1.0, 1.0, Kind::Unspecified,
                                        ),
                                    ));
                                }
                            }
                            // Labels "N · X janelas" abaixo de cada thumb +
                            // "ATUAL" amber pro ativo. H36.spec.
                            if let Some(text_renderer) = data.state.text.as_ref() {
                                let theme = crate::theme::current();
                                for (id, tx, ty, tw, th) in &layout {
                                    let Some(ws_idx) = id else { continue };
                                    let Some(ws_info) = data
                                        .state
                                        .workspaces
                                        .workspaces
                                        .get(*ws_idx)
                                        .map(|w| (w.windows.len(),))
                                    else { continue };
                                    let n_win = ws_info.0;
                                    let is_active = *ws_idx == current;
                                    let (label_text, color) = if is_active {
                                        (
                                            format!("{} · ATUAL", ws_idx + 1),
                                            theme.colors.accent_amber,
                                        )
                                    } else {
                                        (
                                            format!(
                                                "{} · {} janela{}",
                                                ws_idx + 1,
                                                n_win,
                                                if n_win == 1 { "" } else { "s" }
                                            ),
                                            theme.colors.column_header_color,
                                        )
                                    };
                                    let (pixels, lw, lh) = text_renderer.rasterize_colored(
                                        &label_text,
                                        11.0,
                                        color,
                                    );
                                    if lw == 0 || lh == 0 { continue; }
                                    let buf = smithay::backend::renderer::element::memory::MemoryRenderBuffer::from_slice(
                                        &pixels,
                                        smithay::backend::allocator::Fourcc::Argb8888,
                                        (lw as i32, lh as i32),
                                        1,
                                        smithay::utils::Transform::Normal,
                                        None,
                                    );
                                    let lx = tx + (tw - lw as i32) / 2;
                                    let ly = ty + th + 8;
                                    let lloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, lloc.to_f64(), &buf, None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.push(LumaRenderElement::Memory(e));
                                    }
                                }

                                // Atalhos no rodapé. H36.spec.
                                let shortcuts = [
                                    ("Ctrl+Alt+1..9", "Vai pro workspace N"),
                                    ("Ctrl+Alt+Shift+N", "Move janela pra workspace N"),
                                    ("Ctrl+Alt+Tab", "Cicla pro próximo"),
                                    ("Ctrl+Alt+Esc", "Abre/fecha overview"),
                                ];
                                let bar_h = 64i32;
                                let bar_y = output_size_logical.h - bar_h - 24;
                                let bar_w = output_size_logical.w - 80;
                                let bar_x = 40;
                                // Bg do bar (coffee opaco).
                                let bar_bg = smithay::backend::renderer::element::solid::SolidColorBuffer::new(
                                    (bar_w, bar_h),
                                    smithay::backend::renderer::Color32F::new(
                                        theme.colors.taskbar_bg_top.r() * 0.5,
                                        theme.colors.taskbar_bg_top.g() * 0.5,
                                        theme.colors.taskbar_bg_top.b() * 0.5,
                                        0.7,
                                    ),
                                );
                                let bar_loc = smithay::utils::Point::<i32, smithay::utils::Logical>::from((bar_x, bar_y))
                                    .to_physical_precise_round(1.0);
                                // Atalhos em 2 cols x 2 rows.
                                let col_w = bar_w / 2;
                                for (i, (key, desc)) in shortcuts.iter().enumerate() {
                                    let col = (i % 2) as i32;
                                    let row = (i / 2) as i32;
                                    let cell_x = bar_x + 24 + col * col_w;
                                    let cell_y = bar_y + 14 + row * 22;
                                    // Rasterize key em amber + desc em cream.
                                    let (kp, kw, kh) = text_renderer.rasterize_colored(
                                        key, 11.0, theme.colors.accent_amber,
                                    );
                                    if kw > 0 && kh > 0 {
                                        let kbuf = smithay::backend::renderer::element::memory::MemoryRenderBuffer::from_slice(
                                            &kp, smithay::backend::allocator::Fourcc::Argb8888,
                                            (kw as i32, kh as i32), 1, smithay::utils::Transform::Normal, None,
                                        );
                                        let kloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((cell_x, cell_y))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, kloc.to_f64(), &kbuf, None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.push(LumaRenderElement::Memory(e));
                                        }
                                    }
                                    let (dp, dw, dh) = text_renderer.rasterize_colored(
                                        desc, 11.0, theme.colors.divider,
                                    );
                                    if dw > 0 && dh > 0 {
                                        let dbuf = smithay::backend::renderer::element::memory::MemoryRenderBuffer::from_slice(
                                            &dp, smithay::backend::allocator::Fourcc::Argb8888,
                                            (dw as i32, dh as i32), 1, smithay::utils::Transform::Normal, None,
                                        );
                                        let dloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((cell_x + 140, cell_y))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, dloc.to_f64(), &dbuf, None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.push(LumaRenderElement::Memory(e));
                                        }
                                    }
                                }
                                // Bar bg PUSHED DEPOIS (atras dos textos).
                                custom_elements.push(LumaRenderElement::Solid(
                                    smithay::backend::renderer::element::solid::SolidColorRenderElement::from_buffer(
                                        &bar_bg, bar_loc, 1.0, 1.0, Kind::Unspecified,
                                    ),
                                ));
                            }
                            // Bg dos thumbs (depois das borders, atrás delas).
                            for (id, x, y, _w, _h) in &layout {
                                let buf = match id {
                                    Some(idx) if *idx == current => &data.state.overview.thumb_active_bg,
                                    Some(_) => &data.state.overview.thumb_bg,
                                    None => &data.state.overview.plus_bg,
                                };
                                let loc = smithay::utils::Point::<i32, smithay::utils::Logical>::from((*x, *y))
                                    .to_physical_precise_round(1.0);
                                custom_elements.push(LumaRenderElement::Solid(
                                    smithay::backend::renderer::element::solid::SolidColorRenderElement::from_buffer(
                                        buf, loc, 1.0, 1.0, Kind::Unspecified,
                                    ),
                                ));
                            }
                            // Backdrop fullscreen GRADIENT Coffee→Amber.
                            // Usa backdrop_gradient (1×1200 Memory) e expande
                            // pra cobrir output via dest_size. H36.spec.fix.
                            let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((0, 0))
                                    .to_physical_precise_round(1.0);
                            let bg_src = smithay::utils::Rectangle::<f64, smithay::utils::Logical>::from_size(
                                smithay::utils::Size::<f64, smithay::utils::Logical>::from((1.0, 1200.0)),
                            );
                            if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer,
                                bg_loc.to_f64(),
                                &data.state.overview.backdrop_gradient,
                                None,
                                Some(bg_src),
                                Some(smithay::utils::Size::from((output_size_logical.w, output_size_logical.h))),
                                Kind::Unspecified,
                            ) {
                                custom_elements.push(LumaRenderElement::Memory(e));
                            }
                        }

                        // Clock 2 linhas (H32.5.10): hora 13pt cor primary
                        // H33.taskbar.9-10-11-12: system tray (zona D) ancorado
                        // à esquerda do clock. Ordem ESQ→DIR (spec H33):
                        //   battery+%, wifi, volume, bell.
                        // Calculamos o cursor da DIREITA (bell) pra ESQUERDA
                        // (battery), depois pintamos.
                        {
                            let theme = crate::theme::current();
                            let t_sys = &theme.taskbar.system_tray;
                            let t_grp = &theme.taskbar.right_group;
                            let bar_y_local =
                                output_size_logical.h - crate::taskbar::taskbar_height();
                            let center_y_for = |h: i32| {
                                bar_y_local + (crate::taskbar::taskbar_height() - h) / 2
                            };
                            // Largura efetiva do clock (max entre hora e data).
                            let clock_w = clock_time_label
                                .as_ref()
                                .map(|(_, s)| s.w)
                                .unwrap_or(0)
                                .max(clock_date_label.as_ref().map(|(_, s)| s.w).unwrap_or(0));
                            let right_edge = output_size_logical.w
                                - t_grp.margin_right
                                - crate::taskbar::show_desktop_width();
                            let clock_left = right_edge - t_grp.gap_between_zones - clock_w;
                            // Ordem ESQ→DIR (spec H33): battery+%, wifi,
                            // bluetooth, volume, bell. Cálculo de DIR→ESQ
                            // pra ancorar no clock. Ícones SVG opcionais
                            // (Some quando carregaram do disco). H33.taskbar.SVG.
                            let push_mem = |elems: &mut Vec<LumaRenderElement<GlesRenderer>>,
                                            buf: &MemoryRenderBuffer,
                                            x: i32,
                                            y: i32,
                                            r: &mut GlesRenderer| {
                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((x, y))
                                        .to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    r,
                                    loc.to_f64(),
                                    buf,
                                    None,
                                    None,
                                    None,
                                    Kind::Unspecified,
                                ) {
                                    elems.push(LumaRenderElement::Memory(elem));
                                }
                            };
                            // Largura ocupada por cada ícone (0 quando o
                            // SVG não carregou — esconde aquele slot).
                            let bell_w = bell_render.as_ref().map(|(_, s)| s.w).unwrap_or(0);
                            let volume_w = volume_render.as_ref().map(|(_, s)| s.w).unwrap_or(0);
                            let bt_w = bt_render.as_ref().map(|(_, s)| s.w).unwrap_or(0);
                            let wifi_w = wifi_render.as_ref().map(|(_, s)| s.w).unwrap_or(0);
                            // bell_right_x = canto direito do bell, ancorado ao clock.
                            let bell_right_x = clock_left - t_grp.gap_between_zones;
                            let bell_x = bell_right_x - bell_w;
                            let volume_x = bell_x - t_sys.gap - volume_w;
                            data.state.tray.last_volume_x = Some(volume_x);
                            let bt_x = volume_x - t_sys.gap - bt_w;
                            let wifi_x = bt_x - t_sys.gap - wifi_w;
                            // Battery+%: bloco icon + 4 + label, todo à esquerda do wifi.
                            let (battery_block_w, bat_icon_w) =
                                if let Some((_, bat_sz, _, lbl_sz)) = battery_render.as_ref() {
                                    (bat_sz.w + 4 + lbl_sz.w, bat_sz.w)
                                } else {
                                    (0, 0)
                                };
                            let battery_right_x = if battery_block_w > 0 {
                                wifi_x - t_sys.gap
                            } else {
                                wifi_x
                            };
                            let bat_icon_x = battery_right_x - battery_block_w;
                            let bat_label_x = bat_icon_x + bat_icon_w + 4;

                            // Pinta os ícones.
                            if let Some((bat_buf, bat_sz, lbl_buf, lbl_sz)) = battery_render.as_ref() {
                                push_mem(&mut custom_elements, bat_buf,
                                         bat_icon_x, center_y_for(bat_sz.h), renderer);
                                push_mem(&mut custom_elements, lbl_buf,
                                         bat_label_x, center_y_for(lbl_sz.h), renderer);
                            }
                            if let Some((buf, sz)) = wifi_render.as_ref() {
                                push_mem(&mut custom_elements, buf,
                                         wifi_x, center_y_for(sz.h), renderer);
                            }
                            if let Some((buf, sz)) = bt_render.as_ref() {
                                push_mem(&mut custom_elements, buf,
                                         bt_x, center_y_for(sz.h), renderer);
                            }
                            if let Some((buf, sz)) = volume_render.as_ref() {
                                push_mem(&mut custom_elements, buf,
                                         volume_x, center_y_for(sz.h), renderer);
                            }
                            if let Some((buf, sz)) = bell_render.as_ref() {
                                push_mem(&mut custom_elements, buf,
                                         bell_x, center_y_for(sz.h), renderer);
                            }
                            // Notif dot reservado pra H40 (count sempre 0 hoje).
                            let _ = notif_dot_render;

                            // H47a.tray.1: setinha de overflow à esquerda dos
                            // 5 ícones nativos. Botão 36×36 com bg arredondado
                            // + chevron SVG (down/up). Sem SNI até H47b, fica
                            // em alpha reduzida (arrow_disabled_alpha).
                            let t_ov = &theme.taskbar.tray_overflow;
                            let arrow_size = t_ov.arrow_size;
                            // Gap entre setinha e bloco de ícones (usa gap do
                            // grupo direito pra coerência visual).
                            let arrow_right_x = if battery_block_w > 0 {
                                bat_icon_x - t_grp.gap_between_zones
                            } else {
                                wifi_x - t_grp.gap_between_zones
                            };
                            let arrow_x = arrow_right_x - arrow_size;
                            let arrow_y = center_y_for(arrow_size);
                            data.state.tray.last_overflow_x = Some(arrow_x);
                            // H47a.tray.3: bg vira âmbar quando open;
                            // chevron vira pra cima (icon-tray-overflow-up).
                            // H47a.tray.4: dim (alpha reduzida) quando não
                            // há itens SNI registrados.
                            // H47b.tray.8: sync sni_count com o vetor
                            // compartilhado da thread D-Bus.
                            let sni_snapshot: Vec<crate::sni::SniItem> = data
                                .state
                                .sni_list
                                .lock()
                                .map(|g| g.clone())
                                .unwrap_or_default();
                            data.state.tray.sni_count = sni_snapshot.len();
                            let open = data.state.tray.overflow_open;
                            let has_items = data.state.tray.sni_count > 0;
                            let arrow_alpha = if has_items {
                                1.0
                            } else {
                                t_ov.arrow_disabled_alpha
                            };
                            let arrow_bg_color = if open {
                                t_ov.arrow_bg_open
                            } else {
                                t_ov.arrow_bg
                            };
                            let arrow_bg = crate::gfx::make_rounded_panel(
                                arrow_size,
                                arrow_size,
                                t_ov.arrow_corner_radius,
                                arrow_bg_color,
                            );
                            // Render do bg + chevron passa alpha (dim quando
                            // vazio). insert(0, ...) pra ambos: bg primeiro
                            // (vai pra position 1), chev depois (position 0)
                            // = chev na frente do bg. Push fica errado aqui
                            // pq Smithay renderiza fim→inicio e o ultimo
                            // pushed iria pra BACK.
                            let arrow_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                smithay::utils::Point::<i32, smithay::utils::Logical>
                                    ::from((arrow_x, arrow_y))
                                    .to_physical_precise_round(1.0);
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer, arrow_loc.to_f64(), &arrow_bg,
                                Some(arrow_alpha), None, None, Kind::Unspecified,
                            ) {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            // Chevron icon centralizado dentro do botão.
                            // ~60% do tamanho do botão.
                            let chev_size = (arrow_size as f32 * 0.6) as i32;
                            let chev_name = if open {
                                "tray-overflow-up"
                            } else {
                                "tray-overflow-down"
                            };
                            match crate::tray::ensure_icon_sized(
                                &mut data.state.tray,
                                chev_name,
                                chev_size,
                            ) {
                                Some((chev_buf, _)) => {
                                    let chev_off = (arrow_size - chev_size) / 2;
                                    let chev_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>
                                            ::from((arrow_x + chev_off, arrow_y + chev_off))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, chev_loc.to_f64(), &chev_buf,
                                        Some(arrow_alpha), None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }
                                None => {
                                    tracing::warn!(
                                        "tray overflow: SVG '{}' não carregou em size {}",
                                        chev_name, chev_size,
                                    );
                                }
                            }

                            // H47a.tray.2: painel de overflow renderiza
                            // acima da setinha quando open. Insert(0,)
                            // pra ficar no topo do z-stack como o popup.
                            if data.state.tray.overflow_open {
                                if let Some(panel) = crate::taskbar::tray_overflow_panel_zone(
                                    output_size_logical, &data.state.tray,
                                ) {
                                    let panel_bg = crate::gfx::make_rounded_panel(
                                        panel.size.w,
                                        panel.size.h,
                                        t_ov.panel_corner,
                                        t_ov.panel_bg,
                                    );
                                    // Top accent (linha âmbar 2px no topo
                                    // do painel, marca de seleção).
                                    let accent = crate::gfx::make_rounded_panel(
                                        panel.size.w - 24,
                                        2,
                                        1,
                                        t_ov.panel_top_accent,
                                    );
                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>
                                            ::from((panel.loc.x, panel.loc.y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer,
                                        loc.to_f64(),
                                        &panel_bg,
                                        None, None, None,
                                        Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    let acc_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>
                                            ::from((panel.loc.x + 12, panel.loc.y + 8))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer,
                                        acc_loc.to_f64(),
                                        &accent,
                                        None, None, None,
                                        Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    // H47b.tray.9: grid de ícones SNI no
                                    // painel quando há itens; caso contrário
                                    // mostra placeholder (H47a.tray.4).
                                    if sni_snapshot.is_empty() {
                                        if let Some(text_r) = data.state.text.as_ref() {
                                            let (px, w, h) = text_r.rasterize_colored(
                                                "Sem itens",
                                                t_ov.placeholder_text_size,
                                                t_ov.placeholder_text_color,
                                            );
                                            if w > 0 && h > 0 {
                                                let buf = MemoryRenderBuffer::from_slice(
                                                    &px,
                                                    smithay::backend::allocator::Fourcc::Argb8888,
                                                    (w as i32, h as i32),
                                                    1,
                                                    Transform::Normal,
                                                    None,
                                                );
                                                let tx = panel.loc.x + (panel.size.w - w as i32) / 2;
                                                let ty = panel.loc.y + (panel.size.h - h as i32) / 2;
                                                let t_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                                        ::from((tx, ty))
                                                        .to_physical_precise_round(1.0);
                                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                    renderer,
                                                    t_loc.to_f64(),
                                                    &buf,
                                                    None, None, None,
                                                    Kind::Unspecified,
                                                ) {
                                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                }
                                            }
                                        }
                                    } else {
                                        // Grid 4-col 40×40 com bg arredondado
                                        // + ícone 22 centralizado. H47b.tray.9.
                                        let t_sni = &theme.taskbar.tray_sni;
                                        let btn = t_sni.button_size;
                                        let gap = t_sni.gap;
                                        let cols = t_sni.grid_columns.max(1);
                                        let pad = t_sni.padding;
                                        let total_w = cols * btn + (cols - 1) * gap;
                                        let grid_x = panel.loc.x + (panel.size.w - total_w) / 2;
                                        let grid_y = panel.loc.y + pad + 14;
                                        for (idx, item) in sni_snapshot.iter().enumerate() {
                                            let col = (idx as i32) % cols;
                                            let row = (idx as i32) / cols;
                                            let bx = grid_x + col * (btn + gap);
                                            let by = grid_y + row * (btn + gap);
                                            // Botão bg.
                                            let bg = crate::gfx::make_rounded_panel(
                                                btn,
                                                btn,
                                                t_sni.button_corner_radius,
                                                t_sni.button_bg,
                                            );
                                            let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>
                                                    ::from((bx, by))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, bg_loc.to_f64(), &bg,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                            // Ícone do item.
                                            if let Some(icon_buf) =
                                                crate::sni::load_icon(item, t_sni.icon_size)
                                            {
                                                let icon_off = (btn - t_sni.icon_size) / 2;
                                                let ic_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                                        ::from((bx + icon_off, by + icon_off))
                                                        .to_physical_precise_round(1.0);
                                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                    renderer, ic_loc.to_f64(), &icon_buf,
                                                    None, None, None, Kind::Unspecified,
                                                ) {
                                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // H33.taskbar.F.16: botão Show Desktop 44×44 no
                        // canto direito. Bg condicional (hover/active),
                        // ícone SVG centralizado. Toggle vem em F.17.
                        {
                            let theme = crate::theme::current();
                            let t_sd = &theme.show_desktop;
                            let t_grp = &theme.taskbar.right_group;
                            let bar_y_local =
                                output_size_logical.h - crate::taskbar::taskbar_height();
                            let size = t_sd.size;
                            let sd_x =
                                output_size_logical.w - t_grp.margin_right - size;
                            let sd_y = bar_y_local
                                + (crate::taskbar::taskbar_height() - size) / 2;
                            // Bg: prioridade ACTIVE > HOVER > sem bg.
                            let bg_color = if desktop_visible {
                                Some(t_sd.active_bg)
                            } else if show_desktop_hovered {
                                Some(t_sd.hover_bg)
                            } else {
                                None
                            };
                            // Ícone PRIMEIRO (push antes = na frente no z-stack
                            // do Smithay, que renderiza fim→começo).
                            if let Some((buf, isz)) = show_desktop_render.as_ref() {
                                let ix = sd_x + (size - isz.w) / 2;
                                let iy = sd_y + (size - isz.h) / 2;
                                let iloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                        ::from((ix, iy))
                                        .to_physical_precise_round(1.0);
                                let alpha = if desktop_visible || show_desktop_hovered {
                                    None
                                } else {
                                    Some(t_sd.icon_alpha_default)
                                };
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer,
                                    iloc.to_f64(),
                                    buf,
                                    alpha,
                                    None,
                                    None,
                                    Kind::Unspecified,
                                ) {
                                    custom_elements.push(LumaRenderElement::Memory(elem));
                                }
                            }
                            // Bg depois do ícone (= atrás no z-stack).
                            if let Some(c) = bg_color {
                                let bg = crate::gfx::make_rounded_panel(
                                    size, size, t_sd.corner_radius, c,
                                );
                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                        ::from((sd_x, sd_y))
                                        .to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer,
                                    loc.to_f64(),
                                    &bg,
                                    None,
                                    None,
                                    None,
                                    Kind::Unspecified,
                                ) {
                                    custom_elements.push(LumaRenderElement::Memory(elem));
                                }
                            }
                        }
                        // alpha 1.0 + data 9pt cor secondary alpha 0.65.
                        // Posições calculadas juntas pra alinhamento right.
                        if let (Some((time_buf, time_size)), Some((date_buf, date_size))) =
                            (clock_time_label.as_ref(), clock_date_label.as_ref())
                        {
                            let (time_pt, date_pt) = crate::taskbar::clock_labels_location(
                                output_size_logical,
                                *time_size,
                                *date_size,
                            );
                            let time_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                time_pt.to_physical_precise_round(1.0);
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer,
                                time_loc.to_f64(),
                                time_buf,
                                None,
                                None,
                                None,
                                Kind::Unspecified,
                            ) {
                                custom_elements.push(LumaRenderElement::Memory(elem));
                            }
                            let date_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                date_pt.to_physical_precise_round(1.0);
                            // H33.taskbar.E.14: cor da data já vem do
                            // token `clock.date_color`, sem alpha extra.
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer,
                                date_loc.to_f64(),
                                date_buf,
                                None,
                                None,
                                None,
                                Kind::Unspecified,
                            ) {
                                custom_elements.push(LumaRenderElement::Memory(elem));
                            }
                        }

                        // Launcher (container 32x32 com mini logo Luma desenhado
                        // dentro, H32.5.9). Pré-renderizado uma vez no init.
                        let lw = crate::taskbar::launcher_width();
                        let lh = crate::taskbar::launcher_height();
                        let launcher_loc_log =
                            crate::taskbar::launcher_location(output_size_logical);
                        let launcher_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                            launcher_loc_log.to_physical_precise_round(1.0);
                        let launcher_src = smithay::utils::Rectangle::<f64, smithay::utils::Logical>::from_size(
                            smithay::utils::Size::<f64, smithay::utils::Logical>::from((
                                lw as f64,
                                lh as f64,
                            )),
                        );
                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                            renderer,
                            launcher_loc.to_f64(),
                            &launcher_tex,
                            None,
                            Some(launcher_src),
                            Some(smithay::utils::Size::from((lw, lh))),
                            Kind::Unspecified,
                        ) {
                            custom_elements.push(LumaRenderElement::Memory(elem));
                        }

                        // H36.7-9: pager de workspaces entre launcher e blocks.
                        let ws_count = data.state.workspaces.count();
                        if crate::pager::is_visible(ws_count) {
                            let launcher_end_x = launcher_loc_log.x + lw;
                            let pager_start_x =
                                crate::pager::pager_start_x(launcher_end_x);
                            let taskbar_h = crate::taskbar::taskbar_height();
                            let taskbar_y = output_size_logical.h - taskbar_h;
                            let layout = crate::pager::item_layout(
                                pager_start_x,
                                taskbar_y,
                                taskbar_h,
                                ws_count,
                                data.state.workspaces.current,
                            );
                            let hovered = data.state.pager.hovered;
                            // H33.taskbar.7: pager re-skin — quadrados 32×32
                            // numerados. Ativo = pill âmbar + número coffee.
                            // Inativo = sem bg + número cream alpha 0.45.
                            let item_size = crate::theme::current().workspaces.pager_item_size;
                            let inactive_alpha =
                                crate::theme::current().workspaces.pager_text_alpha_inactive;
                            let labels_active: Vec<Option<(_, _)>> = data
                                .state
                                .pager
                                .number_labels_active
                                .iter()
                                .map(|o| o.as_ref().map(|(b, s)| (b.clone(), *s)))
                                .collect();
                            let labels_inactive: Vec<Option<(_, _)>> = data
                                .state
                                .pager
                                .number_labels_inactive
                                .iter()
                                .map(|o| o.as_ref().map(|(b, s)| (b.clone(), *s)))
                                .collect();
                            // H33.taskbar.polish.2: usa text_size NOMINAL como
                            // altura de referência pro cálculo vertical (fontdue
                            // retorna bbox do glyph -- '1' tem bbox menor que
                            // '8' e quadrados ficavam desalinhados). Horizontal
                            // segue size.w real (centralização visual exata).
                            let nominal_h =
                                crate::theme::current().workspaces.pager_text_size as i32;
                            for (idx, (x, y, active)) in layout.iter().enumerate() {
                                // Número PRIMEIRO (= em cima do bg, vem
                                // na frente do z-stack pq pushed antes).
                                let label = if *active {
                                    labels_active.get(idx).and_then(|o| o.as_ref())
                                } else {
                                    labels_inactive.get(idx).and_then(|o| o.as_ref())
                                };
                                let label_alpha = if *active { None } else { Some(inactive_alpha) };
                                if let Some((buf, size)) = label {
                                    let label_x = x + (item_size - size.w) / 2;
                                    let label_y = y + (item_size - nominal_h) / 2;
                                    let label_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                            label_x, label_y,
                                        ))
                                        .to_physical_precise_round(1.0);
                                    if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer,
                                        label_loc.to_f64(),
                                        buf,
                                        label_alpha,
                                        None,
                                        None,
                                        Kind::Unspecified,
                                    ) {
                                        custom_elements.push(LumaRenderElement::Memory(e));
                                    }
                                }
                                // Bg do item: pill âmbar se ativo, hover sutil
                                // se inativo+hovered, transparente caso contrário.
                                let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((*x, *y))
                                        .to_physical_precise_round(1.0);
                                if *active {
                                    if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer,
                                        bg_loc.to_f64(),
                                        &data.state.pager.pill_active,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.push(LumaRenderElement::Memory(e));
                                    }
                                } else if hovered == Some(idx) {
                                    if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer,
                                        bg_loc.to_f64(),
                                        &data.state.pager.hover_bg,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.push(LumaRenderElement::Memory(e));
                                    }
                                }
                            }
                        }

                        // H33.taskbar.4: quadrados KDE-style 44×44. Cada app
                        // aberto é um botão; ícone 32px centralizado dentro.
                        // App ativo recebe bg `active_bg`, hover recebe
                        // `hover_bg`, inativo não tem bg. Minimizado fica
                        // com alpha reduzido. Indicador âmbar do ativo
                        // (linha 24×3) vem em B.5 via render_solid_elements.
                        let _ = &taskbar_titles; // título inline saiu, vai virar tooltip em B.6
                        let start_x = crate::taskbar::blocks_start_x_with_pager(
                            data.state.workspaces.count(),
                        );
                        let bar_y = output_size_logical.h - crate::taskbar::taskbar_height();
                        let bw = crate::taskbar::block_width();
                        let bh = crate::taskbar::block_height();
                        let bsp = crate::taskbar::block_spacing();
                        let block_top = bar_y + (crate::taskbar::taskbar_height() - bh) / 2;
                        let icon_size_px: i32 = crate::taskbar::app_icon_size();
                        let icon_inset = (bw - icon_size_px) / 2;
                        // Ícones primeiro (z-stack fim→começo: ícone na frente).
                        for (i, st) in block_states.iter().enumerate() {
                            let alpha = match st {
                                crate::taskbar::BlockState::Minimized => Some(0.5),
                                _ => None,
                            };
                            let block_x = start_x + i as i32 * (bw + bsp);
                            let icon_x = block_x + icon_inset;
                            let icon_y = block_top + icon_inset;
                            let iloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                    icon_x, icon_y,
                                ))
                                .to_physical_precise_round(1.0);
                            if let Some((icon_buf, src_size)) = taskbar_icons.get(i).and_then(|o| o.as_ref()) {
                                // Ícone real escalado pra 32×32.
                                let src_rect = smithay::utils::Rectangle::<
                                    f64,
                                    smithay::utils::Logical,
                                >::from_size(
                                    smithay::utils::Size::<f64, smithay::utils::Logical>::from((
                                        src_size.w as f64,
                                        src_size.h as f64,
                                    )),
                                );
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer,
                                    iloc.to_f64(),
                                    icon_buf,
                                    alpha,
                                    Some(src_rect),
                                    Some(smithay::utils::Size::from((icon_size_px, icon_size_px))),
                                    Kind::Unspecified,
                                ) {
                                    custom_elements.push(LumaRenderElement::Memory(elem));
                                }
                            } else if let Some(fb) = taskbar_fallbacks.get(i) {
                                // H33.taskbar.polish.1: inicial PRIMEIRO
                                // (= na frente no z-stack). Centralizada no
                                // quadrado 32×32.
                                if let Some((init_buf, init_sz)) =
                                    taskbar_initials.get(i).and_then(|o| o.as_ref())
                                {
                                    let icon_size_px = crate::taskbar::app_icon_size();
                                    let lx = icon_x + (icon_size_px - init_sz.w) / 2;
                                    let ly = icon_y + (icon_size_px - init_sz.h) / 2;
                                    let lloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                            lx, ly,
                                        ))
                                        .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer,
                                        lloc.to_f64(),
                                        init_buf,
                                        alpha,
                                        None,
                                        None,
                                        Kind::Unspecified,
                                    ) {
                                        custom_elements.push(LumaRenderElement::Memory(elem));
                                    }
                                }
                                // Fallback colorido por hash do nome do app.
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer,
                                    iloc.to_f64(),
                                    fb,
                                    alpha,
                                    None,
                                    None,
                                    Kind::Unspecified,
                                ) {
                                    custom_elements.push(LumaRenderElement::Memory(elem));
                                }
                            }
                        }
                        // Bg dos botões (atrás dos ícones). Pula Inactive
                        // (sem bg). Hover e Active têm texturas próprias;
                        // Minimized usa block_minimized.
                        for (i, st) in block_states.iter().enumerate() {
                            let block_tex = match st {
                                crate::taskbar::BlockState::Inactive => continue,
                                crate::taskbar::BlockState::Hover => &block_hover,
                                crate::taskbar::BlockState::Active => &block_active,
                                crate::taskbar::BlockState::Minimized => &block_minimized,
                            };
                            let block_x = start_x + i as i32 * (bw + bsp);
                            let bloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                    block_x, block_top,
                                ))
                                .to_physical_precise_round(1.0);
                            let src_rect = smithay::utils::Rectangle::<
                                f64,
                                smithay::utils::Logical,
                            >::from_size(
                                smithay::utils::Size::<f64, smithay::utils::Logical>::from((
                                    bw as f64,
                                    bh as f64,
                                )),
                            );
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer,
                                bloc.to_f64(),
                                block_tex,
                                None,
                                Some(src_rect),
                                Some(smithay::utils::Size::from((bw, bh))),
                                Kind::Unspecified,
                            ) {
                                custom_elements.push(LumaRenderElement::Memory(elem));
                            }
                        }
                        let _ = &block_inactive; // unused após H33.taskbar.4

                        // H33.taskbar.6: tooltip do app hovered. Pushed
                        // ANTES dos solids (= mais à frente no z-stack)
                        // pra ficar em cima da taskbar + bordas.
                        if let Some((tc, i)) = tooltip_render.as_ref() {
                            let t_theme = &crate::theme::current().taskbar.tooltip;
                            let block_x = start_x + *i as i32 * (bw + bsp);
                            let button_center_x = block_x + bw / 2;
                            let tx = button_center_x - tc.bg_size.w / 2;
                            let ty = block_top - tc.bg_size.h - t_theme.margin_above;
                            // Label primeiro (vai na frente, em cima do bg).
                            let lx = tx + (tc.bg_size.w - tc.label_size.w) / 2;
                            let ly = ty + (tc.bg_size.h - tc.label_size.h) / 2;
                            let lloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                    lx, ly,
                                ))
                                .to_physical_precise_round(1.0);
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer,
                                lloc.to_f64(),
                                &tc.label,
                                None,
                                None,
                                None,
                                Kind::Unspecified,
                            ) {
                                custom_elements.push(LumaRenderElement::Memory(elem));
                            }
                            // Bg do tooltip (atrás do label).
                            let bloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                    tx, ty,
                                ))
                                .to_physical_precise_round(1.0);
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer,
                                bloc.to_f64(),
                                &tc.bg,
                                None,
                                None,
                                None,
                                Kind::Unspecified,
                            ) {
                                custom_elements.push(LumaRenderElement::Memory(elem));
                            }
                        }

                        custom_elements
                            .extend(taskbar_solids.into_iter().map(LumaRenderElement::Solid));

                        // H44.volume.17: live refresh -- enquanto popup aberto,
                        // re-lista devices/streams a cada 1s pra refletir
                        // mudanças externas (ex: outro app mudando volume).
                        if data.state.applet_popup.is_some() {
                            let stale = data
                                .state
                                .audio_last_refresh
                                .map(|t| t.elapsed() >= std::time::Duration::from_secs(1))
                                .unwrap_or(true);
                            if stale {
                                let (sinks, sources) = crate::audio::list_devices();
                                data.state.audio_sinks = sinks;
                                data.state.audio_sources = sources;
                                data.state.audio_streams = crate::audio::list_streams();
                                data.state.audio_last_refresh = Some(std::time::Instant::now());
                            }
                        }
                        // H44.volume.1-4: applet popup com bg gradient +
                        // tabs + slider. Tokens vêm de theme.popup.
                        // Z-stack: insert(0, ...) coloca cada item NO TOPO.
                        // Ordem de insert = ordem REVERSA do z desejado
                        // (último inserido = mais à frente).
                        if let Some(popup) = data.state.applet_popup.clone() {
                            let theme = crate::theme::current();
                            let t_popup = &theme.popup;
                            let t_slider = &t_popup.slider;
                            let o = popup.origin(output_size_logical);
                            let active_tab = popup.volume_tab;
                            let (devices_rect, apps_rect) = popup.tab_rects(output_size_logical);

                            // (1) Bg do popup (gradient + cantos top) -- inserido por ÚLTIMO
                            // = mais ATRÁS no z-stack.
                            let bg = crate::gfx::make_titlebar_bg(
                                popup.size.w,
                                popup.size.h,
                                t_popup.corner_radius,
                                theme.window.titlebar_bg_top,
                                theme.window.titlebar_bg_bottom,
                            );

                            // (2) Indicador da tab ativa (linha âmbar sob o label).
                            let active_rect = if active_tab == crate::popup::VolumeTab::Devices {
                                devices_rect
                            } else {
                                apps_rect
                            };
                            let ind_w = t_popup.tab_indicator_width;
                            let ind_h = t_popup.tab_indicator_height;
                            let ind_x = active_rect.loc.x + (active_rect.size.w - ind_w) / 2;
                            let ind_y = active_rect.loc.y + active_rect.size.h - ind_h - 2;
                            let indicator = crate::gfx::make_rounded_panel(
                                ind_w,
                                ind_h,
                                1,
                                t_popup.tab_indicator_color,
                            );

                            // (3) Labels das tabs rasterizados pela cor por estado.
                            let mut tab_labels: Vec<(MemoryRenderBuffer, i32, i32)> = Vec::new();
                            if let Some(text) = data.state.text.as_ref() {
                                for (label, rect, is_active) in [
                                    ("Dispositivos", devices_rect, active_tab == crate::popup::VolumeTab::Devices),
                                    ("Aplicativos", apps_rect, active_tab == crate::popup::VolumeTab::Applications),
                                ] {
                                    let color = if is_active {
                                        theme.colors.text_primary
                                    } else {
                                        theme.colors.text_secondary
                                    };
                                    let (pixels, w, h) =
                                        text.rasterize_colored(label, t_popup.tab_text_size, color);
                                    if w > 0 && h > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &pixels,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let lx = rect.loc.x + (rect.size.w - w as i32) / 2;
                                        let ly = rect.loc.y + (rect.size.h - h as i32) / 2 - 4;
                                        tab_labels.push((buf, lx, ly));
                                    }
                                }
                            }

                            // Helper local pra push de Memory element.
                            let mk_mem = |buf: &MemoryRenderBuffer,
                                          x: i32,
                                          y: i32,
                                          r: &mut GlesRenderer|
                             -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                        ::from((x, y))
                                        .to_physical_precise_round(1.0);
                                MemoryRenderBufferRenderElement::from_buffer(
                                    r,
                                    loc.to_f64(),
                                    buf,
                                    None,
                                    None,
                                    None,
                                    Kind::Unspecified,
                                )
                                .ok()
                            };

                            // H44.volume.5-12: listas conforme a aba.
                            let (sinks, sources): (
                                Vec<crate::audio::AudioDevice>,
                                Vec<crate::audio::AudioDevice>,
                            ) = if active_tab == crate::popup::VolumeTab::Devices {
                                (
                                    data.state.audio_sinks.clone(),
                                    data.state.audio_sources.clone(),
                                )
                            } else {
                                (Vec::new(), Vec::new())
                            };
                            let streams: Vec<crate::audio::AudioStream> =
                                if active_tab == crate::popup::VolumeTab::Applications {
                                    data.state.audio_streams.clone()
                                } else {
                                    Vec::new()
                                };

                            // Insert ordem: bg (mais atrás), indicator, labels,
                            // sinks (= mais à frente). insert(0) coloca cada
                            // novo no topo do z-stack.
                            if let Some(elem) = mk_mem(&bg, o.x, o.y, renderer) {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            if let Some(elem) = mk_mem(&indicator, ind_x, ind_y, renderer) {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            for (buf, lx, ly) in &tab_labels {
                                if let Some(elem) = mk_mem(buf, *lx, *ly, renderer) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }

                            // Render: 2 blocos (Sinks com header "Saídas",
                            // depois Sources com header "Entradas").
                            let entry_h = 64;
                            let track_w = popup.size.w - t_popup.padding * 2;
                            let track_h = t_slider.track_height;
                            let handle_r = t_slider.handle_radius;

                            // Headers de seção.
                            let render_header =
                                |elems: &mut Vec<LumaRenderElement<GlesRenderer>>,
                                 r: &mut GlesRenderer,
                                 text_r: &crate::text::TextRenderer,
                                 label: &str,
                                 x: i32,
                                 y: i32| {
                                    let (px, w, h) = text_r.rasterize_colored(
                                        label,
                                        11.0,
                                        theme.colors.text_secondary,
                                    );
                                    if w > 0 && h > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        if let Some(elem) = mk_mem(&buf, x, y, r) {
                                            elems.insert(
                                                0,
                                                LumaRenderElement::Memory(elem),
                                            );
                                        }
                                    }
                                };
                            if active_tab == crate::popup::VolumeTab::Devices {
                                if let Some(text_r) = data.state.text.as_ref() {
                                    render_header(
                                        &mut custom_elements,
                                        renderer,
                                        text_r,
                                        "SAÍDAS",
                                        o.x + t_popup.padding,
                                        crate::popup::render_sinks_header_top(
                                            &popup,
                                            output_size_logical,
                                        ),
                                    );
                                    render_header(
                                        &mut custom_elements,
                                        renderer,
                                        text_r,
                                        "ENTRADAS",
                                        o.x + t_popup.padding,
                                        crate::popup::render_sources_header_top(
                                            &popup,
                                            output_size_logical,
                                            sinks.len(),
                                        ),
                                    );
                                }
                            }

                            // Render dos SINKS (block 1).
                            let mut y_cursor = crate::popup::render_sinks_top(
                                &popup,
                                output_size_logical,
                            );
                            // Coleta idx pra index no audio_sinks (cache).
                            for (sink_idx, sink) in sinks.iter().enumerate() {
                                if y_cursor + entry_h > o.y + popup.size.h - t_popup.padding {
                                    break;
                                }
                                let _ = sink_idx;
                                // Ícone mute (esquerda).
                                let mute_icon_name = if sink.muted {
                                    "volume-muted"
                                } else {
                                    "volume-high"
                                };
                                if let Some((ib, isz)) = crate::tray::ensure_icon_sized(
                                    &mut data.state.tray, mute_icon_name, 18,
                                ) {
                                    let ix = o.x + t_popup.padding;
                                    let iy = y_cursor;
                                    let _ = isz;
                                    if let Some(elem) = mk_mem(&ib, ix, iy, renderer) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }

                                // Truncate descrição em ~38 chars pra caber.
                                let mut label_text =
                                    truncate_chars(&sink.description, 32);
                                if sink.is_default {
                                    label_text = format!("★ {}", label_text);
                                }
                                let pct = (sink.volume.clamp(0.0, 1.5) * 100.0) as u32;
                                let pct_text = format!("{}%", pct);

                                // Rasteriza labels (label nome + pct).
                                if let Some(text) = data.state.text.as_ref() {
                                    let color_label = if sink.muted {
                                        theme.colors.text_secondary
                                    } else {
                                        theme.colors.text_primary
                                    };
                                    let (lp, lw, lh) = text.rasterize_colored(
                                        &label_text, 12.0, color_label,
                                    );
                                    if lw > 0 && lh > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &lp,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (lw as i32, lh as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        // Label começa após o ícone (24 + 8 gap).
                                        if let Some(elem) = mk_mem(
                                            &buf,
                                            o.x + t_popup.padding + 32,
                                            y_cursor,
                                            renderer,
                                        ) {
                                            custom_elements.insert(
                                                0,
                                                LumaRenderElement::Memory(elem),
                                            );
                                        }
                                    }
                                    let (pp, pw, ph) = text.rasterize_colored(
                                        &pct_text,
                                        12.0,
                                        theme.colors.text_secondary,
                                    );
                                    if pw > 0 && ph > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &pp,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (pw as i32, ph as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let px = o.x + popup.size.w
                                            - t_popup.padding
                                            - pw as i32;
                                        if let Some(elem) =
                                            mk_mem(&buf, px, y_cursor, renderer)
                                        {
                                            custom_elements.insert(
                                                0,
                                                LumaRenderElement::Memory(elem),
                                            );
                                        }
                                    }
                                }

                                // Slider deste sink. H44.volume.15: escala
                                // pelo boost (max 1.0 ou 1.5).
                                let smax = popup.slider_max();
                                let value = sink.volume.clamp(0.0, smax);
                                let fill_w = ((track_w as f32) * value / smax) as i32;
                                let track_x = o.x + t_popup.padding;
                                let track_y = y_cursor + 22;
                                let track = crate::gfx::make_rounded_panel(
                                    track_w,
                                    track_h,
                                    track_h / 2,
                                    t_slider.track_color,
                                );
                                let fill = crate::gfx::make_rounded_panel(
                                    fill_w.max(1),
                                    track_h,
                                    track_h / 2,
                                    t_slider.fill_color,
                                );
                                let handle = crate::gfx::make_rounded_panel(
                                    handle_r * 2,
                                    handle_r * 2,
                                    handle_r,
                                    t_slider.handle_color,
                                );
                                let handle_cx = track_x + fill_w;
                                let handle_y = track_y + track_h / 2 - handle_r;
                                if let Some(elem) =
                                    mk_mem(&track, track_x, track_y, renderer)
                                {
                                    custom_elements.insert(
                                        0,
                                        LumaRenderElement::Memory(elem),
                                    );
                                }
                                if let Some(elem) =
                                    mk_mem(&fill, track_x, track_y, renderer)
                                {
                                    custom_elements.insert(
                                        0,
                                        LumaRenderElement::Memory(elem),
                                    );
                                }
                                if let Some(elem) = mk_mem(
                                    &handle,
                                    handle_cx - handle_r,
                                    handle_y,
                                    renderer,
                                ) {
                                    custom_elements.insert(
                                        0,
                                        LumaRenderElement::Memory(elem),
                                    );
                                }
                                y_cursor += entry_h;
                            }

                            // Render dos SOURCES (block 2 — entradas/microfones).
                            // H44.volume.9-11. Mesma estrutura dos sinks.
                            let mut y_cursor = crate::popup::render_sources_top(
                                &popup,
                                output_size_logical,
                                sinks.len(),
                            );
                            for sink in &sources {
                                if y_cursor + entry_h > o.y + popup.size.h - t_popup.padding {
                                    break;
                                }
                                let mute_icon_name = if sink.muted {
                                    "volume-muted"
                                } else {
                                    "volume-high"
                                };
                                if let Some((ib, _isz)) = crate::tray::ensure_icon_sized(
                                    &mut data.state.tray, mute_icon_name, 18,
                                ) {
                                    let ix = o.x + t_popup.padding;
                                    let iy = y_cursor;
                                    if let Some(elem) = mk_mem(&ib, ix, iy, renderer) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }
                                let mut label_text =
                                    truncate_chars(&sink.description, 32);
                                if sink.is_default {
                                    label_text = format!("★ {}", label_text);
                                }
                                let pct = (sink.volume.clamp(0.0, 1.5) * 100.0) as u32;
                                let pct_text = format!("{}%", pct);
                                if let Some(text) = data.state.text.as_ref() {
                                    let color_label = if sink.muted {
                                        theme.colors.text_secondary
                                    } else {
                                        theme.colors.text_primary
                                    };
                                    let (lp, lw, lh) =
                                        text.rasterize_colored(&label_text, 12.0, color_label);
                                    if lw > 0 && lh > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &lp,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (lw as i32, lh as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        if let Some(elem) = mk_mem(
                                            &buf,
                                            o.x + t_popup.padding + 32,
                                            y_cursor,
                                            renderer,
                                        ) {
                                            custom_elements
                                                .insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    let (pp, pw, ph) = text.rasterize_colored(
                                        &pct_text,
                                        12.0,
                                        theme.colors.text_secondary,
                                    );
                                    if pw > 0 && ph > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &pp,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (pw as i32, ph as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let px = o.x + popup.size.w
                                            - t_popup.padding
                                            - pw as i32;
                                        if let Some(elem) =
                                            mk_mem(&buf, px, y_cursor, renderer)
                                        {
                                            custom_elements
                                                .insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                }
                                let smax = popup.slider_max();
                                let value = sink.volume.clamp(0.0, smax);
                                let fill_w = ((track_w as f32) * value / smax) as i32;
                                let track_x = o.x + t_popup.padding;
                                let track_y = y_cursor + 22;
                                let track = crate::gfx::make_rounded_panel(
                                    track_w,
                                    track_h,
                                    track_h / 2,
                                    t_slider.track_color,
                                );
                                let fill = crate::gfx::make_rounded_panel(
                                    fill_w.max(1),
                                    track_h,
                                    track_h / 2,
                                    t_slider.fill_color,
                                );
                                let handle = crate::gfx::make_rounded_panel(
                                    handle_r * 2,
                                    handle_r * 2,
                                    handle_r,
                                    t_slider.handle_color,
                                );
                                let handle_cx = track_x + fill_w;
                                let handle_y = track_y + track_h / 2 - handle_r;
                                if let Some(elem) =
                                    mk_mem(&track, track_x, track_y, renderer)
                                {
                                    custom_elements
                                        .insert(0, LumaRenderElement::Memory(elem));
                                }
                                if let Some(elem) =
                                    mk_mem(&fill, track_x, track_y, renderer)
                                {
                                    custom_elements
                                        .insert(0, LumaRenderElement::Memory(elem));
                                }
                                if let Some(elem) = mk_mem(
                                    &handle,
                                    handle_cx - handle_r,
                                    handle_y,
                                    renderer,
                                ) {
                                    custom_elements
                                        .insert(0, LumaRenderElement::Memory(elem));
                                }
                                y_cursor += entry_h;
                            }

                            // Render dos STREAMS na aba Applications.
                            // H44.volume.12-14. Mesma estrutura dos sinks.
                            let mut y_cursor = crate::popup::render_streams_top(
                                &popup,
                                output_size_logical,
                            );
                            for stream in &streams {
                                if y_cursor + entry_h > o.y + popup.size.h - t_popup.padding {
                                    break;
                                }
                                let mute_icon_name = if stream.muted {
                                    "volume-muted"
                                } else {
                                    "volume-high"
                                };
                                if let Some((ib, _isz)) = crate::tray::ensure_icon_sized(
                                    &mut data.state.tray, mute_icon_name, 18,
                                ) {
                                    let ix = o.x + t_popup.padding;
                                    let iy = y_cursor;
                                    if let Some(elem) = mk_mem(&ib, ix, iy, renderer) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }
                                let label_text = truncate_chars(&stream.name, 36);
                                let pct = (stream.volume.clamp(0.0, 1.5) * 100.0) as u32;
                                let pct_text = format!("{}%", pct);
                                if let Some(text) = data.state.text.as_ref() {
                                    let color_label = if stream.muted {
                                        theme.colors.text_secondary
                                    } else {
                                        theme.colors.text_primary
                                    };
                                    let (lp, lw, lh) =
                                        text.rasterize_colored(&label_text, 12.0, color_label);
                                    if lw > 0 && lh > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &lp,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (lw as i32, lh as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        if let Some(elem) = mk_mem(
                                            &buf,
                                            o.x + t_popup.padding + 32,
                                            y_cursor,
                                            renderer,
                                        ) {
                                            custom_elements
                                                .insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    let (pp, pw, ph) = text.rasterize_colored(
                                        &pct_text,
                                        12.0,
                                        theme.colors.text_secondary,
                                    );
                                    if pw > 0 && ph > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &pp,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (pw as i32, ph as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let px = o.x + popup.size.w
                                            - t_popup.padding
                                            - pw as i32;
                                        if let Some(elem) =
                                            mk_mem(&buf, px, y_cursor, renderer)
                                        {
                                            custom_elements
                                                .insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                }
                                let smax = popup.slider_max();
                                let value = stream.volume.clamp(0.0, smax);
                                let fill_w = ((track_w as f32) * value / smax) as i32;
                                let track_x = o.x + t_popup.padding;
                                let track_y = y_cursor + 22;
                                let track = crate::gfx::make_rounded_panel(
                                    track_w,
                                    track_h,
                                    track_h / 2,
                                    t_slider.track_color,
                                );
                                let fill = crate::gfx::make_rounded_panel(
                                    fill_w.max(1),
                                    track_h,
                                    track_h / 2,
                                    t_slider.fill_color,
                                );
                                let handle = crate::gfx::make_rounded_panel(
                                    handle_r * 2,
                                    handle_r * 2,
                                    handle_r,
                                    t_slider.handle_color,
                                );
                                let handle_cx = track_x + fill_w;
                                let handle_y = track_y + track_h / 2 - handle_r;
                                if let Some(elem) =
                                    mk_mem(&track, track_x, track_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                if let Some(elem) =
                                    mk_mem(&fill, track_x, track_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                if let Some(elem) = mk_mem(
                                    &handle,
                                    handle_cx - handle_r,
                                    handle_y,
                                    renderer,
                                ) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                y_cursor += entry_h;
                            }

                            // H44.volume.15-16: toggle "Permitir > 100%" no
                            // canto inferior do popup + aviso coral se algum
                            // device tem volume > 1.0.
                            let toggle_rect = popup.boost_toggle_rect(output_size_logical);
                            let toggle_color = if popup.boost {
                                theme.colors.accent_amber
                            } else {
                                theme.colors.applet_bg_hover
                            };
                            let toggle_bg = crate::gfx::make_rounded_panel(
                                toggle_rect.size.w,
                                toggle_rect.size.h,
                                toggle_rect.size.h / 2,
                                toggle_color,
                            );
                            if let Some(elem) = mk_mem(
                                &toggle_bg,
                                toggle_rect.loc.x,
                                toggle_rect.loc.y,
                                renderer,
                            ) {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            if let Some(text) = data.state.text.as_ref() {
                                let label = if popup.boost {
                                    "★ permitir > 100%"
                                } else {
                                    "  permitir > 100%"
                                };
                                let color = if popup.boost {
                                    theme.colors.taskbar_bg_top
                                } else {
                                    theme.colors.text_primary
                                };
                                let (px, w, h) = text.rasterize_colored(label, 11.0, color);
                                if w > 0 && h > 0 {
                                    let buf = MemoryRenderBuffer::from_slice(
                                        &px,
                                        smithay::backend::allocator::Fourcc::Argb8888,
                                        (w as i32, h as i32),
                                        1,
                                        Transform::Normal,
                                        None,
                                    );
                                    let lx = toggle_rect.loc.x
                                        + (toggle_rect.size.w - w as i32) / 2;
                                    let ly = toggle_rect.loc.y
                                        + (toggle_rect.size.h - h as i32) / 2;
                                    if let Some(elem) = mk_mem(&buf, lx, ly, renderer) {
                                        custom_elements
                                            .insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }
                            }

                            // Aviso coral se algum device/stream > 100%.
                            let any_over = data
                                .state
                                .audio_sinks
                                .iter()
                                .any(|d| d.volume > 1.0)
                                || data.state.audio_sources.iter().any(|d| d.volume > 1.0)
                                || data.state.audio_streams.iter().any(|s| s.volume > 1.0);
                            if any_over {
                                if let Some(text) = data.state.text.as_ref() {
                                    let msg = "Volume acima de 100% pode danificar";
                                    let (px, w, h) =
                                        text.rasterize_colored(msg, 10.0, theme.colors.hover_close);
                                    if w > 0 && h > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let lx = o.x + (popup.size.w - w as i32) / 2;
                                        let ly = toggle_rect.loc.y - h as i32 - 4;
                                        if let Some(elem) = mk_mem(&buf, lx, ly, renderer) {
                                            custom_elements
                                                .insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                }
                            }
                        }

                        // H45.osd: overlay centralizado (volume/brilho). Render
                        // PURO — sem hit-test, sem keyboard focus. Aparece por
                        // cima de TUDO (Smithay renderiza fim→começo, então
                        // insert(0,...) por último vira o topmost). Render
                        // depois do popup faz OSD ficar acima do popup
                        // quando ambos estão ativos — comportamento desejado.
                        //
                        // Auto-dismiss: zera state.osd quando expira (evita
                        // re-checar pra sempre). Próximo tick pula o bloco.
                        let dismiss_ms = crate::theme::current().osd.dismiss_ms;
                        if data
                            .state
                            .osd
                            .as_ref()
                            .map(|o| o.expired(dismiss_ms))
                            .unwrap_or(false)
                        {
                            data.state.osd = None;
                        }
                        let osd_snapshot = data.state.osd.clone();
                        if let Some(osd) = osd_snapshot {
                            let theme = crate::theme::current();
                            let t_osd = &theme.osd;
                            let layout = crate::osd::OsdLayout::compute(
                                output_size_logical.w,
                                output_size_logical.h,
                                t_osd,
                            );

                            // Helper local pra evitar duplicação. Mesma forma
                            // do mk_mem do popup.
                            let osd_mk = |buf: &MemoryRenderBuffer,
                                          x: i32,
                                          y: i32,
                                          r: &mut GlesRenderer|
                             -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                        ::from((x, y))
                                        .to_physical_precise_round(1.0);
                                MemoryRenderBufferRenderElement::from_buffer(
                                    r,
                                    loc.to_f64(),
                                    buf,
                                    None,
                                    None,
                                    None,
                                    Kind::Unspecified,
                                )
                                .ok()
                            };

                            // (1) Pill bg.
                            let bg = crate::gfx::make_rounded_panel(
                                layout.width,
                                layout.height,
                                t_osd.corner_radius,
                                t_osd.bg,
                            );

                            // (2) Bar bg.
                            let bar_bg = crate::gfx::make_rounded_panel(
                                layout.bar_w,
                                layout.bar_h,
                                layout.bar_h / 2,
                                t_osd.bar_bg,
                            );

                            // (3) Bar fill.
                            let fill_color = if osd.muted {
                                t_osd.bar_fill_muted
                            } else if osd.is_boost() {
                                t_osd.bar_fill_boost
                            } else {
                                t_osd.bar_fill
                            };
                            let fill_w =
                                (layout.bar_w as f32 * osd.bar_fraction()).round() as i32;
                            let bar_fill = if fill_w > 0 {
                                Some(crate::gfx::make_rounded_panel(
                                    fill_w,
                                    layout.bar_h,
                                    layout.bar_h / 2,
                                    fill_color,
                                ))
                            } else {
                                None
                            };

                            // (4) Icon SVG (via tray cache pra reusar fetch).
                            let icon_pair = crate::tray::ensure_icon_sized(
                                &mut data.state.tray,
                                osd.icon_name(),
                                layout.icon_size,
                            );

                            // (5) Label "N%".
                            let label_text = format!("{}%", osd.pct());
                            let label_color = if osd.muted {
                                t_osd.bar_fill_muted
                            } else if osd.is_boost() {
                                t_osd.bar_fill_boost
                            } else {
                                t_osd.text_color
                            };
                            let label_buf = data.state.text.as_ref().and_then(|tx| {
                                let (px, w, h) =
                                    tx.rasterize_colored(&label_text, t_osd.text_size, label_color);
                                if w == 0 || h == 0 {
                                    return None;
                                }
                                Some((
                                    MemoryRenderBuffer::from_slice(
                                        &px,
                                        smithay::backend::allocator::Fourcc::Argb8888,
                                        (w as i32, h as i32),
                                        1,
                                        Transform::Normal,
                                        None,
                                    ),
                                    w as i32,
                                    h as i32,
                                ))
                            });

                            // Push em z-stack: bg primeiro (mais atrás), depois
                            // bar_bg, bar_fill, icon, label (mais à frente).
                            if let Some(elem) =
                                osd_mk(&bg, layout.origin_x, layout.origin_y, renderer)
                            {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            if let Some(elem) =
                                osd_mk(&bar_bg, layout.bar_x, layout.bar_y, renderer)
                            {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            if let Some(buf) = bar_fill.as_ref() {
                                if let Some(elem) =
                                    osd_mk(buf, layout.bar_x, layout.bar_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                            if let Some((ibuf, _isz)) = icon_pair.as_ref() {
                                if let Some(elem) =
                                    osd_mk(ibuf, layout.icon_x, layout.icon_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                            if let Some((lbuf, lw, lh)) = label_buf.as_ref() {
                                let lx = layout.label_x_right - lw;
                                let ly = layout.label_center_y - lh / 2;
                                if let Some(elem) = osd_mk(lbuf, lx, ly, renderer) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                        }

                        // H46.osd-media: card now-playing. Estrutura espelha
                        // o H45 (filter expired → snapshot → render). Diferenças:
                        //   * polling de 500ms re-faz playerctl (live update)
                        //   * fade-out via alpha modulation no from_buffer
                        //   * hit-test só nos 3 botões (resto click-through)
                        let t_media = crate::theme::current().osd_media.clone();
                        if data
                            .state
                            .osd_media
                            .as_ref()
                            .map(|m| m.expired(t_media.timeout_ms))
                            .unwrap_or(false)
                        {
                            data.state.osd_media = None;
                        }
                        // Polling enquanto visível.
                        if let Some(m) = data.state.osd_media.as_mut() {
                            if m.last_poll.elapsed() >= std::time::Duration::from_millis(500) {
                                let fresh = crate::media::read_info();
                                // Se trocou de track, reseta timer (re-trigger).
                                let track_changed =
                                    fresh.title != m.info.title || fresh.length != m.info.length;
                                m.info = fresh;
                                m.last_poll = std::time::Instant::now();
                                if track_changed {
                                    m.shown_at = std::time::Instant::now();
                                }
                            }
                        }
                        if let Some(media) = data.state.osd_media.clone() {
                            let theme = crate::theme::current();
                            let t = &theme.osd_media;
                            let layout = crate::osd::MediaOsdLayout::compute(
                                output_size_logical.w,
                                output_size_logical.h,
                                t,
                            );
                            let fade = media.alpha(t.timeout_ms, t.fade_ms);

                            // Helper local com alpha modulation (fade-out).
                            let mk_alpha = |buf: &MemoryRenderBuffer,
                                            x: i32,
                                            y: i32,
                                            r: &mut GlesRenderer|
                             -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                        ::from((x, y))
                                        .to_physical_precise_round(1.0);
                                MemoryRenderBufferRenderElement::from_buffer(
                                    r,
                                    loc.to_f64(),
                                    buf,
                                    Some(fade),
                                    None,
                                    None,
                                    Kind::Unspecified,
                                )
                                .ok()
                            };

                            // (1) Card bg (rounded).
                            let bg = crate::gfx::make_rounded_panel(
                                t.width,
                                t.height,
                                t.corner_radius,
                                t.bg,
                            );

                            // (2) Arte: ou imagem carregada do mpris:artUrl,
                            // ou placeholder de nota musical.
                            let art_buf = if media.info.art_url.is_empty() {
                                None
                            } else {
                                crate::media::load_art(&media.info.art_url, t.art_size)
                            };
                            let placeholder_buf = if art_buf.is_none() {
                                crate::tray::ensure_icon_sized(
                                    &mut data.state.tray,
                                    "media-art-placeholder",
                                    t.art_size,
                                )
                                .map(|(b, _)| b)
                            } else {
                                None
                            };

                            // (3) Texto (title + artist).
                            let title_buf = data.state.text.as_ref().and_then(|tx| {
                                let title = if media.info.title.is_empty() {
                                    "—".to_string()
                                } else {
                                    // H46.media.8: ellipsis pra titulos longos.
                                    truncate_chars(&media.info.title, 32)
                                };
                                let (px, w, h) =
                                    tx.rasterize_colored(&title, t.title_size, t.title_color);
                                if w == 0 || h == 0 {
                                    return None;
                                }
                                Some((
                                    MemoryRenderBuffer::from_slice(
                                        &px,
                                        smithay::backend::allocator::Fourcc::Argb8888,
                                        (w as i32, h as i32),
                                        1,
                                        Transform::Normal,
                                        None,
                                    ),
                                    w as i32,
                                    h as i32,
                                ))
                            });
                            let artist_buf = data.state.text.as_ref().and_then(|tx| {
                                if media.info.artist.is_empty() {
                                    return None;
                                }
                                let artist = truncate_chars(&media.info.artist, 38);
                                let (px, w, h) =
                                    tx.rasterize_colored(&artist, t.artist_size, t.artist_color);
                                if w == 0 || h == 0 {
                                    return None;
                                }
                                Some((
                                    MemoryRenderBuffer::from_slice(
                                        &px,
                                        smithay::backend::allocator::Fourcc::Argb8888,
                                        (w as i32, h as i32),
                                        1,
                                        Transform::Normal,
                                        None,
                                    ),
                                    w as i32,
                                    h as i32,
                                ))
                            });

                            // (4) Botão play/pause: círculo amber + ícone.
                            let play_bg = crate::gfx::make_rounded_panel(
                                layout.play_size,
                                layout.play_size,
                                layout.play_size / 2,
                                t.play_btn_bg,
                            );
                            // Ícone: pause se Playing, play caso contrário.
                            let is_playing = matches!(
                                media.info.status,
                                Some(crate::media::PlaybackStatus::Playing)
                            );
                            let play_icon_name = if is_playing {
                                "media-pause"
                            } else {
                                "media-play"
                            };
                            // Ícone do play/pause em ~60% do botão.
                            let play_icon_size = (layout.play_size as f32 * 0.55) as i32;
                            let play_icon = crate::tray::ensure_icon_sized(
                                &mut data.state.tray,
                                play_icon_name,
                                play_icon_size,
                            );
                            let prev_icon = crate::tray::ensure_icon_sized(
                                &mut data.state.tray,
                                "media-prev",
                                layout.prev_size,
                            );
                            let next_icon = crate::tray::ensure_icon_sized(
                                &mut data.state.tray,
                                "media-next",
                                layout.next_size,
                            );

                            // (5) Progress bar (bg + fill se length > 0).
                            let progress_bg = crate::gfx::make_rounded_panel(
                                layout.progress_w,
                                layout.progress_h,
                                layout.progress_h / 2,
                                t.progress_bg,
                            );
                            let progress_fill = if media.info.length > 0.0 {
                                let cur = media.current_position();
                                let frac = (cur / media.info.length).clamp(0.0, 1.0) as f32;
                                let fw = (layout.progress_w as f32 * frac).round() as i32;
                                if fw > 0 {
                                    Some(crate::gfx::make_rounded_panel(
                                        fw,
                                        layout.progress_h,
                                        layout.progress_h / 2,
                                        t.progress_fill,
                                    ))
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            // Push order: bg (mais atrás) → art → texts → progress bg →
                            // progress fill → play_bg → icons (mais à frente).
                            if let Some(elem) =
                                mk_alpha(&bg, layout.origin_x, layout.origin_y, renderer)
                            {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            if let Some(buf) = art_buf.as_ref() {
                                if let Some(elem) =
                                    mk_alpha(buf, layout.art_x, layout.art_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            } else if let Some(buf) = placeholder_buf.as_ref() {
                                if let Some(elem) =
                                    mk_alpha(buf, layout.art_x, layout.art_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                            if let Some((tbuf, _tw, _th)) = title_buf.as_ref() {
                                if let Some(elem) =
                                    mk_alpha(tbuf, layout.right_col_x, layout.title_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                            if let Some((abuf, _aw, _ah)) = artist_buf.as_ref() {
                                if let Some(elem) =
                                    mk_alpha(abuf, layout.right_col_x, layout.artist_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                            if let Some(elem) = mk_alpha(
                                &progress_bg,
                                layout.progress_x,
                                layout.progress_y,
                                renderer,
                            ) {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            if let Some(fbuf) = progress_fill.as_ref() {
                                if let Some(elem) = mk_alpha(
                                    fbuf,
                                    layout.progress_x,
                                    layout.progress_y,
                                    renderer,
                                ) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                            if let Some(elem) =
                                mk_alpha(&play_bg, layout.play_x, layout.controls_y, renderer)
                            {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            // Centraliza o icon dentro do botão circular.
                            let play_icon_off = (layout.play_size - play_icon_size) / 2;
                            if let Some((ibuf, _isz)) = play_icon.as_ref() {
                                if let Some(elem) = mk_alpha(
                                    ibuf,
                                    layout.play_x + play_icon_off,
                                    layout.controls_y + play_icon_off,
                                    renderer,
                                ) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                            if let Some((ibuf, _isz)) = prev_icon.as_ref() {
                                if let Some(elem) =
                                    mk_alpha(ibuf, layout.prev_x, layout.controls_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                            if let Some((ibuf, _isz)) = next_icon.as_ref() {
                                if let Some(elem) =
                                    mk_alpha(ibuf, layout.next_x, layout.controls_y, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                            }
                        }

                        // Background gradient da taskbar (Coffee→Black, H32.5.3).
                        // Pushed POR ÚLTIMO em custom_elements pra ficar atrás de
                        // todos os solid elements da taskbar (blocos, launcher,
                        // show-desktop) — Smithay renderiza fim→começo do Vec.
                        let taskbar_h = crate::taskbar::taskbar_height();
                        let bar_y = output_size_logical.h - taskbar_h;
                        let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((0, bar_y))
                                .to_physical_precise_round(1.0);
                        let bg_src = smithay::utils::Rectangle::<f64, smithay::utils::Logical>::from_size(
                            smithay::utils::Size::<f64, smithay::utils::Logical>::from((
                                1.0,
                                taskbar_h as f64,
                            )),
                        );
                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                            renderer,
                            bg_loc.to_f64(),
                            &taskbar_bg,
                            None,
                            Some(bg_src),
                            Some(smithay::utils::Size::from((output_size_logical.w, taskbar_h))),
                            Kind::Unspecified,
                        ) {
                            custom_elements.push(LumaRenderElement::Memory(elem));
                        }

                        let render_result = smithay::desktop::space::render_output::<
                            _,
                            LumaRenderElement<GlesRenderer>,
                            _,
                            _,
                        >(
                            &output,
                            renderer,
                            &mut framebuffer,
                            1.0,
                            0,
                            [&data.state.space],
                            &custom_elements,
                            &mut damage_tracker,
                            crate::theme::current().clear,
                        );
                        if let Err(err) = render_result {
                            warn!("render_output falhou: {err}");
                        }
                    }
                    Err(err) => error!("backend.bind falhou: {err}"),
                }

                if let Err(err) = backend.submit(Some(&[damage])) {
                    warn!("submit falhou: {err}");
                }

                let now = data.state.start_time.elapsed();
                data.state.space.elements().for_each(|item| {
                    if let Some(window) = item.as_window() {
                        window.window().send_frame(
                            &output,
                            now,
                            Some(Duration::ZERO),
                            |_, _| Some(output.clone()),
                        );
                    }
                });
                // H36.3: detecta surfaces que morreram e remove do
                // workspace (cleanup_empty roda dentro de remove_window).
                let alive_surfaces: std::collections::HashSet<_> = data
                    .state
                    .space
                    .elements()
                    .filter_map(|i| i.as_window())
                    .filter_map(|w| w.wl_surface())
                    .collect();
                let dead_keys: Vec<_> = data
                    .state
                    .workspaces
                    .workspaces
                    .iter()
                    .flat_map(|ws| ws.windows.iter().cloned())
                    .filter(|k| !alive_surfaces.contains(k))
                    .collect();
                for key in dead_keys {
                    data.state.workspaces.remove_window(&key);
                    data.state.saved_positions.retain(|(k, _)| k != &key);
                }
                data.state.space.refresh();
                data.state.popups.cleanup();
                let _ = data.display.flush_clients();

                backend.window().request_redraw();
            }
            WinitEvent::CloseRequested => {
                info!("janela fechada pelo usuário, encerrando");
                data.state.loop_signal.stop();
            }
        })
        .map_err(|e| format!("inserir source winit falhou: {e}"))?;

    Ok(())
}
