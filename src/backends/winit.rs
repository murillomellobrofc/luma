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
                    let time_str = format!("{:02}:{:02}", now.format("%H"), now.format("%M"));
                    let date_str = format!(
                        "{} {}",
                        now.format("%d"),
                        month_pt_br(now.format("%m").to_string().parse::<u32>().unwrap_or(1))
                    );
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
                // H33.taskbar.9: battery icon + label %, gerados aqui pra
                // evitar borrow conflict no render. Posicionamento final
                // (grupo direito colado ao clock) vem em D.12.
                let battery_render: Option<(
                    MemoryRenderBuffer,
                    smithay::utils::Size<i32, smithay::utils::Logical>,
                    MemoryRenderBuffer,
                    smithay::utils::Size<i32, smithay::utils::Logical>,
                )> = {
                    let info = crate::tray::read_battery();
                    match (info, data.state.text.as_ref()) {
                        (Some(b), Some(text_r)) => {
                            let icon = crate::tray::ensure_battery_icon(&mut data.state.tray, b.capacity);
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
                                ($buf:expr, $px:expr, $py:expr $(,)?) => {{
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
                            let search_focused = data.state.menu_new.focused_zone
                                == crate::menu::state::FocusedZone::Search;
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
                        // H33.taskbar.9: battery icon + label "%" antes do
                        // clock. Posicionamento provisório (à esquerda do
                        // clock atual com gap fixo); D.12 vai re-ancorar
                        // no grupo direito real.
                        if let Some((bat_buf, bat_sz, lbl_buf, lbl_sz)) = battery_render.as_ref() {
                            let t_sys = &crate::theme::current().taskbar.system_tray;
                            let bar_y_local =
                                output_size_logical.h - crate::taskbar::taskbar_height();
                            let icon_y =
                                bar_y_local + (crate::taskbar::taskbar_height() - bat_sz.h) / 2;
                            // Posiciona à esquerda do clock atual: clock vive
                            // em `output_w - show_desktop_width - clock_padding_right`.
                            // Pra D.9 jogo battery_x = clock_anchor - 80 (espaço
                            // pra label %). Vai ser reformulado em D.12.
                            let clock_anchor_x = output_size_logical.w
                                - crate::taskbar::show_desktop_width()
                                - crate::taskbar::clock_padding_right();
                            let label_x = clock_anchor_x - 90;
                            let icon_x = label_x - bat_sz.w - 4;
                            let icon_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                    icon_x, icon_y,
                                ))
                                .to_physical_precise_round(1.0);
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer,
                                icon_loc.to_f64(),
                                bat_buf,
                                None,
                                None,
                                None,
                                Kind::Unspecified,
                            ) {
                                custom_elements.push(LumaRenderElement::Memory(elem));
                            }
                            let label_y =
                                bar_y_local + (crate::taskbar::taskbar_height() - lbl_sz.h) / 2;
                            let label_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                                    label_x, label_y,
                                ))
                                .to_physical_precise_round(1.0);
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer,
                                label_loc.to_f64(),
                                lbl_buf,
                                None,
                                None,
                                None,
                                Kind::Unspecified,
                            ) {
                                custom_elements.push(LumaRenderElement::Memory(elem));
                            }
                            let _ = t_sys;
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
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer,
                                date_loc.to_f64(),
                                date_buf,
                                Some(0.65),
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
                                    let label_y = y + (item_size - size.h) / 2;
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
