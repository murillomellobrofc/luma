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

/// Quebra texto em linhas **medindo a largura real em pixels** via
/// rasterize (fontdue). Layout natural primeiro: cada linha recebe
/// quantas palavras couberem em `max_width_px`. SÓ depois aplica
/// truncation: se o resultado natural tem mais que `max_lines`, mantém
/// as primeiras e adiciona "…" na última (não no meio). Palavra única
/// maior que `max_width_px` é hard-cut por chars.
/// H54.P3.fix: substitui o wrap por char-count que truncava errado.
fn wrap_to_lines_pixels(
    text: &str,
    text_renderer: &crate::text::TextRenderer,
    size: f32,
    color: smithay::backend::renderer::Color32F,
    max_width_px: i32,
    max_lines: usize,
) -> Vec<String> {
    if text.is_empty() || max_lines == 0 || max_width_px <= 0 {
        return Vec::new();
    }
    // Mede a largura em pixels de um texto qualquer.
    let measure = |s: &str| -> i32 {
        if s.is_empty() {
            return 0;
        }
        text_renderer.rasterize_colored(s, size, color).1 as i32
    };
    // Phase 1: word wrap natural por largura.
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut natural: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in &words {
        let candidate = if current.is_empty() {
            (*word).to_string()
        } else {
            format!("{} {}", current, word)
        };
        if measure(&candidate) <= max_width_px {
            current = candidate;
            continue;
        }
        // Não cabe junto com current. Fecha current se tiver algo.
        if !current.is_empty() {
            natural.push(std::mem::take(&mut current));
        }
        // Palavra sozinha cabe?
        if measure(word) <= max_width_px {
            current = (*word).to_string();
            continue;
        }
        // Hard-cut por char.
        let mut acc = String::new();
        for ch in word.chars() {
            let next = format!("{}{}", acc, ch);
            if measure(&next) > max_width_px {
                if !acc.is_empty() {
                    natural.push(acc.clone());
                }
                acc = ch.to_string();
            } else {
                acc = next;
            }
        }
        current = acc;
    }
    if !current.is_empty() {
        natural.push(current);
    }
    // Phase 2: clamp em max_lines com "…" só na última se sobrou.
    if natural.len() <= max_lines {
        return natural;
    }
    let mut clipped: Vec<String> = natural.into_iter().take(max_lines).collect();
    // Última linha visível ganha "…" — encolhe por palavra/char até caber.
    if let Some(last) = clipped.last_mut() {
        loop {
            let candidate = format!("{}…", last);
            if measure(&candidate) <= max_width_px || last.is_empty() {
                *last = candidate;
                break;
            }
            // Remove última palavra OU char se sem espaço.
            if let Some(sp) = last.rfind(' ') {
                last.truncate(sp);
                while last.ends_with(' ') {
                    last.pop();
                }
            } else {
                last.pop();
            }
        }
    }
    clipped
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
    let initial_size = smithay::utils::Size::<i32, smithay::utils::Logical>::from((
        backend.window_size().w,
        backend.window_size().h,
    ));
    if let Some(wp) = data.state.wallpaper.take() {
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

    // H50.icons.3: Camada de ícones do desktop como SpaceItem z_index=5
    // (entre wallpaper e janelas). Scan inicial de ~/Desktop carrega
    // as entradas; polling 2s em icons.7 atualiza enquanto luma roda.
    {
        let layer = std::sync::Arc::new(crate::desktop_icons::DesktopIconsLayer::new());
        layer.set_output_size(initial_size);
        let entries = crate::desktop_icons::scan();
        layer.set_entries(entries.clone());
        // H50.2b.drag.5: aplica positions persistidas no layer.
        layer.set_positions(data.state.desktop_positions.clone());
        data.state.desktop_entries = entries;
        data.state.desktop_last_scan = Some(std::time::Instant::now());
        data.state.space.map_element(
            crate::space_item::SpaceItem::DesktopIcons(layer.clone()),
            smithay::utils::Point::<i32, smithay::utils::Logical>::from((0, 0)),
            false,
        );
        data.state.desktop_icons = Some(layer);
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
                if let Some(layer) = data.state.desktop_icons.as_ref() {
                    layer.set_output_size(smithay::utils::Size::from((size.w, size.h)));
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
                    // H50.icons.7: polling 2s do ~/Desktop. Re-scan e
                    // só atualiza o layer se mudou (path/mtime). Cheap:
                    // read_dir + sort ordena 5-50 entries em <1ms.
                    // H47c.fix: polling do pending_dbus_menu — thread
                    // bloqueante do D-Bus pode demorar. Quando o slot
                    // virar Some, constrói ContextMenu::for_dbusmenu
                    // no cursor original. Timeout (3s) descarta.
                    if let Some(pending) = data.state.pending_dbus_menu.as_ref() {
                        let ready: Option<Vec<crate::sni::DBusMenuItem>> = pending
                            .result
                            .lock()
                            .ok()
                            .and_then(|g| g.clone());
                        if let Some(items) = ready {
                            if items.is_empty() {
                                // Sem items → fallback delegate.
                                crate::sni::context_menu(
                                    &pending.service,
                                    &pending.menu_path,
                                    pending.cursor_x,
                                    pending.cursor_y,
                                );
                            } else {
                                let tokens =
                                    crate::theme::current().context_menu.clone();
                                let mut menu =
                                    crate::context_menu::ContextMenu::for_dbusmenu(
                                        smithay::utils::Point::from((
                                            pending.cursor_x,
                                            pending.cursor_y,
                                        )),
                                        pending.service.clone(),
                                        pending.menu_path.clone(),
                                        items,
                                    );
                                menu.clamp_into(output_size_logical, &tokens);
                                data.state.context_menu = Some(menu);
                            }
                            data.state.pending_dbus_menu = None;
                        } else if pending.expired() {
                            tracing::warn!(
                                "SNI: DBusMenu timeout ({}ms) — app não respondeu",
                                crate::sni::PendingDBusMenu::TIMEOUT_MS
                            );
                            data.state.pending_dbus_menu = None;
                        }
                    }
                    let stale_icons = data
                        .state
                        .desktop_last_scan
                        .map(|t| t.elapsed() >= std::time::Duration::from_secs(2))
                        .unwrap_or(true);
                    if stale_icons {
                        let fresh = crate::desktop_icons::scan();
                        let changed = fresh.len() != data.state.desktop_entries.len()
                            || fresh.iter().zip(data.state.desktop_entries.iter()).any(
                                |(a, b)| a.path != b.path || a.mtime != b.mtime,
                            );
                        if changed {
                            if let Some(layer) = data.state.desktop_icons.as_ref() {
                                layer.set_entries(fresh.clone());
                            }
                            data.state.desktop_entries = fresh;
                        }
                        data.state.desktop_last_scan = Some(std::time::Instant::now());
                    }
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
                // H54.P5: ícone do sino reflete estado de DND.
                let (dnd_enabled, unread_count) = match data.state.notifications.lock() {
                    Ok(g) => (g.dnd_enabled, g.unread_count),
                    Err(_) => (false, 0),
                };
                let bell_name = crate::tray::bell_icon_name(dnd_enabled);
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
                            // H57 hover/active calculados antes do
                            // render dos applets (push_applet_bg vem
                            // depois). Definidos mais abaixo após o
                            // helper — usamos uma versão postergada
                            // chamando inline aqui via closure data.
                            if let Some((bat_buf, bat_sz, lbl_buf, lbl_sz)) = battery_render.as_ref() {
                                push_mem(&mut custom_elements, bat_buf,
                                         bat_icon_x, center_y_for(bat_sz.h), renderer);
                                push_mem(&mut custom_elements, lbl_buf,
                                         bat_label_x, center_y_for(lbl_sz.h), renderer);
                                // H57: cacheia rect do bloco da bateria
                                // (ícone + label) pro click handler.
                                let bar_y_l = output_size_logical.h
                                    - crate::taskbar::taskbar_height();
                                let tb_h = crate::taskbar::taskbar_height();
                                let bat_block_w = bat_label_x + lbl_sz.w - bat_icon_x;
                                data.state.tray.last_battery_rect = Some(
                                    smithay::utils::Rectangle::new(
                                        smithay::utils::Point::from((bat_icon_x, bar_y_l)),
                                        smithay::utils::Size::from((bat_block_w, tb_h)),
                                    ),
                                );
                            }
                            if let Some((buf, sz)) = wifi_render.as_ref() {
                                push_mem(&mut custom_elements, buf,
                                         wifi_x, center_y_for(sz.h), renderer);
                                // H57.2: cacheia rect do wifi pra click.
                                let bar_y_l = output_size_logical.h
                                    - crate::taskbar::taskbar_height();
                                let tb_h = crate::taskbar::taskbar_height();
                                data.state.tray.last_wifi_rect = Some(
                                    smithay::utils::Rectangle::new(
                                        smithay::utils::Point::from((wifi_x, bar_y_l)),
                                        smithay::utils::Size::from((sz.w, tb_h)),
                                    ),
                                );
                            }
                            if let Some((buf, sz)) = bt_render.as_ref() {
                                push_mem(&mut custom_elements, buf,
                                         bt_x, center_y_for(sz.h), renderer);
                                // H57.3: cacheia rect do bluetooth.
                                let bar_y_l = output_size_logical.h
                                    - crate::taskbar::taskbar_height();
                                let tb_h = crate::taskbar::taskbar_height();
                                data.state.tray.last_bt_rect = Some(
                                    smithay::utils::Rectangle::new(
                                        smithay::utils::Point::from((bt_x, bar_y_l)),
                                        smithay::utils::Size::from((sz.w, tb_h)),
                                    ),
                                );
                            }
                            // H55 polish + H57 hover: helper pra desenhar
                            // bg atrás de um ícone do tray. Cor varia
                            // por estado — active (popup aberto) usa
                            // applet_bg_active, hover usa applet_bg_hover.
                            let push_applet_bg = |elems: &mut Vec<LumaRenderElement<GlesRenderer>>,
                                                  icon_x: i32,
                                                  icon_w: i32,
                                                  color: smithay::backend::renderer::Color32F,
                                                  r: &mut GlesRenderer| {
                                let tb_h = crate::taskbar::taskbar_height();
                                let bar_y_l = output_size_logical.h - tb_h;
                                let pad = 6i32;
                                let bg_w = icon_w + pad * 2;
                                let bg_h = (tb_h - 6).max(28);
                                let bg_x = icon_x - pad;
                                let bg_y = bar_y_l + (tb_h - bg_h) / 2;
                                let bg = crate::gfx::make_rounded_panel(
                                    bg_w, bg_h, 8, color,
                                );
                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((bg_x, bg_y))
                                        .to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    r, loc.to_f64(), &bg,
                                    None, None, None,
                                    Kind::Unspecified,
                                ) {
                                    elems.push(LumaRenderElement::Memory(elem));
                                }
                            };
                            let active_color = theme.colors.applet_bg_active;
                            let hover_color = theme.colors.applet_bg_hover;
                            let tray_hover = data.state.tray.hover;
                            let popup_kind = data.state.applet_popup.as_ref()
                                .map(|p| p.kind);
                            let volume_active = popup_kind == Some(crate::popup::AppletKind::Volume);
                            let battery_active = popup_kind == Some(crate::popup::AppletKind::Battery);
                            let wifi_active = popup_kind == Some(crate::popup::AppletKind::Wifi);
                            let bt_active = popup_kind == Some(crate::popup::AppletKind::Bluetooth);
                            let bell_active = match data.state.notifications.lock() {
                                Ok(g) => g.panel_open,
                                Err(_) => false,
                            };
                            // Push bg dos applets que NÃO sejam volume/bell
                            // (esses tem sua própria branch abaixo).
                            // Battery: usa bloco icon+label, anchor no icon.
                            if let Some((_, bat_sz, _, lbl_sz)) = battery_render.as_ref() {
                                let block_w = bat_label_x + lbl_sz.w - bat_icon_x;
                                let _ = bat_sz;
                                if battery_active {
                                    push_applet_bg(&mut custom_elements,
                                                   bat_icon_x, block_w, active_color, renderer);
                                } else if tray_hover == Some(crate::tray::TrayHover::Battery) {
                                    push_applet_bg(&mut custom_elements,
                                                   bat_icon_x, block_w, hover_color, renderer);
                                }
                            }
                            if let Some((_, sz)) = wifi_render.as_ref() {
                                if wifi_active {
                                    push_applet_bg(&mut custom_elements,
                                                   wifi_x, sz.w, active_color, renderer);
                                } else if tray_hover == Some(crate::tray::TrayHover::Wifi) {
                                    push_applet_bg(&mut custom_elements,
                                                   wifi_x, sz.w, hover_color, renderer);
                                }
                            }
                            if let Some((_, sz)) = bt_render.as_ref() {
                                if bt_active {
                                    push_applet_bg(&mut custom_elements,
                                                   bt_x, sz.w, active_color, renderer);
                                } else if tray_hover == Some(crate::tray::TrayHover::Bluetooth) {
                                    push_applet_bg(&mut custom_elements,
                                                   bt_x, sz.w, hover_color, renderer);
                                }
                            }

                            if let Some((buf, sz)) = volume_render.as_ref() {
                                push_mem(&mut custom_elements, buf,
                                         volume_x, center_y_for(sz.h), renderer);
                                if volume_active {
                                    push_applet_bg(&mut custom_elements,
                                                   volume_x, sz.w, active_color, renderer);
                                } else if tray_hover == Some(crate::tray::TrayHover::Volume) {
                                    push_applet_bg(&mut custom_elements,
                                                   volume_x, sz.w, hover_color, renderer);
                                }
                            }
                            if let Some((buf, sz)) = bell_render.as_ref() {
                                let by = center_y_for(sz.h);
                                push_mem(&mut custom_elements, buf,
                                         bell_x, by, renderer);
                                if bell_active {
                                    push_applet_bg(&mut custom_elements,
                                                   bell_x, sz.w, active_color, renderer);
                                } else if tray_hover == Some(crate::tray::TrayHover::Bell) {
                                    push_applet_bg(&mut custom_elements,
                                                   bell_x, sz.w, hover_color, renderer);
                                }
                                // H54.P5: cacheia rect do sino pra click hit-test.
                                data.state.notification_bell_rect =
                                    Some((bell_x, by, sz.w, sz.h));
                                // Badge de não-vistas — círculo coral + número.
                                if unread_count > 0 {
                                    let t_n = &theme.notifications;
                                    let badge_size = t_n.bell_badge_size;
                                    let badge_buf = crate::gfx::make_rounded_panel(
                                        badge_size,
                                        badge_size,
                                        badge_size / 2,
                                        t_n.bell_badge_bg,
                                    );
                                    let badge_x = bell_x + sz.w - badge_size + 2;
                                    let badge_y = by - 2;
                                    push_mem(&mut custom_elements, &badge_buf,
                                             badge_x, badge_y, renderer);
                                    // Texto do badge: 1-9 ou "9+".
                                    if let Some(tr) = data.state.text.as_ref() {
                                        let label = if unread_count > 9 {
                                            "9+".to_string()
                                        } else {
                                            unread_count.to_string()
                                        };
                                        let (px, w, h) = tr.rasterize_colored(
                                            &label,
                                            t_n.bell_badge_font_size,
                                            t_n.bell_badge_text_color,
                                        );
                                        if w > 0 && h > 0 {
                                            let lbl_buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1,
                                                Transform::Normal,
                                                None,
                                            );
                                            let lbl_x = badge_x + (badge_size - w as i32) / 2;
                                            let lbl_y = badge_y + (badge_size - h as i32) / 2;
                                            push_mem(&mut custom_elements, &lbl_buf,
                                                     lbl_x, lbl_y, renderer);
                                        }
                                    }
                                }
                            } else {
                                data.state.notification_bell_rect = None;
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

                        // H48.ctx: menu de contexto do desktop. Render
                        // estilo popup — insert(0, ...) pra ficar na
                        // frente, bg + linha accent + items + (ctx.4)
                        // submenu.
                        if let Some(menu) = data.state.context_menu.clone() {
                            let theme = crate::theme::current();
                            let t_ctx = theme.context_menu.clone();
                            let rect = menu.rect(&t_ctx);

                            // Helper: insert(0) com pos lógica.
                            let render_menu_panel = |
                                elements: &mut Vec<LumaRenderElement<GlesRenderer>>,
                                renderer: &mut GlesRenderer,
                                text_renderer: Option<&crate::text::TextRenderer>,
                                tray: &mut crate::tray::TrayState,
                                items: &[crate::context_menu::ContextItem],
                                rect: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
                                hovered_idx: Option<usize>,
                            | {
                                // (a) bg painel.
                                let bg = crate::gfx::make_rounded_panel(
                                    rect.size.w, rect.size.h,
                                    t_ctx.corner_radius, t_ctx.bg,
                                );
                                let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                        ::from((rect.loc.x, rect.loc.y))
                                        .to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, bg_loc.to_f64(), &bg,
                                    None, None, None, Kind::Unspecified,
                                ) {
                                    elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                // (b) accent top.
                                let accent = crate::gfx::make_rounded_panel(
                                    rect.size.w - 24, 2, 1, t_ctx.top_accent,
                                );
                                let acc_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                        ::from((rect.loc.x + 12, rect.loc.y + 4))
                                        .to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, acc_loc.to_f64(), &accent,
                                    None, None, None, Kind::Unspecified,
                                ) {
                                    elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                // (c) cada item.
                                let mut y = rect.loc.y + 4;
                                for (idx, item) in items.iter().enumerate() {
                                    let h = crate::context_menu::slot_height(item, &t_ctx);
                                    match item.kind {
                                        crate::context_menu::ItemKind::Separator => {
                                            // Linha horizontal centrada.
                                            let sep_w = rect.size.w - 2 * t_ctx.item_padding_horizontal;
                                            let sep = crate::gfx::make_rounded_panel(
                                                sep_w, t_ctx.separator_height, 0, t_ctx.separator,
                                            );
                                            let sep_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>
                                                    ::from((
                                                        rect.loc.x + t_ctx.item_padding_horizontal,
                                                        y + t_ctx.separator_padding,
                                                    ))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, sep_loc.to_f64(), &sep,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                        _ => {
                                            // Hover bg só pra items interactive.
                                            if item.interactive() && hovered_idx == Some(idx) {
                                                let hover = crate::gfx::make_rounded_panel(
                                                    rect.size.w - 8, h, 6, t_ctx.item_hover_bg,
                                                );
                                                let h_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                    smithay::utils::Point::<i32, smithay::utils::Logical>
                                                        ::from((rect.loc.x + 4, y))
                                                        .to_physical_precise_round(1.0);
                                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                    renderer, h_loc.to_f64(), &hover,
                                                    None, None, None, Kind::Unspecified,
                                                ) {
                                                    elements.insert(0, LumaRenderElement::Memory(elem));
                                                }
                                            }
                                            // Ícone (se houver).
                                            let alpha = if item.enabled { 1.0 } else { 0.55 };
                                            let icon_x = rect.loc.x + t_ctx.item_padding_horizontal;
                                            let icon_y = y + (h - t_ctx.icon_size) / 2;
                                            if let Some(name) = item.icon {
                                                if let Some((buf, _)) =
                                                    crate::tray::ensure_icon_sized(tray, name, t_ctx.icon_size)
                                                {
                                                    let ic_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                        smithay::utils::Point::<i32, smithay::utils::Logical>
                                                            ::from((icon_x, icon_y))
                                                            .to_physical_precise_round(1.0);
                                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                        renderer, ic_loc.to_f64(), &buf,
                                                        Some(alpha), None, None, Kind::Unspecified,
                                                    ) {
                                                        elements.insert(0, LumaRenderElement::Memory(elem));
                                                    }
                                                }
                                            }
                                            // Label.
                                            if let Some(text_r) = text_renderer {
                                                let label_color = if item.enabled {
                                                    t_ctx.item_text
                                                } else {
                                                    t_ctx.item_disabled
                                                };
                                                let (pixels, w, hh) = text_r.rasterize_colored(
                                                    &item.label, t_ctx.item_text_size, label_color,
                                                );
                                                if w > 0 && hh > 0 {
                                                    let buf = MemoryRenderBuffer::from_slice(
                                                        &pixels,
                                                        smithay::backend::allocator::Fourcc::Argb8888,
                                                        (w as i32, hh as i32), 1,
                                                        Transform::Normal, None,
                                                    );
                                                    let lx = icon_x + t_ctx.icon_size + t_ctx.icon_text_gap;
                                                    let ly = y + (h - hh as i32) / 2;
                                                    let l_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                        smithay::utils::Point::<i32, smithay::utils::Logical>
                                                            ::from((lx, ly))
                                                            .to_physical_precise_round(1.0);
                                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                        renderer, l_loc.to_f64(), &buf,
                                                        None, None, None, Kind::Unspecified,
                                                    ) {
                                                        elements.insert(0, LumaRenderElement::Memory(elem));
                                                    }
                                                }
                                                // Shortcut (cinza, à direita) ou
                                                // setinha de submenu.
                                                let right_label: Option<String> = match (&item.kind, item.shortcut) {
                                                    (crate::context_menu::ItemKind::Submenu(_), _) => Some("›".to_string()),
                                                    (_, Some(s)) => Some(s.to_string()),
                                                    _ => None,
                                                };
                                                if let Some(s) = right_label {
                                                    let (px2, w2, h2) = text_r.rasterize_colored(
                                                        &s, t_ctx.item_text_size, t_ctx.shortcut_color,
                                                    );
                                                    if w2 > 0 && h2 > 0 {
                                                        let buf2 = MemoryRenderBuffer::from_slice(
                                                            &px2,
                                                            smithay::backend::allocator::Fourcc::Argb8888,
                                                            (w2 as i32, h2 as i32), 1,
                                                            Transform::Normal, None,
                                                        );
                                                        let rx = rect.loc.x + rect.size.w
                                                            - t_ctx.item_padding_horizontal - w2 as i32;
                                                        let ry = y + (h - h2 as i32) / 2;
                                                        let r_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                            smithay::utils::Point::<i32, smithay::utils::Logical>
                                                                ::from((rx, ry))
                                                                .to_physical_precise_round(1.0);
                                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                            renderer, r_loc.to_f64(), &buf2,
                                                            None, None, None, Kind::Unspecified,
                                                        ) {
                                                            elements.insert(0, LumaRenderElement::Memory(elem));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    y += h;
                                }
                            };

                            render_menu_panel(
                                &mut custom_elements, renderer,
                                data.state.text.as_ref(),
                                &mut data.state.tray,
                                &menu.items, rect, menu.hovered,
                            );

                            // (d) Submenu, se aberto. H48.ctx.4.
                            if let Some(sub_rect) = menu.submenu_rect(&t_ctx, output_size_logical) {
                                if let Some(parent) = menu
                                    .open_submenu
                                    .and_then(|i| menu.items.get(i))
                                {
                                    if let crate::context_menu::ItemKind::Submenu(sub_items) = &parent.kind {
                                        render_menu_panel(
                                            &mut custom_elements, renderer,
                                            data.state.text.as_ref(),
                                            &mut data.state.tray,
                                            sub_items, sub_rect, menu.submenu_hovered,
                                        );
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
                            // H55: cacheia rect da zona do relógio pra
                            // hit-test do clique que abre o calendário,
                            // e como anchor (borda direita) do popup.
                            // Bounding box dos 2 labels com hit target
                            // = altura inteira da taskbar.
                            {
                                let zone_w = time_size.w.max(date_size.w);
                                let bar_y = output_size_logical.h
                                    - crate::taskbar::taskbar_height();
                                let zone_x = time_pt.x.min(date_pt.x);
                                let zone_w = (time_pt.x + time_size.w)
                                    .max(date_pt.x + date_size.w) - zone_x;
                                let _ = zone_w;
                                let right_edge = (time_pt.x + time_size.w)
                                    .max(date_pt.x + date_size.w);
                                let left_edge = time_pt.x.min(date_pt.x);
                                data.state.tray.last_clock_rect = Some(
                                    smithay::utils::Rectangle::new(
                                        smithay::utils::Point::from((left_edge, bar_y)),
                                        smithay::utils::Size::from((
                                            right_edge - left_edge,
                                            crate::taskbar::taskbar_height(),
                                        )),
                                    ),
                                );
                            }
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
                            // H55: clock "acende" na taskbar quando o
                            // popup do calendário está aberto. Bg
                            // applet_bg_active com folga lateral pra
                            // hit target visual ~40px. Push depois dos
                            // labels = atrás deles (end of vec = back).
                            // H57 hover: também acende ao hover (com
                            // applet_bg_hover ao invés de _active).
                            let calendar_open_visual =
                                data.state.applet_popup.as_ref()
                                    .map(|p| p.kind == crate::popup::AppletKind::Calendar)
                                    .unwrap_or(false);
                            let clock_hover_visual = !calendar_open_visual
                                && data.state.tray.hover == Some(crate::tray::TrayHover::Clock);
                            if calendar_open_visual || clock_hover_visual {
                                let th = crate::theme::current();
                                let tb_h = crate::taskbar::taskbar_height();
                                let pad_h = 10i32;
                                let right_edge = (time_pt.x + time_size.w)
                                    .max(date_pt.x + date_size.w);
                                let left_edge = time_pt.x.min(date_pt.x);
                                let zone_w = right_edge - left_edge;
                                let bg_w = zone_w + pad_h * 2;
                                let bg_h = (tb_h - 6).max(28);
                                let bg_x = left_edge - pad_h;
                                let bg_y = (output_size_logical.h - tb_h)
                                    + (tb_h - bg_h) / 2;
                                let bg_color = if calendar_open_visual {
                                    th.colors.applet_bg_active
                                } else {
                                    th.colors.applet_bg_hover
                                };
                                let bg = crate::gfx::make_rounded_panel(
                                    bg_w, bg_h, 8, bg_color,
                                );
                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((bg_x, bg_y))
                                        .to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, loc.to_f64(), &bg,
                                    None, None, None,
                                    Kind::Unspecified,
                                ) {
                                    custom_elements.push(LumaRenderElement::Memory(elem));
                                }
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
                        // H57.3: live refresh do Bluetooth enquanto
                        // popup aberto (~2s) + observer do outbox.
                        if data.state.applet_popup.as_ref().map(|p| p.kind)
                            == Some(crate::popup::AppletKind::Bluetooth)
                        {
                            let stale_bt = data
                                .state
                                .bt_last_refresh
                                .map(|t| t.elapsed() >= std::time::Duration::from_secs(2))
                                .unwrap_or(true);
                            if stale_bt {
                                data.state.bt_state = crate::bluetooth::read_state(
                                    data.state.bt_pair_mode,
                                );
                                data.state.bt_last_refresh = Some(std::time::Instant::now());
                            }
                        }
                        let bt_outcome = data
                            .state
                            .bt_op_outbox
                            .lock()
                            .ok()
                            .and_then(|mut g| g.take());
                        if let Some(outcome) = bt_outcome {
                            data.state.bt_op_in_flight = None;
                            match outcome {
                                crate::bluetooth::BtResult::Success { .. } => {
                                    data.state.bt_error_for = None;
                                    data.state.bt_state = crate::bluetooth::read_state(
                                        data.state.bt_pair_mode,
                                    );
                                    data.state.bt_last_refresh = Some(std::time::Instant::now());
                                }
                                crate::bluetooth::BtResult::Failure { mac, message, .. } => {
                                    data.state.bt_error_for = Some((mac, message));
                                }
                            }
                        }
                        // H57.2: live refresh do Wi-Fi enquanto popup
                        // aberto (~2s) + observer do outbox de connect.
                        if data.state.applet_popup.as_ref().map(|p| p.kind)
                            == Some(crate::popup::AppletKind::Wifi)
                        {
                            let stale_wifi = data
                                .state
                                .wifi_last_refresh
                                .map(|t| t.elapsed() >= std::time::Duration::from_secs(2))
                                .unwrap_or(true);
                            if stale_wifi {
                                data.state.wifi_state = crate::wifi::read_state();
                                data.state.wifi_last_refresh = Some(std::time::Instant::now());
                            }
                        }
                        // Observer do outbox de connect — sempre, mesmo
                        // se o popup fechou no meio.
                        let outcome = data
                            .state
                            .wifi_connect_outbox
                            .lock()
                            .ok()
                            .and_then(|mut g| g.take());
                        if let Some(outcome) = outcome {
                            match outcome {
                                crate::wifi::ConnectResult::Success { ssid } => {
                                    tracing::info!("wifi connected to {ssid}");
                                    data.state.wifi_connecting = None;
                                    data.state.wifi_password_for = None;
                                    data.state.wifi_password_text.clear();
                                    data.state.wifi_password_cursor = 0;
                                    data.state.wifi_error_for = None;
                                    // Refresh imediato.
                                    data.state.wifi_state = crate::wifi::read_state();
                                    data.state.wifi_last_refresh = Some(std::time::Instant::now());
                                }
                                crate::wifi::ConnectResult::Failure { ssid, message } => {
                                    data.state.wifi_connecting = None;
                                    data.state.wifi_error_for = Some((ssid, message));
                                }
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
                            let o = popup.origin(output_size_logical);
                            // H57.3: branch Bluetooth. Mesma forma do
                            // Wi-Fi (toggle + list + footer botão).
                            // Pair mode troca a lista pra descobertos.
                            if popup.kind == crate::popup::AppletKind::Bluetooth {
                                let t_wp = &theme.wifi_popup;
                                // Bg gradient (shell compartilhada).
                                let bg = crate::gfx::make_titlebar_bg(
                                    popup.size.w, popup.size.h,
                                    t_popup.corner_radius,
                                    theme.window.titlebar_bg_top,
                                    theme.window.titlebar_bg_bottom,
                                );
                                let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    o.to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, bg_loc.to_f64(), &bg,
                                    None, None, None, Kind::Unspecified,
                                ) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                let text_r_ref = data.state.text.as_ref();
                                let inner_x = o.x + t_wp.padding;
                                let inner_w = popup.size.w - t_wp.padding * 2;
                                let bt_has_hw = data.state.bt_state.has_hardware;
                                let bt_on = data.state.bt_state.enabled;
                                let pair_mode = data.state.bt_pair_mode;
                                // Header: "Bluetooth" / "Procurando…" + toggle.
                                let header_text = if pair_mode {
                                    "Procurando…"
                                } else {
                                    "Bluetooth"
                                };
                                if let Some(tr) = text_r_ref {
                                    let (px, w, h) = tr.rasterize_colored(
                                        header_text, t_wp.header_size, t_wp.header_color,
                                    );
                                    if w > 0 && h > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1, Transform::Normal, None,
                                        );
                                        let ly = o.y + (t_wp.header_height - h as i32) / 2;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((inner_x, ly))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                }
                                // Toggle só se tem hw E não está em pair_mode.
                                if bt_has_hw && !pair_mode {
                                    let toggle_x = o.x + popup.size.w
                                        - t_wp.padding - t_wp.toggle_w;
                                    let toggle_y = o.y + (t_wp.header_height - t_wp.toggle_h) / 2;
                                    let toggle_bg = crate::gfx::make_rounded_panel(
                                        t_wp.toggle_w, t_wp.toggle_h, t_wp.toggle_corner,
                                        if bt_on { t_wp.toggle_on_bg } else { t_wp.toggle_off_bg },
                                    );
                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((toggle_x, toggle_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, loc.to_f64(), &toggle_bg,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    let knob_r = (t_wp.toggle_h - 6) / 2;
                                    let knob_d = knob_r * 2;
                                    let knob_y = toggle_y + 3;
                                    let knob_x = if bt_on {
                                        toggle_x + t_wp.toggle_w - knob_d - 3
                                    } else {
                                        toggle_x + 3
                                    };
                                    let knob = crate::gfx::make_rounded_panel(
                                        knob_d, knob_d, knob_r, t_wp.toggle_knob_color,
                                    );
                                    let kloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((knob_x, knob_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, kloc.to_f64(), &knob,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    data.state.bt_toggle_rect = Some(
                                        smithay::utils::Rectangle::new(
                                            smithay::utils::Point::from((toggle_x, toggle_y)),
                                            smithay::utils::Size::from((t_wp.toggle_w, t_wp.toggle_h)),
                                        ),
                                    );
                                } else {
                                    data.state.bt_toggle_rect = None;
                                }
                                let header_bot = o.y + t_wp.header_height;
                                let div = crate::gfx::make_rounded_panel(
                                    popup.size.w, 1, 0, t_wp.divider_color,
                                );
                                let dloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((o.x, header_bot))
                                        .to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, dloc.to_f64(), &div,
                                    None, None, None, Kind::Unspecified,
                                ) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                // Reserva footer pra botão (pair ou voltar).
                                let footer_h = t_wp.pw_btn_height + t_wp.padding * 2;
                                let list_top = header_bot + 4;
                                let list_bot = o.y + popup.size.h - footer_h;
                                let mut cur_y = list_top;
                                data.state.bt_row_rects.clear();
                                if !bt_has_hw {
                                    if let Some(tr) = text_r_ref {
                                        let (px, w, h) = tr.rasterize_colored(
                                            "Sem controlador Bluetooth",
                                            t_wp.empty_label_size,
                                            t_wp.empty_label_color,
                                        );
                                        if w > 0 && h > 0 {
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1, Transform::Normal, None,
                                            );
                                            let lx = inner_x + (inner_w - w as i32) / 2;
                                            let ly = (list_top + list_bot) / 2 - h as i32 / 2;
                                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, loc.to_f64(), &buf,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                    }
                                } else if !bt_on {
                                    if let Some(tr) = text_r_ref {
                                        let (px, w, h) = tr.rasterize_colored(
                                            "Bluetooth desligado",
                                            t_wp.empty_label_size,
                                            t_wp.empty_label_color,
                                        );
                                        if w > 0 && h > 0 {
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1, Transform::Normal, None,
                                            );
                                            let lx = inner_x + (inner_w - w as i32) / 2;
                                            let ly = (list_top + list_bot) / 2 - h as i32 / 2;
                                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, loc.to_f64(), &buf,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                    }
                                } else {
                                    // Lista de devices. Em pair_mode lista
                                    // todos (paired + descobertos). Senão
                                    // só os paired (já filtrado pelo
                                    // read_state).
                                    let devices = data.state.bt_state.devices.clone();
                                    for dev in devices.iter() {
                                        if cur_y + t_wp.row_height > list_bot {
                                            break;
                                        }
                                        let row_x = o.x;
                                        let row_w = popup.size.w;
                                        let row_y = cur_y;
                                        let name_color = if dev.connected {
                                            t_wp.ssid_connected_color
                                        } else {
                                            t_wp.ssid_color
                                        };
                                        // Glyph type + nome.
                                        if let Some(tr) = text_r_ref {
                                            let display = format!("{} {}", dev.icon_kind.glyph(), dev.name);
                                            let max_w = row_w
                                                - t_wp.row_padding_h * 2 - 100;
                                            let lines = wrap_to_lines_pixels(
                                                &display, tr, t_wp.ssid_size,
                                                name_color, max_w, 1,
                                            );
                                            if let Some(line) = lines.first() {
                                                let (px, w, h) = tr.rasterize_colored(
                                                    line, t_wp.ssid_size, name_color,
                                                );
                                                if w > 0 && h > 0 {
                                                    let buf = MemoryRenderBuffer::from_slice(
                                                        &px,
                                                        smithay::backend::allocator::Fourcc::Argb8888,
                                                        (w as i32, h as i32),
                                                        1, Transform::Normal, None,
                                                    );
                                                    let lx = row_x + t_wp.row_padding_h;
                                                    let ly = row_y + (t_wp.row_height - h as i32) / 2;
                                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                            .to_physical_precise_round(1.0);
                                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                        renderer, loc.to_f64(), &buf,
                                                        None, None, None, Kind::Unspecified,
                                                    ) {
                                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                    }
                                                }
                                            }
                                            // Estado à direita.
                                            let in_flight = data.state.bt_op_in_flight.as_deref() == Some(&dev.mac);
                                            let right = if in_flight {
                                                "…".to_string()
                                            } else if dev.connected {
                                                "conectado".to_string()
                                            } else if dev.paired {
                                                "pareado".to_string()
                                            } else {
                                                "novo".to_string()
                                            };
                                            let right_color = if dev.connected {
                                                t_wp.state_label_color
                                            } else {
                                                t_wp.signal_color
                                            };
                                            let (px, w, h) = tr.rasterize_colored(
                                                &right, t_wp.state_label_size, right_color,
                                            );
                                            if w > 0 && h > 0 {
                                                let buf = MemoryRenderBuffer::from_slice(
                                                    &px,
                                                    smithay::backend::allocator::Fourcc::Argb8888,
                                                    (w as i32, h as i32),
                                                    1, Transform::Normal, None,
                                                );
                                                let lx = row_x + row_w
                                                    - t_wp.row_padding_h - w as i32;
                                                let ly = row_y + (t_wp.row_height - h as i32) / 2;
                                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                        .to_physical_precise_round(1.0);
                                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                    renderer, loc.to_f64(), &buf,
                                                    None, None, None, Kind::Unspecified,
                                                ) {
                                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                }
                                            }
                                        }
                                        data.state.bt_row_rects.push((
                                            dev.mac.clone(),
                                            smithay::utils::Rectangle::new(
                                                smithay::utils::Point::from((row_x, row_y)),
                                                smithay::utils::Size::from((row_w, t_wp.row_height)),
                                            ),
                                        ));
                                        cur_y += t_wp.row_height;
                                        // Erro inline (se houver pra esse mac).
                                        if let Some((emac, msg)) = data.state.bt_error_for.clone() {
                                            if emac == dev.mac {
                                                if let Some(tr) = text_r_ref {
                                                    let (px, w, h) = tr.rasterize_colored(
                                                        &msg, t_wp.state_label_size,
                                                        t_wp.pw_error_color,
                                                    );
                                                    if w > 0 && h > 0 {
                                                        let buf = MemoryRenderBuffer::from_slice(
                                                            &px,
                                                            smithay::backend::allocator::Fourcc::Argb8888,
                                                            (w as i32, h as i32),
                                                            1, Transform::Normal, None,
                                                        );
                                                        let lx = row_x + t_wp.row_padding_h;
                                                        let ly = cur_y;
                                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                                .to_physical_precise_round(1.0);
                                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                            renderer, loc.to_f64(), &buf,
                                                            None, None, None, Kind::Unspecified,
                                                        ) {
                                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                        }
                                                        cur_y += h as i32 + 4;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Footer botão: "Parear novo dispositivo"
                                // ou "Voltar" no pair_mode.
                                data.state.bt_pair_btn_rect = None;
                                data.state.bt_back_btn_rect = None;
                                if bt_has_hw && bt_on {
                                    let btn_label = if pair_mode {
                                        "Voltar"
                                    } else {
                                        "Parear novo dispositivo"
                                    };
                                    if let Some(tr) = text_r_ref {
                                        let (px, w, h) = tr.rasterize_colored(
                                            btn_label, t_wp.pw_btn_label_size, t_wp.pw_btn_fg,
                                        );
                                        let btn_w = inner_w;
                                        let btn_x = inner_x;
                                        let btn_y = o.y + popup.size.h
                                            - t_wp.padding - t_wp.pw_btn_height;
                                        let btn_bg = crate::gfx::make_rounded_panel(
                                            btn_w, t_wp.pw_btn_height, t_wp.pw_btn_corner,
                                            t_wp.pw_btn_bg,
                                        );
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((btn_x, btn_y))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &btn_bg,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                        if w > 0 && h > 0 {
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1, Transform::Normal, None,
                                            );
                                            let lx = btn_x + (btn_w - w as i32) / 2;
                                            let ly = btn_y + (t_wp.pw_btn_height - h as i32) / 2;
                                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, loc.to_f64(), &buf,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                        let rect = smithay::utils::Rectangle::new(
                                            smithay::utils::Point::from((btn_x, btn_y)),
                                            smithay::utils::Size::from((btn_w, t_wp.pw_btn_height)),
                                        );
                                        if pair_mode {
                                            data.state.bt_back_btn_rect = Some(rect);
                                        } else {
                                            data.state.bt_pair_btn_rect = Some(rect);
                                        }
                                    }
                                }
                            } else if popup.kind == crate::popup::AppletKind::Wifi {
                                let t_wp = &theme.wifi_popup;
                                // Bg gradient.
                                let bg = crate::gfx::make_titlebar_bg(
                                    popup.size.w, popup.size.h,
                                    t_popup.corner_radius,
                                    theme.window.titlebar_bg_top,
                                    theme.window.titlebar_bg_bottom,
                                );
                                let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    o.to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, bg_loc.to_f64(), &bg,
                                    None, None, None, Kind::Unspecified,
                                ) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                let text_r_ref = data.state.text.as_ref();
                                let inner_x = o.x + t_wp.padding;
                                let inner_w = popup.size.w - t_wp.padding * 2;
                                // Header: "Wi-Fi" à esquerda + toggle à direita.
                                if let Some(tr) = text_r_ref {
                                    let (px, w, h) = tr.rasterize_colored(
                                        "Wi-Fi", t_wp.header_size, t_wp.header_color,
                                    );
                                    if w > 0 && h > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1, Transform::Normal, None,
                                        );
                                        let ly = o.y + (t_wp.header_height - h as i32) / 2;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((inner_x, ly))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                }
                                // Toggle: pill rounded + knob.
                                // H57.2 fix: só renderiza se tem placa
                                // wifi. Sem placa, popup serve só de
                                // exibição da conexão cabeada.
                                let wifi_has_hw = data.state.wifi_state.has_hardware;
                                let toggle_x = o.x + popup.size.w
                                    - t_wp.padding - t_wp.toggle_w;
                                let toggle_y = o.y + (t_wp.header_height - t_wp.toggle_h) / 2;
                                let wifi_on = data.state.wifi_state.enabled;
                                if !wifi_has_hw {
                                    data.state.wifi_toggle_rect = None;
                                }
                                let toggle_bg = if wifi_has_hw {
                                    crate::gfx::make_rounded_panel(
                                        t_wp.toggle_w, t_wp.toggle_h, t_wp.toggle_corner,
                                        if wifi_on { t_wp.toggle_on_bg } else { t_wp.toggle_off_bg },
                                    )
                                } else {
                                    crate::gfx::make_rounded_panel(1, 1, 0,
                                        smithay::backend::renderer::Color32F::new(0.0, 0.0, 0.0, 0.0))
                                };
                                if wifi_has_hw {
                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((toggle_x, toggle_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, loc.to_f64(), &toggle_bg,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    let knob_r = (t_wp.toggle_h - 6) / 2;
                                    let knob_d = knob_r * 2;
                                    let knob_y = toggle_y + 3;
                                    let knob_x = if wifi_on {
                                        toggle_x + t_wp.toggle_w - knob_d - 3
                                    } else {
                                        toggle_x + 3
                                    };
                                    let knob = crate::gfx::make_rounded_panel(
                                        knob_d, knob_d, knob_r, t_wp.toggle_knob_color,
                                    );
                                    let kloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((knob_x, knob_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, kloc.to_f64(), &knob,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    data.state.wifi_toggle_rect = Some(
                                        smithay::utils::Rectangle::new(
                                            smithay::utils::Point::from((toggle_x, toggle_y)),
                                            smithay::utils::Size::from((t_wp.toggle_w, t_wp.toggle_h)),
                                        ),
                                    );
                                }
                                let header_bot = o.y + t_wp.header_height;
                                // Divisor.
                                let div = crate::gfx::make_rounded_panel(
                                    popup.size.w, 1, 0, t_wp.divider_color,
                                );
                                let dloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((o.x, header_bot))
                                        .to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, dloc.to_f64(), &div,
                                    None, None, None, Kind::Unspecified,
                                ) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                let mut cur_y = header_bot + 4;
                                data.state.wifi_row_rects.clear();
                                data.state.wifi_connect_btn_rect = None;
                                // H57.2: row de Cabeado no topo se conectado.
                                // Mesma estética da rede conectada (sage),
                                // mas não clicável.
                                if let Some(eth) = data.state.wifi_state.ethernet.clone() {
                                    let row_x = o.x;
                                    let row_w = popup.size.w;
                                    let row_y = cur_y;
                                    if let Some(tr) = text_r_ref {
                                        // SSID-like: "Cabeado" + device discreto.
                                        let label = format!("Cabeado · {}", eth.device);
                                        let (px, w, h) = tr.rasterize_colored(
                                            &label, t_wp.ssid_size, t_wp.ssid_connected_color,
                                        );
                                        if w > 0 && h > 0 {
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1, Transform::Normal, None,
                                            );
                                            let lx = row_x + t_wp.row_padding_h;
                                            let ly = row_y + (t_wp.row_height - h as i32) / 2;
                                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, loc.to_f64(), &buf,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                        // "conectado" à direita.
                                        let (px, w, h) = tr.rasterize_colored(
                                            "conectado", t_wp.state_label_size, t_wp.state_label_color,
                                        );
                                        if w > 0 && h > 0 {
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1, Transform::Normal, None,
                                            );
                                            let lx = row_x + row_w
                                                - t_wp.row_padding_h - w as i32;
                                            let ly = row_y + (t_wp.row_height - h as i32) / 2;
                                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, loc.to_f64(), &buf,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                    }
                                    cur_y += t_wp.row_height;
                                    // Divisor fino entre cabeado e lista wifi.
                                    let div = crate::gfx::make_rounded_panel(
                                        row_w - t_wp.row_padding_h * 2,
                                        1, 0, t_wp.divider_color,
                                    );
                                    let dloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((row_x + t_wp.row_padding_h, cur_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, dloc.to_f64(), &div,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    cur_y += 8;
                                }
                                if !wifi_has_hw {
                                    // Sem placa wifi: popup é só pro
                                    // ethernet. Skip seção wifi inteira.
                                } else if !wifi_on {
                                    // Estado vazio.
                                    if let Some(tr) = text_r_ref {
                                        let (px, w, h) = tr.rasterize_colored(
                                            "Wi-Fi desligado",
                                            t_wp.empty_label_size,
                                            t_wp.empty_label_color,
                                        );
                                        if w > 0 && h > 0 {
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1, Transform::Normal, None,
                                            );
                                            let lx = inner_x + (inner_w - w as i32) / 2;
                                            let ly = cur_y + (popup.size.h
                                                - t_wp.header_height) / 2 - h as i32 / 2;
                                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, loc.to_f64(), &buf,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                    }
                                } else {
                                    // Lista de redes. Conectada primeiro (já ordenada por
                                    // read_state). Cada row é uma linha clicável.
                                    let networks = data.state.wifi_state.networks.clone();
                                    let max_y = o.y + popup.size.h - t_wp.padding;
                                    for net in networks.iter() {
                                        if cur_y + t_wp.row_height > max_y {
                                            break;
                                        }
                                        let row_x = o.x;
                                        let row_w = popup.size.w;
                                        let row_y = cur_y;
                                        // Cor por estado.
                                        let ssid_color = if net.in_use {
                                            t_wp.ssid_connected_color
                                        } else {
                                            t_wp.ssid_color
                                        };
                                        let signal_color = if net.in_use {
                                            t_wp.signal_connected_color
                                        } else {
                                            t_wp.signal_color
                                        };
                                        // SSID (truncado se largo).
                                        if let Some(tr) = text_r_ref {
                                            let max_ssid_w = row_w
                                                - t_wp.row_padding_h * 2 - 80;
                                            let lines = wrap_to_lines_pixels(
                                                &net.ssid, tr, t_wp.ssid_size,
                                                ssid_color, max_ssid_w, 1,
                                            );
                                            if let Some(line) = lines.first() {
                                                let (px, w, h) = tr.rasterize_colored(
                                                    line, t_wp.ssid_size, ssid_color,
                                                );
                                                if w > 0 && h > 0 {
                                                    let buf = MemoryRenderBuffer::from_slice(
                                                        &px,
                                                        smithay::backend::allocator::Fourcc::Argb8888,
                                                        (w as i32, h as i32),
                                                        1, Transform::Normal, None,
                                                    );
                                                    let lx = row_x + t_wp.row_padding_h;
                                                    let ly = row_y + (t_wp.row_height - h as i32) / 2;
                                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                            .to_physical_precise_round(1.0);
                                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                        renderer, loc.to_f64(), &buf,
                                                        None, None, None, Kind::Unspecified,
                                                    ) {
                                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                    }
                                                }
                                            }
                                            // À direita: "conectado" se in_use, ou
                                            // sinal + cadeado.
                                            let right_label = if net.in_use {
                                                "conectado".to_string()
                                            } else if net.is_protected() {
                                                format!("{}% 🔒", net.signal)
                                            } else {
                                                format!("{}%", net.signal)
                                            };
                                            let (px, w, h) = tr.rasterize_colored(
                                                &right_label,
                                                t_wp.state_label_size,
                                                if net.in_use { t_wp.state_label_color } else { signal_color },
                                            );
                                            if w > 0 && h > 0 {
                                                let buf = MemoryRenderBuffer::from_slice(
                                                    &px,
                                                    smithay::backend::allocator::Fourcc::Argb8888,
                                                    (w as i32, h as i32),
                                                    1, Transform::Normal, None,
                                                );
                                                let lx = row_x + row_w
                                                    - t_wp.row_padding_h - w as i32;
                                                let ly = row_y + (t_wp.row_height - h as i32) / 2;
                                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                        .to_physical_precise_round(1.0);
                                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                    renderer, loc.to_f64(), &buf,
                                                    None, None, None, Kind::Unspecified,
                                                ) {
                                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                }
                                            }
                                        }
                                        data.state.wifi_row_rects.push((
                                            net.ssid.clone(),
                                            smithay::utils::Rectangle::new(
                                                smithay::utils::Point::from((row_x, row_y)),
                                                smithay::utils::Size::from((row_w, t_wp.row_height)),
                                            ),
                                        ));
                                        cur_y += t_wp.row_height;
                                        // Expansão de senha inline.
                                        if data.state.wifi_password_for.as_deref()
                                            == Some(&net.ssid)
                                        {
                                            let exp_h = t_wp.pw_field_height + 8
                                                + t_wp.pw_btn_height + 16;
                                            if cur_y + exp_h > max_y {
                                                break;
                                            }
                                            // Field bg.
                                            let field_w = row_w - t_wp.row_padding_h * 2;
                                            let field = crate::gfx::make_rounded_panel(
                                                field_w, t_wp.pw_field_height,
                                                t_wp.pw_field_corner,
                                                t_wp.pw_field_bg,
                                            );
                                            let floc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((row_x + t_wp.row_padding_h, cur_y))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, floc.to_f64(), &field,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                            // Senha mascarada (•) + cursor.
                                            let masked: String = "•".repeat(
                                                data.state.wifi_password_text.chars().count(),
                                            );
                                            let display = if masked.is_empty() {
                                                "Senha".to_string()
                                            } else {
                                                masked
                                            };
                                            let display_color = if data.state.wifi_password_text.is_empty() {
                                                t_wp.empty_label_color
                                            } else {
                                                t_wp.pw_field_text_color
                                            };
                                            if let Some(tr) = text_r_ref {
                                                let (px, w, h) = tr.rasterize_colored(
                                                    &display, t_wp.pw_field_size, display_color,
                                                );
                                                if w > 0 && h > 0 {
                                                    let buf = MemoryRenderBuffer::from_slice(
                                                        &px,
                                                        smithay::backend::allocator::Fourcc::Argb8888,
                                                        (w as i32, h as i32),
                                                        1, Transform::Normal, None,
                                                    );
                                                    let lx = row_x + t_wp.row_padding_h + 12;
                                                    let ly = cur_y
                                                        + (t_wp.pw_field_height - h as i32) / 2;
                                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                            .to_physical_precise_round(1.0);
                                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                        renderer, loc.to_f64(), &buf,
                                                        None, None, None, Kind::Unspecified,
                                                    ) {
                                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                    }
                                                }
                                            }
                                            cur_y += t_wp.pw_field_height + 8;
                                            // Botão "Conectar".
                                            let btn_label = if data.state.wifi_connecting.as_deref()
                                                == Some(&net.ssid)
                                            {
                                                "Conectando…"
                                            } else {
                                                "Conectar"
                                            };
                                            if let Some(tr) = text_r_ref {
                                                let (px, w, h) = tr.rasterize_colored(
                                                    btn_label, t_wp.pw_btn_label_size, t_wp.pw_btn_fg,
                                                );
                                                let btn_w = (w as i32 + 32).max(100);
                                                let btn_x = row_x + row_w
                                                    - t_wp.row_padding_h - btn_w;
                                                let btn_y = cur_y;
                                                let btn_bg = crate::gfx::make_rounded_panel(
                                                    btn_w, t_wp.pw_btn_height, t_wp.pw_btn_corner,
                                                    t_wp.pw_btn_bg,
                                                );
                                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((btn_x, btn_y))
                                                        .to_physical_precise_round(1.0);
                                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                    renderer, loc.to_f64(), &btn_bg,
                                                    None, None, None, Kind::Unspecified,
                                                ) {
                                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                }
                                                if w > 0 && h > 0 {
                                                    let buf = MemoryRenderBuffer::from_slice(
                                                        &px,
                                                        smithay::backend::allocator::Fourcc::Argb8888,
                                                        (w as i32, h as i32),
                                                        1, Transform::Normal, None,
                                                    );
                                                    let lx = btn_x + (btn_w - w as i32) / 2;
                                                    let ly = btn_y + (t_wp.pw_btn_height - h as i32) / 2;
                                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                            .to_physical_precise_round(1.0);
                                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                        renderer, loc.to_f64(), &buf,
                                                        None, None, None, Kind::Unspecified,
                                                    ) {
                                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                    }
                                                }
                                                data.state.wifi_connect_btn_rect = Some(
                                                    smithay::utils::Rectangle::new(
                                                        smithay::utils::Point::from((btn_x, btn_y)),
                                                        smithay::utils::Size::from((btn_w, t_wp.pw_btn_height)),
                                                    ),
                                                );
                                                cur_y += t_wp.pw_btn_height + 8;
                                            }
                                            // Erro inline (se houver pra esse ssid).
                                            if let Some((esid, msg)) =
                                                data.state.wifi_error_for.clone()
                                            {
                                                if esid == net.ssid {
                                                    if let Some(tr) = text_r_ref {
                                                        let (px, w, h) = tr.rasterize_colored(
                                                            &msg, t_wp.state_label_size,
                                                            t_wp.pw_error_color,
                                                        );
                                                        if w > 0 && h > 0 {
                                                            let buf = MemoryRenderBuffer::from_slice(
                                                                &px,
                                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                                (w as i32, h as i32),
                                                                1, Transform::Normal, None,
                                                            );
                                                            let lx = row_x + t_wp.row_padding_h;
                                                            let ly = cur_y;
                                                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                                    .to_physical_precise_round(1.0);
                                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                                renderer, loc.to_f64(), &buf,
                                                                None, None, None, Kind::Unspecified,
                                                            ) {
                                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                            }
                                                            cur_y += h as i32 + 4;
                                                        }
                                                    }
                                                }
                                            }
                                            cur_y += 8;
                                        }
                                    }
                                }
                            } else if popup.kind == crate::popup::AppletKind::Battery {
                                let t_bp = &theme.battery_popup;
                                // Bg gradient (mesma casca de applet).
                                let bg = crate::gfx::make_titlebar_bg(
                                    popup.size.w, popup.size.h,
                                    t_popup.corner_radius,
                                    theme.window.titlebar_bg_top,
                                    theme.window.titlebar_bg_bottom,
                                );
                                let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    o.to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer, bg_loc.to_f64(), &bg,
                                    None, None, None, Kind::Unspecified,
                                ) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                let bat = data.state.tray.battery_info;
                                let ppd = data.state.power_profiles.clone();
                                let pct = bat.map(|b| b.capacity).unwrap_or(0);
                                let charging = bat.map(|b| b.charging).unwrap_or(false);
                                let is_low = pct < t_bp.low_threshold_pct;
                                let pct_color = if is_low {
                                    t_bp.hero_pct_low_color
                                } else {
                                    t_bp.hero_pct_color
                                };
                                let bar_fill_color = if is_low {
                                    t_bp.level_bar_fill_low_color
                                } else {
                                    t_bp.level_bar_fill_color
                                };
                                let state_text = if charging && pct >= 100 {
                                    "Carregada"
                                } else if charging {
                                    "Carregando"
                                } else {
                                    "Descarregando"
                                };
                                let inner_x = o.x + t_bp.padding;
                                let inner_w = popup.size.w - t_bp.padding * 2;
                                let text_r_ref = data.state.text.as_ref();
                                // Header "Bateria" centralizado.
                                if let Some(tr) = text_r_ref {
                                    let (px, w, h) = tr.rasterize_colored(
                                        "Bateria", t_bp.header_size, t_bp.header_color,
                                    );
                                    if w > 0 && h > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1, Transform::Normal, None,
                                        );
                                        let lx = inner_x + (inner_w - w as i32) / 2;
                                        let ly = o.y + (t_bp.header_height - h as i32) / 2;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                }
                                let mut cur_y = o.y + t_bp.header_height;
                                // % grande centralizado.
                                let pct_str = format!("{}%", pct);
                                let text_r_ref = data.state.text.as_ref();
                                if let Some(tr) = text_r_ref {
                                    let (px, w, h) = tr.rasterize_colored(
                                        &pct_str, t_bp.hero_pct_size, pct_color,
                                    );
                                    if w > 0 && h > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1, Transform::Normal, None,
                                        );
                                        let lx = inner_x + (inner_w - w as i32) / 2;
                                        let ly = cur_y + 8;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                        cur_y = ly + h as i32 + 6;
                                    }
                                }
                                // State text centralizado abaixo do %.
                                if let Some(tr) = text_r_ref {
                                    let (px, w, h) = tr.rasterize_colored(
                                        state_text, t_bp.hero_state_size, t_bp.hero_state_color,
                                    );
                                    if w > 0 && h > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1, Transform::Normal, None,
                                        );
                                        let lx = inner_x + (inner_w - w as i32) / 2;
                                        let ly = cur_y;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                        cur_y = ly + h as i32 + 14;
                                    }
                                }
                                // Barra de nível (track + fill).
                                {
                                    let bar_w = inner_w;
                                    let bar_h = t_bp.level_bar_height;
                                    let bar_x = inner_x;
                                    let bar_y = cur_y;
                                    let bar_bg = crate::gfx::make_rounded_panel(
                                        bar_w, bar_h, t_bp.level_bar_corner,
                                        t_bp.level_bar_bg_color,
                                    );
                                    let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((bar_x, bar_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, bg_loc.to_f64(), &bar_bg,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    let fill_w = ((pct as i32) * bar_w / 100).max(0);
                                    if fill_w > 0 {
                                        let fill = crate::gfx::make_rounded_panel(
                                            fill_w, bar_h, t_bp.level_bar_corner,
                                            bar_fill_color,
                                        );
                                        let fl_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((bar_x, bar_y))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, fl_loc.to_f64(), &fill,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    cur_y = bar_y + bar_h + 20;
                                }
                                // Divider antes da seção de modo.
                                if ppd.is_some() {
                                    let div = crate::gfx::make_rounded_panel(
                                        inner_w, 1, 0, t_bp.divider_color,
                                    );
                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((inner_x, cur_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, loc.to_f64(), &div,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    cur_y += 12;
                                }
                                // "MODO DE ENERGIA" label + slider.
                                data.state.battery_mode_zones.clear();
                                if let Some(ppd) = ppd.as_ref() {
                                    if let Some(tr) = text_r_ref {
                                        let (px, w, h) = tr.rasterize_colored(
                                            "MODO DE ENERGIA",
                                            t_bp.mode_label_size,
                                            t_bp.mode_label_color,
                                        );
                                        if w > 0 && h > 0 {
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1, Transform::Normal, None,
                                            );
                                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((inner_x, cur_y))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, loc.to_f64(), &buf,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                            cur_y += h as i32 + 12;
                                        }
                                    }
                                    // Slider track (com bg) + fill + thumb.
                                    let stops = ppd.available.len() as i32;
                                    let active_idx = ppd.active_idx().unwrap_or(0) as i32;
                                    let track_x = inner_x + t_bp.mode_slider_thumb_radius;
                                    let track_w = inner_w - t_bp.mode_slider_thumb_radius * 2;
                                    let track_y = cur_y + t_bp.mode_slider_thumb_radius
                                        - t_bp.mode_slider_track_height / 2;
                                    let track_bg = crate::gfx::make_rounded_panel(
                                        track_w, t_bp.mode_slider_track_height,
                                        t_bp.mode_slider_track_height / 2,
                                        t_bp.mode_slider_track_color,
                                    );
                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((track_x, track_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, loc.to_f64(), &track_bg,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    // Posição do thumb: centro da zona ativa.
                                    let thumb_step = if stops > 1 {
                                        track_w / (stops - 1)
                                    } else {
                                        0
                                    };
                                    let thumb_center_x = track_x + active_idx * thumb_step;
                                    // Fill: do início do track até o thumb.
                                    let fill_w = (thumb_center_x - track_x).max(0);
                                    if fill_w > 0 {
                                        let fill = crate::gfx::make_rounded_panel(
                                            fill_w, t_bp.mode_slider_track_height,
                                            t_bp.mode_slider_track_height / 2,
                                            t_bp.mode_slider_fill_color,
                                        );
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((track_x, track_y))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &fill,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    // Thumb ring + thumb.
                                    let r = t_bp.mode_slider_thumb_radius;
                                    let thumb_y = cur_y;
                                    let ring = crate::gfx::make_rounded_panel(
                                        r * 2 + 4, r * 2 + 4, r + 2,
                                        t_bp.mode_slider_thumb_ring_color,
                                    );
                                    let r_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((thumb_center_x - r - 2, thumb_y - 2))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, r_loc.to_f64(), &ring,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    let thumb = crate::gfx::make_rounded_panel(
                                        r * 2, r * 2, r, t_bp.mode_slider_thumb_color,
                                    );
                                    let t_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((thumb_center_x - r, thumb_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, t_loc.to_f64(), &thumb,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    cur_y += r * 2 + 10;
                                    // Labels embaixo + hit zones (1/3 cada).
                                    let zone_w = inner_w / stops.max(1);
                                    for (i, name) in ppd.available.iter().enumerate() {
                                        let label = crate::power::profile_label(name);
                                        let active = i as i32 == active_idx;
                                        let color = if active {
                                            t_bp.mode_stop_label_active_color
                                        } else {
                                            t_bp.mode_stop_label_color
                                        };
                                        if let Some(tr) = text_r_ref {
                                            let (px, w, h) = tr.rasterize_colored(
                                                label, t_bp.mode_stop_label_size, color,
                                            );
                                            if w > 0 && h > 0 {
                                                let buf = MemoryRenderBuffer::from_slice(
                                                    &px,
                                                    smithay::backend::allocator::Fourcc::Argb8888,
                                                    (w as i32, h as i32),
                                                    1, Transform::Normal, None,
                                                );
                                                let zone_x = inner_x + (i as i32) * zone_w;
                                                let lx = zone_x + (zone_w - w as i32) / 2;
                                                let ly = cur_y;
                                                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                    smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                        .to_physical_precise_round(1.0);
                                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                    renderer, loc.to_f64(), &buf,
                                                    None, None, None, Kind::Unspecified,
                                                ) {
                                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                                }
                                            }
                                        }
                                        // Cache zone rect pra hit-test.
                                        // Zona é vertical: cobre o track
                                        // + thumb + label (~50px alto).
                                        let zone_x = inner_x + (i as i32) * zone_w;
                                        let zone_top = track_y - r;
                                        let zone_height = (r * 2) + 30;
                                        data.state.battery_mode_zones.push((
                                            name.clone(),
                                            smithay::utils::Rectangle::new(
                                                smithay::utils::Point::from((zone_x, zone_top)),
                                                smithay::utils::Size::from((zone_w, zone_height)),
                                            ),
                                        ));
                                    }
                                }
                            } else if popup.kind == crate::popup::AppletKind::Calendar {
                                let bg = crate::gfx::make_titlebar_bg(
                                    popup.size.w,
                                    popup.size.h,
                                    t_popup.corner_radius,
                                    theme.window.titlebar_bg_top,
                                    theme.window.titlebar_bg_bottom,
                                );
                                let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                    o.to_physical_precise_round(1.0);
                                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                    renderer,
                                    bg_loc.to_f64(),
                                    &bg,
                                    None,
                                    None,
                                    None,
                                    Kind::Unspecified,
                                ) {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                // H55 Fatia 2: grade estática do mês atual.
                                // Header (título), linha dos dias da semana,
                                // grid 6×7 com hoje em coral, vizinhos
                                // apagados. Sem navegação ainda (Fatia 3),
                                // sem rodapé (Fatia 4).
                                let t_cal = &theme.calendario;
                                if let Some(text_r) = data.state.text.as_ref() {
                                    use chrono::{Datelike, Local, NaiveDate};
                                    let today = Local::now().date_naive();
                                    let d_today = today.day();
                                    // H55 Fatia 3: mês exibido vem da
                                    // navegação (setas/teclado/Hoje).
                                    let displayed_year = popup.displayed_year;
                                    let displayed_month = popup.displayed_month;
                                    // Regra do destaque: coral SÓ quando
                                    // o mês exibido é o mês atual.
                                    let show_today_highlight =
                                        displayed_year == today.year()
                                            && displayed_month == today.month();
                                    let meses = [
                                        "Janeiro", "Fevereiro", "Março",
                                        "Abril", "Maio", "Junho", "Julho",
                                        "Agosto", "Setembro", "Outubro",
                                        "Novembro", "Dezembro",
                                    ];
                                    let mes_label = meses
                                        .get((displayed_month as usize).saturating_sub(1))
                                        .copied()
                                        .unwrap_or("");
                                    let titulo = format!("{} {}", mes_label, displayed_year);

                                    // H55 Fatia 3: setas anterior/próximo
                                    // no cabeçalho. Hit target 40×40.
                                    // Posições: esquerda e direita do
                                    // header, com folga do padding lateral.
                                    let arrow_hit = 40i32;
                                    let header_y = o.y + t_cal.padding_vertical;
                                    let arrow_y = header_y
                                        + (t_cal.header_height - arrow_hit) / 2;
                                    let prev_x = o.x + t_cal.padding_lateral;
                                    let next_x = o.x + popup.size.w
                                        - t_cal.padding_lateral - arrow_hit;
                                    data.state.calendar_prev_rect = Some(
                                        smithay::utils::Rectangle::new(
                                            smithay::utils::Point::from((prev_x, arrow_y)),
                                            smithay::utils::Size::from((arrow_hit, arrow_hit)),
                                        ),
                                    );
                                    data.state.calendar_next_rect = Some(
                                        smithay::utils::Rectangle::new(
                                            smithay::utils::Point::from((next_x, arrow_y)),
                                            smithay::utils::Size::from((arrow_hit, arrow_hit)),
                                        ),
                                    );
                                    // Render dos chevrons como texto + bg
                                    // de hover/focus. H55 Fatia 4: foco
                                    // do teclado e hover do mouse usam
                                    // o accent_amber como ring/bg.
                                    let amber = theme.colors.accent_amber;
                                    let focus_idx = popup.calendar_focus;
                                    let hover_idx = popup.calendar_hover;
                                    let render_arrow = |chev: &str, cx: i32, cy: i32, idx: u8,
                                                        elems: &mut Vec<LumaRenderElement<GlesRenderer>>,
                                                        r: &mut GlesRenderer| {
                                        let is_focused = focus_idx == Some(idx);
                                        let is_hovered = hover_idx == Some(idx);
                                        if is_focused || is_hovered {
                                            let alpha = if is_focused { 0.30 } else { 0.18 };
                                            let bg_color = smithay::backend::renderer::Color32F::new(
                                                amber.r(), amber.g(), amber.b(), alpha,
                                            );
                                            let bg = crate::gfx::make_rounded_panel(
                                                arrow_hit, arrow_hit, 8, bg_color,
                                            );
                                            let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((cx, cy))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                r, bg_loc.to_f64(), &bg, None, None, None,
                                                Kind::Unspecified,
                                            ) {
                                                elems.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                        let chev_color = if is_hovered || is_focused {
                                            t_cal.seta_fg_hover
                                        } else {
                                            t_cal.seta_fg
                                        };
                                        let (px, w, h) = text_r.rasterize_colored(
                                            chev,
                                            t_cal.titulo_size,
                                            chev_color,
                                        );
                                        if w == 0 || h == 0 {
                                            return;
                                        }
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let lx = cx + (arrow_hit - w as i32) / 2;
                                        let ly = cy + (arrow_hit - h as i32) / 2;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            r, loc.to_f64(), &buf, None, None, None,
                                            Kind::Unspecified,
                                        ) {
                                            elems.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    };
                                    render_arrow("‹", prev_x, arrow_y, 0, &mut custom_elements, renderer);
                                    render_arrow("›", next_x, arrow_y, 2, &mut custom_elements, renderer);

                                    // 1. Título centralizado no header.
                                    let (px, tw, th) = text_r.rasterize_colored(
                                        &titulo,
                                        t_cal.titulo_size,
                                        t_cal.titulo_fg,
                                    );
                                    if tw > 0 && th > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (tw as i32, th as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let tx = o.x + (popup.size.w - tw as i32) / 2;
                                        let ty = o.y + t_cal.padding_vertical
                                            + (t_cal.header_height - th as i32) / 2;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((tx, ty))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None,
                                            Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }

                                    // 2. Linha dos dias da semana
                                    //    (dom seg ter qua qui sex sáb).
                                    let dias_semana = ["dom","seg","ter","qua","qui","sex","sáb"];
                                    let grid_left = o.x + t_cal.padding_lateral;
                                    let diasemana_y = o.y + t_cal.padding_vertical
                                        + t_cal.header_height;
                                    for (col, lbl) in dias_semana.iter().enumerate() {
                                        let (px, w, h) = text_r.rasterize_colored(
                                            lbl,
                                            t_cal.diasemana_size,
                                            t_cal.diasemana_fg,
                                        );
                                        if w == 0 || h == 0 {
                                            continue;
                                        }
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let cell_x = grid_left + (col as i32) * t_cal.cell_size;
                                        let lx = cell_x + (t_cal.cell_size - w as i32) / 2;
                                        let ly = diasemana_y
                                            + (t_cal.diasemana_height - h as i32) / 2;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None,
                                            Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }

                                    // 3. Grade 6×7. Calcula 42 células:
                                    //    primeiras = fim do mês anterior,
                                    //    meio = dias do mês exibido,
                                    //    últimas = início do próximo mês.
                                    let first_of_month = NaiveDate::from_ymd_opt(
                                        displayed_year, displayed_month, 1,
                                    ).unwrap_or(today);
                                    // Domingo = 0 ... Sábado = 6.
                                    let first_dow = first_of_month
                                        .weekday()
                                        .num_days_from_sunday() as i32;
                                    // Dias do mês exibido:
                                    let next_month_first = if displayed_month == 12 {
                                        NaiveDate::from_ymd_opt(displayed_year + 1, 1, 1)
                                    } else {
                                        NaiveDate::from_ymd_opt(displayed_year, displayed_month + 1, 1)
                                    }.unwrap_or(today);
                                    let days_in_month = next_month_first
                                        .signed_duration_since(first_of_month)
                                        .num_days() as i32;
                                    let prev_month_last = first_of_month
                                        .pred_opt()
                                        .unwrap_or(first_of_month);
                                    let days_in_prev = prev_month_last.day() as i32;

                                    let grid_top = diasemana_y + t_cal.diasemana_height;
                                    for cell_idx in 0..42i32 {
                                        let row = cell_idx / 7;
                                        let col = cell_idx % 7;
                                        let (label_num, is_neighbor, is_today_cell)
                                            = if cell_idx < first_dow {
                                                // Mês anterior (apagado).
                                                let n = days_in_prev
                                                    - (first_dow - 1 - cell_idx);
                                                (n, true, false)
                                            } else if cell_idx < first_dow + days_in_month {
                                                let n = cell_idx - first_dow + 1;
                                                let is_today = show_today_highlight
                                                    && n as u32 == d_today;
                                                (n, false, is_today)
                                            } else {
                                                // Mês seguinte (apagado).
                                                let n = cell_idx - first_dow
                                                    - days_in_month + 1;
                                                (n, true, false)
                                            };

                                        let cell_x = grid_left + col * t_cal.cell_size;
                                        let cell_y = grid_top + row * t_cal.cell_size;

                                        // Hoje: quadrado coral inscrito
                                        // na célula com ~3px de folga.
                                        if is_today_cell {
                                            let folga = 3;
                                            let inner = t_cal.cell_size - folga * 2;
                                            let bg = crate::gfx::make_rounded_panel(
                                                inner, inner, t_cal.hoje_raio,
                                                t_cal.hoje_bg,
                                            );
                                            let bx = cell_x + folga;
                                            let by = cell_y + folga;
                                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((bx, by))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, loc.to_f64(), &bg,
                                                None, None, None,
                                                Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }

                                        // Número do dia.
                                        let color = if is_today_cell {
                                            t_cal.hoje_fg
                                        } else if is_neighbor {
                                            t_cal.dia_vizinho_fg
                                        } else {
                                            t_cal.dia_fg
                                        };
                                        let s = label_num.to_string();
                                        let (px, w, h) = text_r.rasterize_colored(
                                            &s, t_cal.dia_size, color,
                                        );
                                        if w == 0 || h == 0 {
                                            continue;
                                        }
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let lx = cell_x + (t_cal.cell_size - w as i32) / 2;
                                        let ly = cell_y + (t_cal.cell_size - h as i32) / 2;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None,
                                            Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }

                                    // H55 Fatia 4: rodapé com data por
                                    // extenso à esquerda + botão "Hoje"
                                    // à direita. Hoje fica dim quando
                                    // displayed == mês atual (opcional
                                    // do spec, ativado).
                                    let footer_y = o.y + popup.size.h
                                        - t_cal.padding_vertical - t_cal.footer_height;
                                    // Data por extenso (hoje, não o mês
                                    // exibido). Formato: "Terça, 26 de maio".
                                    let dias_extenso = [
                                        "Domingo", "Segunda", "Terça",
                                        "Quarta", "Quinta", "Sexta", "Sábado",
                                    ];
                                    let meses_lower = [
                                        "janeiro", "fevereiro", "março",
                                        "abril", "maio", "junho", "julho",
                                        "agosto", "setembro", "outubro",
                                        "novembro", "dezembro",
                                    ];
                                    let dow_idx = today
                                        .weekday()
                                        .num_days_from_sunday() as usize;
                                    let mes_lower_idx = (today.month() as usize)
                                        .saturating_sub(1);
                                    let data_extenso = format!(
                                        "{}, {} de {}",
                                        dias_extenso.get(dow_idx).copied().unwrap_or(""),
                                        today.day(),
                                        meses_lower.get(mes_lower_idx).copied().unwrap_or(""),
                                    );
                                    let (dpx, dw, dh) = text_r.rasterize_colored(
                                        &data_extenso,
                                        t_cal.rodape_size,
                                        t_cal.rodape_fg,
                                    );
                                    if dw > 0 && dh > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &dpx,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (dw as i32, dh as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let dx = o.x + t_cal.padding_lateral;
                                        let dy = footer_y
                                            + (t_cal.footer_height - dh as i32) / 2;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((dx, dy))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None,
                                            Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    // Botão "Hoje".
                                    let is_focused = focus_idx == Some(1);
                                    let is_at_today = show_today_highlight;
                                    let label_color = if is_at_today {
                                        // Apagado: usa cor de rodapé.
                                        t_cal.rodape_fg
                                    } else {
                                        t_cal.hoje_botao_fg
                                    };
                                    let (px, hw, hh) = text_r.rasterize_colored(
                                        "Hoje",
                                        t_cal.rodape_size,
                                        label_color,
                                    );
                                    let btn_h = (t_cal.footer_height - 8).max(28);
                                    let pad_h = 14i32;
                                    let btn_w = (hw as i32 + pad_h * 2).max(64);
                                    let btn_y = footer_y + (t_cal.footer_height - btn_h) / 2;
                                    let btn_x = o.x + popup.size.w
                                        - t_cal.padding_lateral - btn_w;
                                    let bg_color = if is_at_today {
                                        let c = t_cal.hoje_botao_bg;
                                        smithay::backend::renderer::Color32F::new(
                                            c.r(), c.g(), c.b(), c.a() * 0.4,
                                        )
                                    } else if is_focused {
                                        t_cal.hoje_botao_bg_hover
                                    } else {
                                        t_cal.hoje_botao_bg
                                    };
                                    // Anel de foco do teclado pra Hoje.
                                    if is_focused {
                                        let ring = 2i32;
                                        let ring_color = smithay::backend::renderer::Color32F::new(
                                            amber.r(), amber.g(), amber.b(), 0.55,
                                        );
                                        let ring_buf = crate::gfx::make_rounded_panel(
                                            btn_w + ring * 2,
                                            btn_h + ring * 2,
                                            8 + ring,
                                            ring_color,
                                        );
                                        let rloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((btn_x - ring, btn_y - ring))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, rloc.to_f64(), &ring_buf,
                                            None, None, None,
                                            Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    let bg = crate::gfx::make_rounded_panel(
                                        btn_w, btn_h, 8, bg_color,
                                    );
                                    let bg_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((btn_x, btn_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, bg_loc.to_f64(), &bg,
                                        None, None, None,
                                        Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    if hw > 0 && hh > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (hw as i32, hh as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let lx = btn_x + (btn_w - hw as i32) / 2;
                                        let ly = btn_y + (btn_h - hh as i32) / 2;
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, loc.to_f64(), &buf,
                                            None, None, None,
                                            Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    data.state.calendar_hoje_rect = Some(
                                        smithay::utils::Rectangle::new(
                                            smithay::utils::Point::from((btn_x, btn_y)),
                                            smithay::utils::Size::from((btn_w, btn_h)),
                                        ),
                                    );
                                }
                                // Pula todo o pipeline de volume (tabs,
                                // devices, sliders) — Calendar não tem.
                            } else {
                            let t_slider = &t_popup.slider;
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
                            } // fecha else (Volume kind)
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

                        // H54 P3: stack de toasts (até MAX_VISIBLE_TOASTS=4)
                        // no canto sup-direito. Tick processa expiração,
                        // promoção da queue, e remoção de closing entries.
                        // Render itera visible top-down (newest-on-top
                        // configurável). Cada entry tem slide-in + fade-in
                        // nos primeiros ENTRY_ANIM_MS, fade-out linear
                        // nos últimos EXIT_ANIM_MS, reflow interp em
                        // REFLOW_ANIM_MS. Hover-pause global.
                        // H54.P4: tick retorna (id, reason) das removidas →
                        // emite NotificationClosed signal por cada.
                        let closed_in_tick =
                            crate::notifications::tick(&data.state.notifications);
                        for (cid, reason) in closed_in_tick {
                            crate::notifications::emit_notification_closed(
                                &data.state.notifications_conn,
                                cid,
                                reason,
                            );
                        }
                        // H54.P6+P7: gate único de alerta. Itera visible
                        // coletando entries com alert_pending=true, aplica
                        // filtro DND (não-critical sob DND não alerta) e
                        // limpa o flag. Sound e flash são consumidores
                        // independentes desse gate. suppress_sound é
                        // filtro extra só-do-som.
                        struct AlertInfo {
                            sound_file: Option<String>,
                            urgency: crate::notifications::Urgency,
                            suppress_sound: bool,
                        }
                        let (alerts, sound_on, flash_on) =
                            match data.state.notifications.lock() {
                                Ok(mut g) => {
                                    let sound_on = g.sound_enabled;
                                    let flash_on = g.flash_enabled;
                                    let dnd_on = g.dnd_enabled;
                                    let mut acc: Vec<AlertInfo> = Vec::new();
                                    for e in g.visible.iter_mut() {
                                        if !e.alert_pending {
                                            continue;
                                        }
                                        e.alert_pending = false;
                                        if dnd_on
                                            && e.urgency
                                                != crate::notifications::Urgency::Critical
                                        {
                                            continue;
                                        }
                                        acc.push(AlertInfo {
                                            sound_file: e.sound_file.clone(),
                                            urgency: e.urgency,
                                            suppress_sound: e.suppress_sound,
                                        });
                                    }
                                    (acc, sound_on, flash_on)
                                }
                                Err(_) => (Vec::new(), false, false),
                            };
                        if !alerts.is_empty() {
                            let t_n_p = crate::theme::current().notifications.clone();
                            // Consumer 1: som. Filtra suppress_sound + toggle.
                            if sound_on {
                                if let Some(eng) = data.state.sound.as_ref() {
                                    for a in alerts.iter() {
                                        if a.suppress_sound {
                                            continue;
                                        }
                                        let path_str = match &a.sound_file {
                                            Some(p) => p.clone(),
                                            None => match a.urgency {
                                                crate::notifications::Urgency::Critical => {
                                                    t_n_p.sound_critical_path.clone()
                                                }
                                                _ => t_n_p.sound_default_path.clone(),
                                            },
                                        };
                                        eng.play_file(std::path::Path::new(&path_str));
                                    }
                                }
                            }
                            // Consumer 2: piscada (H54.P7). Não checa
                            // suppress_sound. Debounce interno
                            // (rajada = 1 piscada, anti-strobe).
                            if flash_on {
                                let debounce = std::time::Duration::from_millis(
                                    t_n_p.flash_debounce_ms,
                                );
                                if let Ok(mut g) = data.state.notifications.lock() {
                                    for a in alerts.iter() {
                                        crate::notifications::trigger_flash(
                                            &mut g, a.urgency, debounce,
                                        );
                                    }
                                }
                            }
                        }
                        // Snapshot da lista visible.
                        let mut visible_snap: Vec<crate::notifications::NotificationEntry> =
                            match data.state.notifications.lock() {
                                Ok(g) => g.visible.clone(),
                                Err(_) => Vec::new(),
                            };
                        if !visible_snap.is_empty() {
                            let t_n = crate::theme::current().notifications.clone();
                            // Helper local pra rasterizar texto.
                            let text_r = data.state.text.as_ref();
                            // ESTRUTURA: pra cada entry, computar card_h
                            // (depende do body wrap), rasterizar textos
                            // uma vez, computar target_y top-down, interp
                            // current_y, render.
                            struct ToastButton {
                                key: String,
                                label_buf: MemoryRenderBuffer,
                                label_w: i32,
                                label_h: i32,
                                /// (x, y, w, h) relativos ao card top-left.
                                rect_local: (i32, i32, i32, i32),
                            }
                            struct ToastRender {
                                id: u32,
                                card_w: i32,
                                card_h: i32,
                                icon: Option<MemoryRenderBuffer>,
                                icon_size: i32,
                                title_buf: Option<(MemoryRenderBuffer, i32, i32)>,
                                body_bufs: Vec<(MemoryRenderBuffer, i32, i32)>,
                                time_buf: Option<(MemoryRenderBuffer, i32, i32)>,
                                buttons: Vec<ToastButton>,
                                target_y: f32,
                                current_y: f32,
                                slide_offset_x: f32,
                                alpha: f32,
                            }
                            let mut toasts: Vec<ToastRender> = Vec::new();
                            let mut layout_y = t_n.margin as f32;
                            for entry in visible_snap.iter() {
                                // Rasteriza title.
                                let title_buf = text_r.and_then(|t| {
                                    let label = truncate_chars(&entry.summary, 36);
                                    let (px, w, h) = t.rasterize_colored(
                                        &label, t_n.title_size, t_n.title_color,
                                    );
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
                                // H54.P3.fix: wrap por pixel width (não
                                // char-count). Largura disponível = card
                                // menos icon column + paddings.
                                let body_max_width =
                                    t_n.width - t_n.padding * 3 - t_n.icon_size;
                                let body_lines: Vec<String> = if entry.body.is_empty() {
                                    Vec::new()
                                } else if let Some(tr) = text_r {
                                    wrap_to_lines_pixels(
                                        &entry.body,
                                        tr,
                                        t_n.body_size,
                                        t_n.body_color,
                                        body_max_width,
                                        t_n.max_body_lines,
                                    )
                                } else {
                                    Vec::new()
                                };
                                let body_bufs: Vec<(MemoryRenderBuffer, i32, i32)> = text_r
                                    .map(|t| {
                                        body_lines
                                            .iter()
                                            .filter_map(|line| {
                                                let (px, w, h) = t.rasterize_colored(
                                                    line,
                                                    t_n.body_size,
                                                    t_n.body_color,
                                                );
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
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                let time_buf = text_r.and_then(|t| {
                                    let (px, w, h) = t.rasterize_colored(
                                        "agora",
                                        t_n.time_size,
                                        t_n.time_color,
                                    );
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
                                let title_h =
                                    title_buf.as_ref().map(|(_, _, h)| *h).unwrap_or(16);
                                let body_h_total: i32 =
                                    body_bufs.iter().map(|(_, _, h)| h + 2).sum();
                                let time_h =
                                    time_buf.as_ref().map(|(_, _, h)| *h).unwrap_or(12);
                                let text_block_h = title_h + body_h_total + 4 + time_h;
                                // H54.P4: layout dos action buttons. Rasteriza
                                // cada label, computa rect_local, wrap pra
                                // próxima linha se exceder largura.
                                let mut buttons: Vec<ToastButton> = Vec::new();
                                let mut buttons_block_h: i32 = 0;
                                if !entry.actions.is_empty() {
                                    if let Some(tr) = text_r {
                                        let btn_h_unit = t_n.action_btn_height;
                                        // Coluna de conteúdo (alinhada com título/body, após o ícone).
                                        let content_left_x =
                                            t_n.padding + t_n.icon_size + t_n.padding;
                                        let content_right_x = t_n.width - t_n.padding;
                                        let row_top_y = t_n.padding + text_block_h
                                            + t_n.action_btn_top_margin;
                                        let mut cur_x: i32 = content_left_x;
                                        let mut cur_y: i32 = row_top_y;
                                        for (key, label) in entry.actions.iter() {
                                            let (px, w, h) = tr.rasterize_colored(
                                                label,
                                                t_n.action_btn_label_size,
                                                t_n.action_btn_label_color,
                                            );
                                            if w == 0 || h == 0 {
                                                continue;
                                            }
                                            let label_buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1,
                                                Transform::Normal,
                                                None,
                                            );
                                            let btn_w =
                                                w as i32 + t_n.action_btn_padding_h * 2;
                                            // Wrap se passa da largura disponível.
                                            if cur_x + btn_w > content_right_x
                                                && cur_x > content_left_x
                                            {
                                                cur_x = content_left_x;
                                                cur_y += btn_h_unit + t_n.action_btn_gap;
                                            }
                                            buttons.push(ToastButton {
                                                key: key.clone(),
                                                label_buf,
                                                label_w: w as i32,
                                                label_h: h as i32,
                                                rect_local: (cur_x, cur_y, btn_w, btn_h_unit),
                                            });
                                            cur_x += btn_w + t_n.action_btn_gap;
                                        }
                                        // Total occupied = última cur_y + btn_h - row_top_y.
                                        if let Some(last) = buttons.last() {
                                            let (_, by, _, bh) = last.rect_local;
                                            buttons_block_h =
                                                (by + bh) - text_block_h - t_n.padding;
                                        }
                                    }
                                }
                                let card_h = (t_n.padding * 2 + t_n.icon_size)
                                    .max(t_n.padding * 2 + text_block_h + buttons_block_h);
                                // Anim: slide-in + fade-in nos primeiros
                                // ENTRY_ANIM_MS; fade-out linear nos últimos
                                // EXIT_ANIM_MS.
                                let entered_ms =
                                    entry.entered_at.elapsed().as_millis() as u64;
                                let entry_anim = crate::notifications::ENTRY_ANIM_MS;
                                let exit_anim = crate::notifications::EXIT_ANIM_MS;
                                let in_progress = if entered_ms < entry_anim {
                                    crate::notifications::ease_out_cubic(
                                        entered_ms as f32 / entry_anim as f32,
                                    )
                                } else {
                                    1.0
                                };
                                let exit_progress = entry
                                    .closing_since
                                    .map(|t| {
                                        (t.elapsed().as_millis() as f32 / exit_anim as f32)
                                            .clamp(0.0, 1.0)
                                    })
                                    .unwrap_or(0.0);
                                let alpha = in_progress * (1.0 - exit_progress);
                                let slide_max = 32.0;
                                let slide_offset_x =
                                    (1.0 - in_progress) * slide_max
                                        + exit_progress * slide_max;
                                toasts.push(ToastRender {
                                    id: entry.id,
                                    card_w: t_n.width,
                                    card_h,
                                    icon: entry.icon.clone(),
                                    icon_size: t_n.icon_size,
                                    title_buf,
                                    body_bufs,
                                    time_buf,
                                    buttons,
                                    target_y: layout_y,
                                    current_y: entry.current_y,
                                    slide_offset_x,
                                    alpha,
                                });
                                layout_y += card_h as f32 + 10.0; // GAP=10
                            }
                            // Interp current_y → target_y via reflow anim.
                            // Atualiza o state real (não snapshot) com novo
                            // target/current/anim_started.
                            if let Ok(mut g) = data.state.notifications.lock() {
                                let reflow_anim = crate::notifications::REFLOW_ANIM_MS;
                                for (i, entry) in g.visible.iter_mut().enumerate() {
                                    if let Some(t) = toasts.get_mut(i) {
                                        // Se target mudou (mais de 1px), inicia anim.
                                        if (entry.target_y - t.target_y).abs() > 1.0 {
                                            entry.anim_started_at = std::time::Instant::now();
                                            entry.anim_start_y = entry.current_y;
                                            entry.target_y = t.target_y;
                                        }
                                        // Interp.
                                        let anim_ms =
                                            entry.anim_started_at.elapsed().as_millis() as u64;
                                        let p = if reflow_anim == 0 {
                                            1.0
                                        } else {
                                            crate::notifications::ease_out_cubic(
                                                anim_ms as f32 / reflow_anim as f32,
                                            )
                                        };
                                        entry.current_y = entry.anim_start_y
                                            + (entry.target_y - entry.anim_start_y) * p;
                                        t.current_y = entry.current_y;
                                    }
                                }
                            }
                            // Rect cache pro hover/click. Inclui buttons.
                            data.state.notification_rects.clear();
                            data.state.notification_button_rects.clear();
                            // Render.
                            let mk_alpha_n =
                                |buf: &MemoryRenderBuffer,
                                 x: i32,
                                 y: i32,
                                 alpha: f32,
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
                                        Some(alpha),
                                        None,
                                        None,
                                        Kind::Unspecified,
                                    )
                                    .ok()
                                };
                            // H54.P3.shadow: cache de sombras por card_h
                            // (contact + ambient). Gera 1x por tamanho —
                            // reflow só translada, não recalcula gaussian.
                            use std::collections::HashMap;
                            use std::sync::{Mutex, OnceLock};
                            static SHADOW_CACHE: OnceLock<
                                Mutex<HashMap<i32, (MemoryRenderBuffer, MemoryRenderBuffer)>>,
                            > = OnceLock::new();
                            let cache = SHADOW_CACHE
                                .get_or_init(|| Mutex::new(HashMap::new()));
                            let get_shadow = |card_h: i32| -> Option<(MemoryRenderBuffer, MemoryRenderBuffer)> {
                                let mut g = cache.lock().ok()?;
                                if let Some(pair) = g.get(&card_h) {
                                    return Some(pair.clone());
                                }
                                let contact = crate::gfx::make_drop_shadow(
                                    t_n.width,
                                    card_h,
                                    t_n.corner,
                                    t_n.shadow_contact_blur,
                                    t_n.shadow_color_contact,
                                );
                                let ambient = crate::gfx::make_drop_shadow(
                                    t_n.width,
                                    card_h,
                                    t_n.corner,
                                    t_n.shadow_ambient_blur,
                                    t_n.shadow_color_ambient,
                                );
                                let pair = (contact.clone(), ambient.clone());
                                g.insert(card_h, pair.clone());
                                Some(pair)
                            };

                            for t in &toasts {
                                let card_x = output_size_logical.w
                                    - t_n.margin
                                    - t.card_w
                                    + t.slide_offset_x as i32;
                                let card_y = t.current_y as i32;
                                // Layer 0 (BACK, push pra END do vec):
                                // sombras macOS-style. Ambient grande +
                                // contact pequena. Sombras NÃO aparecem na
                                // notification_rects (input region) — clique
                                // na área da sombra passa pra wallpaper/janela.
                                if let Some((contact, ambient)) = get_shadow(t.card_h) {
                                    // Ambient (mais difusa) primeiro =
                                    // mais ATRÁS no vec (drawn first).
                                    let ax = card_x - t_n.shadow_ambient_blur;
                                    let ay = card_y - t_n.shadow_ambient_blur
                                        + t_n.shadow_ambient_offset_y;
                                    let aloc: smithay::utils::Point<
                                        i32,
                                        smithay::utils::Physical,
                                    > = smithay::utils::Point::<
                                        i32,
                                        smithay::utils::Logical,
                                    >::from((ax, ay))
                                        .to_physical_precise_round(1.0);
                                    if let Ok(elem) =
                                        MemoryRenderBufferRenderElement::from_buffer(
                                            renderer,
                                            aloc.to_f64(),
                                            &ambient,
                                            Some(t.alpha),
                                            None,
                                            None,
                                            Kind::Unspecified,
                                        )
                                    {
                                        custom_elements
                                            .push(LumaRenderElement::Memory(elem));
                                    }
                                    let cx = card_x - t_n.shadow_contact_blur;
                                    let cy = card_y - t_n.shadow_contact_blur
                                        + t_n.shadow_contact_offset_y;
                                    let cloc: smithay::utils::Point<
                                        i32,
                                        smithay::utils::Physical,
                                    > = smithay::utils::Point::<
                                        i32,
                                        smithay::utils::Logical,
                                    >::from((cx, cy))
                                        .to_physical_precise_round(1.0);
                                    if let Ok(elem) =
                                        MemoryRenderBufferRenderElement::from_buffer(
                                            renderer,
                                            cloc.to_f64(),
                                            &contact,
                                            Some(t.alpha),
                                            None,
                                            None,
                                            Kind::Unspecified,
                                        )
                                    {
                                        custom_elements
                                            .push(LumaRenderElement::Memory(elem));
                                    }
                                }
                                // Layer 1 (BACK do toast): bg translúcido — "blur"
                                // estético.
                                let bg = crate::gfx::make_rounded_panel(
                                    t.card_w, t.card_h, t_n.corner, t_n.bg,
                                );
                                if let Some(elem) =
                                    mk_alpha_n(&bg, card_x, card_y, t.alpha, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                // Layer 2 (middle): scrim opaco —
                                // garante piso de contraste. H54.P3.fix2.
                                let scrim = crate::gfx::make_rounded_panel(
                                    t.card_w, t.card_h, t_n.corner, t_n.scrim,
                                );
                                if let Some(elem) =
                                    mk_alpha_n(&scrim, card_x, card_y, t.alpha, renderer)
                                {
                                    custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                }
                                if let Some(icon_buf) = t.icon.as_ref() {
                                    // H54.P3.fix: icon top-align (mesma
                                    // linha do title) — não centralizado
                                    // verticalmente. Consistente entre
                                    // toasts de 1 e 3 linhas.
                                    let ix = card_x + t_n.padding;
                                    let iy = card_y + t_n.padding;
                                    if let Some(elem) =
                                        mk_alpha_n(icon_buf, ix, iy, t.alpha, renderer)
                                    {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }
                                let title_x = card_x + t_n.padding + t.icon_size + t_n.padding;
                                let title_y = card_y + t_n.padding;
                                let title_h = t.title_buf.as_ref().map(|(_, _, h)| *h).unwrap_or(16);
                                if let Some((tbuf, _, _)) = t.title_buf.as_ref() {
                                    if let Some(elem) =
                                        mk_alpha_n(tbuf, title_x, title_y, t.alpha, renderer)
                                    {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }
                                let body_y_start = title_y + title_h + 4;
                                let mut body_y = body_y_start;
                                let mut body_h_total = 0;
                                for (bbuf, _bw, bh) in t.body_bufs.iter() {
                                    if let Some(elem) =
                                        mk_alpha_n(bbuf, title_x, body_y, t.alpha, renderer)
                                    {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    body_y += bh + 2;
                                    body_h_total += bh + 2;
                                }
                                let time_y = body_y_start + body_h_total + 2;
                                if let Some((tbuf, _, _)) = t.time_buf.as_ref() {
                                    if let Some(elem) =
                                        mk_alpha_n(tbuf, title_x, time_y, t.alpha, renderer)
                                    {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }
                                // H54.P4: action buttons.
                                let hover_pair: Option<(u32, &str)> = data
                                    .state
                                    .notification_button_hover
                                    .as_ref()
                                    .map(|(i, k)| (*i, k.as_str()));
                                for btn in t.buttons.iter() {
                                    let (lx, ly, lw, lh) = btn.rect_local;
                                    let bx = card_x + lx;
                                    let by = card_y + ly;
                                    let is_hover = hover_pair == Some((t.id, btn.key.as_str()));
                                    let bg_color = if is_hover {
                                        t_n.action_btn_bg_hover
                                    } else {
                                        t_n.action_btn_bg
                                    };
                                    let bg_btn = crate::gfx::make_rounded_panel(
                                        lw, lh, t_n.action_btn_corner, bg_color,
                                    );
                                    if let Some(elem) =
                                        mk_alpha_n(&bg_btn, bx, by, t.alpha, renderer)
                                    {
                                        custom_elements
                                            .insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    // Label centralizada.
                                    let label_x = bx + (lw - btn.label_w) / 2;
                                    let label_y = by + (lh - btn.label_h) / 2;
                                    if let Some(elem) = mk_alpha_n(
                                        &btn.label_buf,
                                        label_x,
                                        label_y,
                                        t.alpha,
                                        renderer,
                                    ) {
                                        custom_elements
                                            .insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    // Cache do rect pro hit-test.
                                    data.state.notification_button_rects.push((
                                        t.id,
                                        btn.key.clone(),
                                        bx,
                                        by,
                                        lw,
                                        lh,
                                    ));
                                }
                                data.state.notification_rects.push((
                                    t.id, card_x, card_y, t.card_w, t.card_h,
                                ));
                            }
                            let _ = visible_snap; // explícito: snapshot só
                        } else {
                            data.state.notification_rects.clear();
                            data.state.notification_button_rects.clear();
                            data.state.notification_button_hover = None;
                        }

                        // H54.P5: central de notificações (painel borda direita).
                        // Renderiza acima dos toasts quando open_factor > 0.
                        {
                            let (history_snap, dnd_on, sound_on, panel_open, open_factor) =
                                match data.state.notifications.lock() {
                                    Ok(g) => (
                                        g.history.iter().cloned().collect::<Vec<_>>(),
                                        g.dnd_enabled,
                                        g.sound_enabled,
                                        g.panel_open,
                                        crate::notifications::panel_open_factor(&g),
                                    ),
                                    Err(_) => (Vec::new(), false, true, false, 0.0),
                                };
                            if open_factor <= 0.001 {
                                data.state.notification_panel_rect = None;
                                data.state.notification_dnd_rect = None;
                                data.state.notification_clear_rect = None;
                                data.state.notification_sound_rect = None;
                                data.state.notification_history_dismiss_rects.clear();
                                data.state.notification_history_hover = None;
                                let _ = sound_on;
                            } else {
                                let t_n = crate::theme::current().notifications.clone();
                                let text_r = data.state.text.as_ref();
                                let panel_w = t_n.panel_width;
                                // H54.P5.fix.taskbar: para na borda de cima da
                                // taskbar — não sobrepõe os ícones do tray.
                                let panel_h = output_size_logical.h
                                    - crate::taskbar::taskbar_height();
                                // Slide: x vai de output_w (factor=0, fora) até
                                // output_w - panel_w (factor=1, dentro).
                                let panel_x = output_size_logical.w
                                    - ((panel_w as f32) * open_factor) as i32;
                                let panel_y = 0;
                                // Cacheia rect só quando totalmente aberto.
                                if panel_open && open_factor >= 0.999 {
                                    data.state.notification_panel_rect =
                                        Some((panel_x, panel_y, panel_w, panel_h));
                                } else {
                                    data.state.notification_panel_rect = None;
                                }

                                let mut push_buf = |elems: &mut Vec<LumaRenderElement<GlesRenderer>>,
                                                    buf: &MemoryRenderBuffer,
                                                    x: i32,
                                                    y: i32,
                                                    r: &mut GlesRenderer,
                                                    front: bool| {
                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((x, y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        r, loc.to_f64(), buf, None, None, None,
                                        Kind::Unspecified,
                                    ) {
                                        if front {
                                            elems.insert(0, LumaRenderElement::Memory(elem));
                                        } else {
                                            elems.push(LumaRenderElement::Memory(elem));
                                        }
                                    }
                                };

                                // H54.P5.fix: sombra esquerda 2 layers (contact
                                // + ambient) com decay gaussiano horizontal —
                                // mesma linguagem e tokens dos toasts.
                                let ambient_blur = t_n.shadow_ambient_blur;
                                if ambient_blur > 0 {
                                    let amb = crate::gfx::make_left_edge_shadow(
                                        ambient_blur, panel_h, t_n.shadow_color_ambient,
                                    );
                                    push_buf(
                                        &mut custom_elements,
                                        &amb,
                                        panel_x - ambient_blur,
                                        panel_y,
                                        renderer,
                                        false,
                                    );
                                }
                                let contact_blur = t_n.shadow_contact_blur;
                                if contact_blur > 0 {
                                    let con = crate::gfx::make_left_edge_shadow(
                                        contact_blur, panel_h, t_n.shadow_color_contact,
                                    );
                                    push_buf(
                                        &mut custom_elements,
                                        &con,
                                        panel_x - contact_blur,
                                        panel_y,
                                        renderer,
                                        false,
                                    );
                                }
                                // BG translúcido + scrim opaco — mesma linguagem
                                // dos toasts mas valores mais sólidos.
                                let bg_buf = crate::gfx::make_rounded_panel(
                                    panel_w, panel_h, 0, t_n.panel_bg,
                                );
                                push_buf(&mut custom_elements, &bg_buf, panel_x, panel_y, renderer, true);
                                let scrim_buf = crate::gfx::make_rounded_panel(
                                    panel_w, panel_h, 0, t_n.panel_scrim,
                                );
                                push_buf(&mut custom_elements, &scrim_buf, panel_x, panel_y, renderer, true);

                                // Header: título + DND + "Limpar".
                                let header_h = t_n.panel_header_height;
                                let pad = t_n.panel_padding;
                                if let Some(tr) = text_r {
                                    // Título à esquerda, vertical center.
                                    let (px, w, h) = tr.rasterize_colored(
                                        "Notificações",
                                        t_n.panel_header_title_size,
                                        t_n.panel_header_title_color,
                                    );
                                    if w > 0 && h > 0 {
                                        let title_buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let tx = panel_x + pad;
                                        let ty = panel_y + (header_h - h as i32) / 2;
                                        push_buf(&mut custom_elements, &title_buf, tx, ty, renderer, true);
                                    }
                                    // Botões header (DND + Limpar) à direita.
                                    let btn_h = t_n.panel_btn_height;
                                    let btn_y = panel_y + (header_h - btn_h) / 2;
                                    // Direita pra esquerda: "Limpar" primeiro.
                                    let (clr_px, clr_w, clr_h) = tr.rasterize_colored(
                                        "Limpar",
                                        t_n.panel_btn_label_size,
                                        t_n.panel_btn_label_color,
                                    );
                                    let clr_btn_w = clr_w as i32 + t_n.panel_btn_padding_h * 2;
                                    let clr_x = panel_x + panel_w - pad - clr_btn_w;
                                    let clear_btn_bg = crate::gfx::make_rounded_panel(
                                        clr_btn_w, btn_h, t_n.panel_btn_corner, t_n.panel_btn_bg,
                                    );
                                    push_buf(&mut custom_elements, &clear_btn_bg, clr_x, btn_y, renderer, true);
                                    if clr_w > 0 && clr_h > 0 {
                                        let lbl_buf = MemoryRenderBuffer::from_slice(
                                            &clr_px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (clr_w as i32, clr_h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let lx = clr_x + (clr_btn_w - clr_w as i32) / 2;
                                        let ly = btn_y + (btn_h - clr_h as i32) / 2;
                                        push_buf(&mut custom_elements, &lbl_buf, lx, ly, renderer, true);
                                    }
                                    if panel_open && open_factor >= 0.999 {
                                        data.state.notification_clear_rect =
                                            Some((clr_x, btn_y, clr_btn_w, btn_h));
                                    } else {
                                        data.state.notification_clear_rect = None;
                                    }
                                    // DND toggle à esquerda do Limpar.
                                    let dnd_label = if dnd_on { "DND on" } else { "DND" };
                                    let (dnd_px, dnd_w, dnd_h) = tr.rasterize_colored(
                                        dnd_label,
                                        t_n.panel_btn_label_size,
                                        t_n.panel_btn_label_color,
                                    );
                                    let dnd_btn_w = dnd_w as i32 + t_n.panel_btn_padding_h * 2;
                                    let dnd_x = clr_x - 10 - dnd_btn_w;
                                    let dnd_bg_color = if dnd_on {
                                        t_n.panel_dnd_active_bg
                                    } else {
                                        t_n.panel_btn_bg
                                    };
                                    let dnd_btn_bg = crate::gfx::make_rounded_panel(
                                        dnd_btn_w, btn_h, t_n.panel_btn_corner, dnd_bg_color,
                                    );
                                    push_buf(&mut custom_elements, &dnd_btn_bg, dnd_x, btn_y, renderer, true);
                                    if dnd_w > 0 && dnd_h > 0 {
                                        let lbl_buf = MemoryRenderBuffer::from_slice(
                                            &dnd_px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (dnd_w as i32, dnd_h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let lx = dnd_x + (dnd_btn_w - dnd_w as i32) / 2;
                                        let ly = btn_y + (btn_h - dnd_h as i32) / 2;
                                        push_buf(&mut custom_elements, &lbl_buf, lx, ly, renderer, true);
                                    }
                                    if panel_open && open_factor >= 0.999 {
                                        data.state.notification_dnd_rect =
                                            Some((dnd_x, btn_y, dnd_btn_w, btn_h));
                                    } else {
                                        data.state.notification_dnd_rect = None;
                                    }
                                    // H54.P6: toggle de som à esquerda do DND.
                                    // Independente do DND — sound off deixa
                                    // toast aparecer mas mudo.
                                    let snd_label = if sound_on { "Som" } else { "Som off" };
                                    let (snd_px, snd_w, snd_h) = tr.rasterize_colored(
                                        snd_label,
                                        t_n.panel_btn_label_size,
                                        t_n.panel_btn_label_color,
                                    );
                                    let snd_btn_w = snd_w as i32 + t_n.panel_btn_padding_h * 2;
                                    let snd_x = dnd_x - 10 - snd_btn_w;
                                    // Off destaca com cor diferente (não amber
                                    // como DND — "som off" não é estado de
                                    // alerta, só estado).
                                    let snd_bg = if sound_on {
                                        t_n.panel_btn_bg
                                    } else {
                                        t_n.panel_dnd_active_bg
                                    };
                                    let snd_bg_buf = crate::gfx::make_rounded_panel(
                                        snd_btn_w, btn_h, t_n.panel_btn_corner, snd_bg,
                                    );
                                    push_buf(&mut custom_elements, &snd_bg_buf, snd_x, btn_y, renderer, true);
                                    if snd_w > 0 && snd_h > 0 {
                                        let lbl_buf = MemoryRenderBuffer::from_slice(
                                            &snd_px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (snd_w as i32, snd_h as i32),
                                            1,
                                            Transform::Normal,
                                            None,
                                        );
                                        let lx = snd_x + (snd_btn_w - snd_w as i32) / 2;
                                        let ly = btn_y + (btn_h - snd_h as i32) / 2;
                                        push_buf(&mut custom_elements, &lbl_buf, lx, ly, renderer, true);
                                    }
                                    if panel_open && open_factor >= 0.999 {
                                        data.state.notification_sound_rect =
                                            Some((snd_x, btn_y, snd_btn_w, btn_h));
                                    } else {
                                        data.state.notification_sound_rect = None;
                                    }
                                } else {
                                    data.state.notification_clear_rect = None;
                                    data.state.notification_dnd_rect = None;
                                    data.state.notification_sound_rect = None;
                                }

                                // Lista de itens (ou empty state). H54.P5.fix:
                                // altura dinâmica por item — body wrapped até
                                // panel_item_max_body_lines com "…" no overflow.
                                data.state.notification_history_dismiss_rects.clear();
                                data.state.notification_history_item_rects.clear();
                                let list_top_y = panel_y + header_h + t_n.panel_item_gap;
                                let item_gap = t_n.panel_item_gap;
                                let hover_id = data.state.notification_history_hover;
                                if history_snap.is_empty() {
                                    if let Some(tr) = text_r {
                                        let (px, w, h) = tr.rasterize_colored(
                                            "Sem notificações",
                                            t_n.panel_empty_label_size,
                                            t_n.panel_empty_label_color,
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
                                            let ex = panel_x + (panel_w - w as i32) / 2;
                                            let ey = panel_y + panel_h / 3;
                                            push_buf(&mut custom_elements, &buf, ex, ey, renderer, true);
                                        }
                                    }
                                } else if let Some(tr) = text_r {
                                    let inner_pad = t_n.panel_item_padding;
                                    let icon_dst = t_n.panel_item_icon_size;
                                    let item_x = panel_x + pad;
                                    let item_w = panel_w - pad * 2;
                                    // Largura do texto disponível: subtrai
                                    // ícone, paddings internos, e reserva
                                    // espaço pro time (~50px).
                                    let text_left_local = inner_pad + icon_dst + inner_pad;
                                    let text_w = (item_w - text_left_local - inner_pad - 50).max(20);
                                    let mut cur_y = list_top_y;
                                    for entry in history_snap.iter() {
                                        // Wrap título (1 linha com "…" se sobrar)
                                        // e body (até max_body_lines).
                                        let title_lines = wrap_to_lines_pixels(
                                            &entry.summary,
                                            tr,
                                            t_n.panel_item_title_size,
                                            t_n.panel_item_title_color,
                                            text_w,
                                            1,
                                        );
                                        let body_lines = wrap_to_lines_pixels(
                                            &entry.body,
                                            tr,
                                            t_n.panel_item_body_size,
                                            t_n.panel_item_body_color,
                                            text_w,
                                            t_n.panel_item_max_body_lines,
                                        );
                                        // Altura de uma linha de cada (mede o "M"
                                        // só pra ter h estável).
                                        let title_line_h = tr
                                            .rasterize_colored(
                                                "M",
                                                t_n.panel_item_title_size,
                                                t_n.panel_item_title_color,
                                            )
                                            .2 as i32;
                                        let body_line_h = tr
                                            .rasterize_colored(
                                                "M",
                                                t_n.panel_item_body_size,
                                                t_n.panel_item_body_color,
                                            )
                                            .2 as i32;
                                        let body_gap = 2;
                                        let text_block_h = title_line_h
                                            + 4
                                            + (body_lines.len() as i32)
                                                * (body_line_h + body_gap);
                                        let item_h = (icon_dst + inner_pad * 2)
                                            .max(text_block_h + inner_pad * 2);
                                        if cur_y + item_h > panel_h - pad {
                                            break;
                                        }
                                        let item_y = cur_y;
                                        if panel_open && open_factor >= 0.999 {
                                            data.state.notification_history_item_rects.push((
                                                entry.id, item_x, item_y, item_w, item_h,
                                            ));
                                        }
                                        let is_hovered = hover_id == Some(entry.id);
                                        if is_hovered {
                                            let hover_bg = crate::gfx::make_rounded_panel(
                                                item_w, item_h, t_n.panel_item_corner,
                                                t_n.panel_item_hover_bg,
                                            );
                                            push_buf(&mut custom_elements, &hover_bg, item_x, item_y, renderer, true);
                                        }
                                        // Ícone.
                                        let icon_x = item_x + inner_pad;
                                        let icon_y = item_y + (item_h - icon_dst) / 2;
                                        if let Some(icon_buf) = entry.icon.as_ref() {
                                            let icon_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((icon_x, icon_y))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, icon_loc.to_f64(), icon_buf, None, None,
                                                Some(smithay::utils::Size::from((icon_dst, icon_dst))),
                                                Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                        let text_left_x = icon_x + icon_dst + inner_pad;
                                        // Time (top right).
                                        let time_str = crate::notifications::relative_time(entry.received_at);
                                        let (t_px, t_w_u, t_h_u) = tr.rasterize_colored(
                                            &time_str,
                                            t_n.panel_item_time_size,
                                            t_n.panel_item_time_color,
                                        );
                                        let time_right_x = item_x + item_w - inner_pad;
                                        let time_x = time_right_x - t_w_u as i32;
                                        let time_y = item_y + inner_pad;
                                        if t_w_u > 0 && t_h_u > 0 {
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &t_px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (t_w_u as i32, t_h_u as i32),
                                                1,
                                                Transform::Normal,
                                                None,
                                            );
                                            push_buf(&mut custom_elements, &buf, time_x, time_y, renderer, true);
                                        }
                                        // Título (1 linha já truncada com "…").
                                        if let Some(title_line) = title_lines.first() {
                                            let (px, w, h) = tr.rasterize_colored(
                                                title_line,
                                                t_n.panel_item_title_size,
                                                t_n.panel_item_title_color,
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
                                                let ti_x = text_left_x;
                                                let ti_y = item_y + inner_pad;
                                                push_buf(&mut custom_elements, &buf, ti_x, ti_y, renderer, true);
                                            }
                                        }
                                        // Body (até 2 linhas, "…" se overflow).
                                        let body_top = item_y + inner_pad + title_line_h + 4;
                                        for (li, line) in body_lines.iter().enumerate() {
                                            let (px, w, h) = tr.rasterize_colored(
                                                line,
                                                t_n.panel_item_body_size,
                                                t_n.panel_item_body_color,
                                            );
                                            if w == 0 || h == 0 {
                                                continue;
                                            }
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1,
                                                Transform::Normal,
                                                None,
                                            );
                                            let bx = text_left_x;
                                            let by = body_top
                                                + (li as i32) * (body_line_h + body_gap);
                                            push_buf(&mut custom_elements, &buf, bx, by, renderer, true);
                                        }
                                        // Dismiss X só no item sob hover.
                                        if is_hovered {
                                            let d_size = t_n.panel_item_dismiss_size;
                                            let d_x = item_x + item_w - inner_pad - d_size;
                                            let d_y = item_y + item_h - inner_pad - d_size;
                                            let (px, w, h) = tr.rasterize_colored(
                                                "×",
                                                t_n.panel_btn_label_size + 4.0,
                                                t_n.panel_item_dismiss_color,
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
                                                let lx = d_x + (d_size - w as i32) / 2;
                                                let ly = d_y + (d_size - h as i32) / 2;
                                                push_buf(&mut custom_elements, &buf, lx, ly, renderer, true);
                                            }
                                            if panel_open && open_factor >= 0.999 {
                                                data.state.notification_history_dismiss_rects.push((
                                                    entry.id, d_x, d_y, d_size, d_size,
                                                ));
                                            }
                                        }
                                        cur_y = item_y + item_h + item_gap;
                                    }
                                }
                            }
                        }

                        // H54.P7: piscada de tela (visual bell). Overlay
                        // full-screen colorido, alpha animada por
                        // flash_alpha_now. Inserido em insert(0) =
                        // FRENTE de tudo, exceto cursor. Pulso único,
                        // protegido por debounce no trigger.
                        {
                            let t_n_flash = crate::theme::current().notifications.clone();
                            let flash = match data.state.notifications.lock() {
                                Ok(g) => crate::notifications::flash_alpha_now(
                                    &g, &t_n_flash,
                                ),
                                Err(_) => None,
                            };
                            if let Some((color, alpha)) = flash {
                                if alpha > 0.0 {
                                    // make_rounded_panel já trata color.a()
                                    // como alpha multiplicador. Construímos
                                    // a cor com alpha computado embutido.
                                    let blended = smithay::backend::renderer::Color32F::new(
                                        color.r(), color.g(), color.b(), alpha,
                                    );
                                    let overlay = crate::gfx::make_rounded_panel(
                                        output_size_logical.w,
                                        output_size_logical.h,
                                        0,
                                        blended,
                                    );
                                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((0, 0))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, loc.to_f64(), &overlay,
                                        None, None, None,
                                        Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
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

                        // H58 fatia 3+4: render lock screen.
                        // 2 estados sobre o mesmo fundo:
                        // - capa: relógio grande centro + dica
                        // - auth: relógio pequeno no topo + painel
                        //   central (avatar + saudação + senha)
                        if data.state.locked {
                            use chrono::{Datelike, Local, Timelike};
                            let t_lock = crate::theme::current().lock.clone();
                            let revealed = data.state.lock_revealed;
                            // Bg full screen.
                            let overlay = crate::gfx::make_rounded_panel(
                                output_size_logical.w,
                                output_size_logical.h,
                                0,
                                t_lock.bg_color,
                            );
                            let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((0, 0))
                                    .to_physical_precise_round(1.0);
                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                renderer, loc.to_f64(), &overlay,
                                None, None, None, Kind::Unspecified,
                            ) {
                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                            }
                            if let Some(tr) = data.state.text.as_ref() {
                                let now = Local::now();
                                let hour_str = format!("{:02}:{:02}", now.hour(), now.minute());
                                let dias_extenso = [
                                    "Domingo","Segunda","Terça","Quarta","Quinta","Sexta","Sábado",
                                ];
                                let meses_lower = [
                                    "janeiro","fevereiro","março","abril","maio","junho",
                                    "julho","agosto","setembro","outubro","novembro","dezembro",
                                ];
                                let dow_idx = now.weekday().num_days_from_sunday() as usize;
                                let mes_idx = (now.month() as usize).saturating_sub(1);
                                let date_str = format!(
                                    "{}, {} de {}",
                                    dias_extenso.get(dow_idx).copied().unwrap_or(""),
                                    now.day(),
                                    meses_lower.get(mes_idx).copied().unwrap_or(""),
                                );
                                let center_x = output_size_logical.w / 2;
                                let center_y = output_size_logical.h / 2;
                                let hour_size = if revealed { t_lock.auth_hour_size } else { t_lock.hour_size };
                                let date_size = if revealed { t_lock.auth_date_size } else { t_lock.date_size };
                                // Hora.
                                let (px, hw, hh) = tr.rasterize_colored(
                                    &hour_str, hour_size, t_lock.hour_color,
                                );
                                let hour_h = hh as i32;
                                let hour_y = if revealed {
                                    // Topo: ~12% da altura.
                                    output_size_logical.h / 8
                                } else {
                                    center_y - hour_h / 2 - 20
                                };
                                if hw > 0 && hh > 0 {
                                    let buf = MemoryRenderBuffer::from_slice(
                                        &px,
                                        smithay::backend::allocator::Fourcc::Argb8888,
                                        (hw as i32, hh as i32),
                                        1, Transform::Normal, None,
                                    );
                                    let lx = center_x - (hw as i32) / 2;
                                    let tloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, hour_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, tloc.to_f64(), &buf,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }
                                // Data.
                                let (px, dw, dh) = tr.rasterize_colored(
                                    &date_str, date_size, t_lock.date_color,
                                );
                                let date_y = hour_y + hour_h + if revealed { 8 } else { 18 };
                                if dw > 0 && dh > 0 {
                                    let buf = MemoryRenderBuffer::from_slice(
                                        &px,
                                        smithay::backend::allocator::Fourcc::Argb8888,
                                        (dw as i32, dh as i32),
                                        1, Transform::Normal, None,
                                    );
                                    let lx = center_x - (dw as i32) / 2;
                                    let tloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, date_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, tloc.to_f64(), &buf,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                }
                                if !revealed {
                                    // Capa: dica após delay.
                                    let show_hint = data.state.locked_at
                                        .map(|t| t.elapsed().as_millis() as u64 >= t_lock.hint_delay_ms)
                                        .unwrap_or(false);
                                    if show_hint {
                                        let (px, w, h) = tr.rasterize_colored(
                                            &t_lock.hint_text,
                                            t_lock.hint_size,
                                            t_lock.hint_color,
                                        );
                                        if w > 0 && h > 0 {
                                            let buf = MemoryRenderBuffer::from_slice(
                                                &px,
                                                smithay::backend::allocator::Fourcc::Argb8888,
                                                (w as i32, h as i32),
                                                1, Transform::Normal, None,
                                            );
                                            let lx = center_x - (w as i32) / 2;
                                            let ly = output_size_logical.h - 80 - h as i32;
                                            let tloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                                smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, ly))
                                                    .to_physical_precise_round(1.0);
                                            if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                                renderer, tloc.to_f64(), &buf,
                                                None, None, None, Kind::Unspecified,
                                            ) {
                                                custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                            }
                                        }
                                    }
                                } else {
                                    // Auth panel: avatar + saudação + campo,
                                    // centralizado verticalmente (clock fica
                                    // no topo, painel no centro da tela).
                                    let user = std::env::var("USER")
                                        .unwrap_or_else(|_| "Usuário".to_string());
                                    let initial = user
                                        .chars()
                                        .next()
                                        .map(|c| c.to_uppercase().to_string())
                                        .unwrap_or_default();
                                    let h = now.hour();
                                    let saudacao = if (5..12).contains(&h) {
                                        "Bom dia"
                                    } else if (12..18).contains(&h) {
                                        "Boa tarde"
                                    } else {
                                        "Boa noite"
                                    };
                                    let greeting = format!("{}, {}", saudacao, user);
                                    // Calcula altura total do bloco:
                                    // avatar + 20 + greeting_h + 24 +
                                    // field_h + 12 + indicator_h.
                                    let block_h = t_lock.avatar_size
                                        + 20 + (t_lock.greeting_size as i32 + 4)
                                        + 24 + t_lock.field_height
                                        + 12 + (t_lock.indicator_size as i32 + 4);
                                    // Painel centralizado verticalmente
                                    // entre o fim do clock e o fim da tela.
                                    let panel_top_min = date_y + dh as i32 + 40;
                                    let block_y_center = output_size_logical.h / 2 - block_h / 2;
                                    let avatar_y = block_y_center.max(panel_top_min);
                                    let avatar_r = t_lock.avatar_size / 2;
                                    let avatar = crate::gfx::make_rounded_panel(
                                        t_lock.avatar_size, t_lock.avatar_size,
                                        avatar_r, t_lock.avatar_bg,
                                    );
                                    let ax = center_x - avatar_r;
                                    let aloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((ax, avatar_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, aloc.to_f64(), &avatar,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    // Inicial dentro do avatar.
                                    let (px, iw, ih) = tr.rasterize_colored(
                                        &initial, t_lock.avatar_initial_size,
                                        t_lock.avatar_initial_color,
                                    );
                                    if iw > 0 && ih > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (iw as i32, ih as i32),
                                            1, Transform::Normal, None,
                                        );
                                        let ix = center_x - (iw as i32) / 2;
                                        // Correção óptica: bbox da
                                        // fontdue inclui ascender acima
                                        // da capital, então centrar por
                                        // bbox deixa visualmente baixo.
                                        // ih/8 balanceia (ih/4 estava
                                        // alto demais, ih/6 baixo).
                                        let iy = avatar_y
                                            + (t_lock.avatar_size - ih as i32) / 2
                                            - (ih as i32) / 8;
                                        let iloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((ix, iy))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, iloc.to_f64(), &buf,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    // Saudação.
                                    let (px, gw, gh) = tr.rasterize_colored(
                                        &greeting, t_lock.greeting_size, t_lock.greeting_color,
                                    );
                                    let greet_y = avatar_y + t_lock.avatar_size + 20;
                                    if gw > 0 && gh > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (gw as i32, gh as i32),
                                            1, Transform::Normal, None,
                                        );
                                        let lx = center_x - (gw as i32) / 2;
                                        let tloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((lx, greet_y))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, tloc.to_f64(), &buf,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    // Campo de senha (bg + bullets).
                                    let field_y = greet_y + gh as i32 + 24;
                                    let field_x = center_x - t_lock.field_width / 2;
                                    // Border (outer rect).
                                    let border = crate::gfx::make_rounded_panel(
                                        t_lock.field_width + 4, t_lock.field_height + 4,
                                        t_lock.field_corner + 2,
                                        t_lock.field_border_focus,
                                    );
                                    let bloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((field_x - 2, field_y - 2))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, bloc.to_f64(), &border,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    let bg = crate::gfx::make_rounded_panel(
                                        t_lock.field_width, t_lock.field_height,
                                        t_lock.field_corner, t_lock.field_bg,
                                    );
                                    let fgloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((field_x, field_y))
                                            .to_physical_precise_round(1.0);
                                    if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                        renderer, fgloc.to_f64(), &bg,
                                        None, None, None, Kind::Unspecified,
                                    ) {
                                        custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                    }
                                    // Texto do campo (bullets ou placeholder).
                                    let pw = data.state.lock_password.clone();
                                    let (display, color) = if pw.is_empty() {
                                        (t_lock.field_placeholder.clone(), t_lock.field_placeholder_color)
                                    } else {
                                        let masked: String = "•".repeat(pw.chars().count());
                                        (masked, t_lock.field_text_color)
                                    };
                                    let (px, tw, th) = tr.rasterize_colored(
                                        &display, t_lock.field_text_size, color,
                                    );
                                    if tw > 0 && th > 0 {
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (tw as i32, th as i32),
                                            1, Transform::Normal, None,
                                        );
                                        let tx = field_x + 14;
                                        let ty = field_y + (t_lock.field_height - th as i32) / 2;
                                        let tloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((tx, ty))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, tloc.to_f64(), &buf,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                    }
                                    // Indicadores: Caps Lock (warn) + layout.
                                    let mut parts: Vec<(String, smithay::backend::renderer::Color32F)> =
                                        Vec::new();
                                    if data.state.lock_caps_on {
                                        parts.push((
                                            "Caps Lock ativado".to_string(),
                                            t_lock.indicator_warn_color,
                                        ));
                                    }
                                    // Layout placeholder — em PT-BR fixo
                                    // até XKB lookup real (fatia 6+).
                                    parts.push(("PT-BR".to_string(), t_lock.indicator_color));
                                    let ind_y = field_y + t_lock.field_height + 12;
                                    let mut ind_x = field_x;
                                    for (text, color) in parts.iter() {
                                        let (px, w, h) = tr.rasterize_colored(
                                            text, t_lock.indicator_size, *color,
                                        );
                                        if w == 0 || h == 0 { continue; }
                                        let buf = MemoryRenderBuffer::from_slice(
                                            &px,
                                            smithay::backend::allocator::Fourcc::Argb8888,
                                            (w as i32, h as i32),
                                            1, Transform::Normal, None,
                                        );
                                        let tloc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::from((ind_x, ind_y))
                                                .to_physical_precise_round(1.0);
                                        if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                                            renderer, tloc.to_f64(), &buf,
                                            None, None, None, Kind::Unspecified,
                                        ) {
                                            custom_elements.insert(0, LumaRenderElement::Memory(elem));
                                        }
                                        ind_x += w as i32 + 16;
                                    }
                                }
                            }
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
