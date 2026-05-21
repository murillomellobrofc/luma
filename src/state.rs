use std::{
    collections::HashMap,
    sync::Arc,
    time::Instant,
};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::element::memory::MemoryRenderBuffer,
    },
    desktop::{PopupManager, Space, WindowSurfaceType},
    input::{pointer::CursorImageStatus, Seat, SeatState},
    output::Output,
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopSignal, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        },
    },
    utils::{Logical, Point, Transform},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::{
            wlr_layer::WlrLayerShellState,
            xdg::{decoration::XdgDecorationState, XdgShellState},
        },
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};
use tracing::{info, warn};

#[allow(dead_code)]
pub struct LumaState {
    pub start_time: Instant,
    pub display_handle: DisplayHandle,
    pub socket_name: String,
    pub loop_signal: LoopSignal,

    pub space: Space<crate::space_item::SpaceItem>,
    pub popups: PopupManager,
    /// Outputs (monitores) registrados. Hoje sempre 1 (winit). Adicionado
    /// em H34.1 pra preparar suporte a multi-monitor — call sites ainda
    /// usam `space.outputs().next()` durante a transição. `primary_output()`
    /// é o helper canônico.
    pub outputs: Vec<Output>,
    /// Nome do output focado (cursor está nele OU recebeu último input).
    /// Default = primário no boot. Determina onde novas janelas spawnam
    /// e onde maximize/snap operam. Em multi-monitor, atualizado conforme
    /// o cursor cruza monitores. H34.6.
    pub focused_output_name: Option<String>,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub layer_shell_state: WlrLayerShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<LumaState>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<LumaState>,

    /// Última posição do ponteiro em coordenadas lógicas do output.
    pub pointer_location: Point<f64, Logical>,
    /// Cursor padrão carregado do tema XCursor. `None` se o sistema não
    /// tiver tema xcursor instalado — render desenha nada nesse caso.
    pub cursor: Option<crate::cursor::Cursor>,
    /// Buffers persistentes por nome de cursor. Cacheados na inicialização
    /// pra alternar o ativo (cursor_kind) sem reimportar textura.
    pub cursor_buffers: HashMap<&'static str, (MemoryRenderBuffer, Point<i32, Logical>)>,
    /// Nome do cursor atualmente ativo. Default: "default".
    pub cursor_kind: &'static str,
    /// Status atual do cursor pedido pelo cliente focado. `Named(_)` (default)
    /// usa nosso cursor XCursor padrão; `Surface(s)` faz o compositor
    /// renderizar a surface do cliente; `Hidden` esconde o cursor.
    pub cursor_status: CursorImageStatus,
    pub taskbar: crate::taskbar::TaskbarState,
    pub text: Option<crate::text::TextRenderer>,
    pub menu_open: bool,
    /// Camada fullscreen do menu híbrido (H32.10.2): backdrop Coffee
    /// 0.94 + linha âmbar 2px no topo. Substituirá o `menu` legacy.
    pub menu_fullscreen: crate::menu::render::FullscreenLayer,
    /// State do menu híbrido (H32.10.3): modo Favoritos/AllApps, zona
    /// com foco, query, seleções, 10 favoritos curados. Coexiste com
    /// `menu` legacy até H32.10.17.
    pub menu_new: crate::menu::state::FullscreenMenuState,
    /// Animação fade-in/out do menu híbrido (H32.10.5). Tickeada no
    /// começo de cada frame antes de renderizar a camada.
    pub menu_anim: crate::menu::animation::MenuAnimation,
    /// Greeting corner do menu híbrido (H32.10.6): avatar + nome +
    /// saudação dinâmica no canto sup-direito.
    pub greeting_corner: crate::menu::greeting_corner::GreetingCorner,
    /// Search bar pill do menu híbrido (H32.10.7).
    pub menu_search_bar: crate::menu::search_bar::SearchBar,
    /// Grid de favoritos curados 5×2 do menu híbrido (H32.10.8).
    pub favorites_grid: crate::menu::favorites_grid::FavoritesGrid,
    /// Grid 8×4 de todos os apps (H32.10.11).
    pub all_apps_grid: crate::menu::all_apps_grid::AllAppsGrid,
    /// Link "TODOS OS APPS →" + hint (H32.10.13).
    pub all_apps_link: crate::menu::all_apps_link::AllAppsLink,
    /// Session bar bottom com 5 botões + 4 places (H32.10.13).
    pub menu_session_bar: crate::menu::session_bar::SessionBar,
    /// Buffers da lista vertical de search results (H32.10.16).
    pub search_results_layer: crate::menu::search_results::SearchResultsLayer,
    /// Modal de confirmação Logout/Restart/Power (H32.10.fix.modal).
    pub confirm_modal: crate::menu::confirm_modal::ConfirmModal,
    /// Wallpaper carregado no boot, antes de ser movido pro Space.
    /// Vira `None` depois que o init_winit transfere ele pra um `Arc`
    /// dentro do Space.
    pub wallpaper: Option<crate::wallpaper::Wallpaper>,
    /// Arc compartilhado entre Space e backend pra atualizar
    /// output_size em resize.
    pub wallpaper_arc: Option<std::sync::Arc<crate::wallpaper::Wallpaper>>,
    /// Lista de apps descobertos via .desktop files, ordenada alfabética,
    /// cap em 16 (cabe na grid AllApps).
    pub apps: Vec<crate::apps::DesktopApp>,
    /// Ícones dos apps (Argb8888 premultiplicado + tamanho original) paralelo
    /// a `apps`. `None` pros que não tinham PNG localizável. O size original
    /// é necessário pra montar o `src` rect ao escalar pra tamanhos menores.
    pub app_icons: Vec<Option<crate::apps::AppIcon>>,
    /// Fallback usado pra apps sem ícone (`app_icons[i] == None`). Quadrado
    /// gradient Coffee→Amber com cantos arredondados. H32.7.fix.4.
    pub fallback_icon: crate::apps::AppIcon,
    /// Lugares (pastas XDG + Sistema) pra coluna direita do menu.
    pub places: Vec<crate::places::Place>,
    /// Tracking in-memory de apps usados recentemente. Mapa de
    /// `app_index → Instant` (timestamp da última abertura via menu).
    /// Usado pra desenhar a bolinha âmbar "recently used" no menu.
    /// Não persistido — esta sessão somente. H32.7.7.
    pub recently_used: std::collections::HashMap<usize, std::time::Instant>,
    /// Index lazy de pastas/arquivos em $HOME pra search. H32.9.5.
    pub home_index: crate::search::HomeIndex,
    /// Workspaces dinâmicos. H36.
    pub workspaces: crate::workspace::WorkspaceManager,
    /// True quando o overview screen está aberto. H36.6 + H36.11.
    pub overview_open: bool,
    /// Timestamp do início do dwell no hot corner top-left.
    /// Quando passa 100ms continuamente, abre o overview. H36.16.
    pub hot_corner_dwell_start: Option<std::time::Instant>,
    /// State visual do pager (buffers + hover). H36.7.
    pub pager: crate::pager::PagerState,
    /// State do system tray (battery, wifi, volume, notifications). H33.taskbar.9-12.
    pub tray: crate::tray::TrayState,
    /// True quando "Mostrar área de trabalho" foi acionado e todas as
    /// janelas estão minimizadas. Toggle via click no botão F (zona F)
    /// ou Super+D. H33.taskbar.F.16-17.
    pub desktop_visible: bool,
    /// Applet popup ativo (None = nenhum aberto). H44.volume.1.
    pub applet_popup: Option<crate::popup::AppletPopup>,
    /// Cache de output devices (sinks) pro popup de volume. H44.volume.5.
    pub audio_sinks: Vec<crate::audio::AudioDevice>,
    /// Cache de input devices (sources) pro popup de volume. H44.volume.9.
    pub audio_sources: Vec<crate::audio::AudioDevice>,
    /// Cache de streams (apps tocando audio). H44.volume.12.
    pub audio_streams: Vec<crate::audio::AudioStream>,
    /// Última atualização do cache audio (refresh quando popup aberto).
    /// H44.volume.17.
    pub audio_last_refresh: Option<std::time::Instant>,
    /// OSD ativo (volume ou brilho via teclas XF86). H45.osd.
    /// `None` = nenhum overlay; tick limpa quando expira.
    pub osd: Option<crate::osd::Osd>,
    /// OSD de mídia (now playing card) ativo. H46.osd-media.
    /// Tick limpa quando expira. Polling de 500ms enquanto visível.
    pub osd_media: Option<crate::osd::MediaOsd>,
    /// State visual do overview (buffers). H36.11.
    pub overview: crate::overview::OverviewState,
    /// Posições salvas das janelas movidas off-screen ao trocar de
    /// workspace. Vec em vez de HashMap porque Hash do WlSurface não é
    /// estável entre clones. H36.fix.4.
    pub saved_positions: Vec<(
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        smithay::utils::Point<i32, smithay::utils::Logical>,
    )>,
}

pub struct CalloopData {
    pub state: LumaState,
    pub display: Display<LumaState>,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

/// Deslocamento em pixels entre janelas em cascata.
const CASCADE_STEP: i32 = 40;
/// Margem mínima que sempre deve sobrar do output além da janela em cascata.
const CASCADE_MARGIN: i32 = 80;

impl LumaState {
    /// Output principal — sempre o primeiro do `outputs`. Panic se vazio.
    /// O caller é responsável por garantir que o output já foi registrado
    /// (acontece em `init_winit`). Em multi-monitor futuro, "primary"
    /// pode virar configurável.
    pub fn primary_output(&self) -> &Output {
        self.outputs.first().expect("nenhum output registrado")
    }

    /// Output cuja geometria contém o ponto `pos`. Se nenhum output
    /// contém o ponto (cursor fora de toda área de tela), cai no
    /// `primary_output()` como fallback. H34.5: helper pros call sites
    /// de input que precisam saber em qual monitor o cursor está.
    pub fn output_under_cursor(&self, pos: Point<f64, Logical>) -> &Output {
        self.outputs
            .iter()
            .find(|o| {
                self.space
                    .output_geometry(o)
                    .map(|g| g.to_f64().contains(pos))
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| self.primary_output())
    }

    /// Output focado — onde novas janelas spawnam e onde maximize/snap
    /// operam. Resolvido pelo `focused_output_name` (busca no Vec por
    /// nome). Fallback pro `primary_output()` se nome desconhecido ou
    /// não setado. H34.6.
    pub fn focused_output(&self) -> &Output {
        match &self.focused_output_name {
            Some(name) => self
                .outputs
                .iter()
                .find(|o| &o.name() == name)
                .unwrap_or_else(|| self.primary_output()),
            None => self.primary_output(),
        }
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space.element_under(pos).and_then(|(item, location)| {
            let luma_win = item.as_window()?;
            let offset = Point::<f64, Logical>::from((
                crate::decoration::BORDER_THICKNESS as f64,
                crate::decoration::HEADER_HEIGHT as f64,
            ));
            let in_window = pos - location.to_f64();
            if in_window.y < crate::decoration::HEADER_HEIGHT as f64
                || in_window.x < crate::decoration::BORDER_THICKNESS as f64
            {
                return None;
            }
            let in_surface = in_window - offset;
            luma_win
                .window()
                .surface_under(in_surface, WindowSurfaceType::ALL)
                .map(|(s, p)| (s, (p + location).to_f64() + offset))
        })
    }

    /// Aplica snap left/right (Aero-style): a janela ocupa metade do
    /// output (descontando taskbar e altura do header). Sem tracking de
    /// "snapped state" — usuário desfaz arrastando a janela pelo header
    /// pra outra posição. `left = true` snap pra esquerda, `false` pra
    /// direita.
    /// Toggle "Mostrar área de trabalho": se há alguma janela visível,
    /// minimiza todas (state.desktop_visible=true). Senão restaura todas
    /// (state.desktop_visible=false). Acionado por click no botão F da
    /// taskbar OU Super+D. H33.taskbar.F.17.
    pub fn toggle_show_desktop(&mut self) {
        let any_visible = self
            .space
            .elements()
            .filter_map(|i| i.as_window())
            .any(|w| !crate::decoration::is_minimized(w.window()));
        let windows: Vec<_> = self
            .space
            .elements()
            .filter_map(|i| i.as_window().cloned())
            .collect();
        for w in &windows {
            crate::decoration::with_decoration(w.window(), |d| d.minimized = any_visible);
        }
        if any_visible {
            if let Some(kb) = self.seat.get_keyboard() {
                let serial = smithay::utils::SERIAL_COUNTER.next_serial();
                kb.set_focus(
                    self,
                    None::<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
                    serial,
                );
            }
        }
        self.desktop_visible = any_visible;
        tracing::info!("show-desktop toggle: desktop_visible={any_visible}");
    }

    pub fn snap_half(&mut self, window: &crate::window::LumaWindow, left: bool) {
        let Some(toplevel) = window.window().toplevel().cloned() else {
            return;
        };
        let output = self.focused_output().clone();
        let Some(geometry) = self.space.output_geometry(&output) else {
            return;
        };

        let half_w = geometry.size.w / 2;
        let content_h = (geometry.size.h
            - crate::decoration::HEADER_HEIGHT
            - crate::taskbar::taskbar_height()
            - crate::decoration::BORDER_THICKNESS)
            .max(1);
        let content_w = (half_w - 2 * crate::decoration::BORDER_THICKNESS).max(1);

        let x = if left { 0 } else { half_w };
        toplevel.with_pending_state(|state| {
            state
                .states
                .unset(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Maximized);
            state.size = Some((content_w, content_h).into());
        });
        crate::decoration::with_decoration(window.window(), |d| d.restore_location = None);
        self.space.map_element(
            crate::space_item::SpaceItem::Window(window.clone()),
            smithay::utils::Point::from((x, 0)),
            true,
        );
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    /// Alterna o estado maximizado de uma janela. Se ela está maximizada,
    /// restaura pra posição salva e libera o tamanho. Se não, salva a
    /// posição atual e expande pra ocupar o output todo (descontando o
    /// header pra ele continuar visível no topo).
    pub fn toggle_maximize(&mut self, window: &crate::window::LumaWindow) {
        use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;

        let Some(toplevel) = window.window().toplevel().cloned() else {
            return;
        };

        let restore = crate::decoration::with_decoration(window.window(), |d| d.restore_location);
        let item = crate::space_item::SpaceItem::Window(window.clone());

        if let Some(saved_loc) = restore {
            toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Maximized);
                state.size = None;
            });
            crate::decoration::with_decoration(window.window(), |d| {
                d.restore_location = None
            });
            self.space.map_element(item, saved_loc, true);
        } else {
            let output = self.focused_output().clone();
            let Some(geometry) = self.space.output_geometry(&output) else {
                return;
            };
            let current_loc = self
                .space
                .element_location(&item)
                .unwrap_or_else(|| Point::from((0, 0)));
            crate::decoration::with_decoration(window.window(), |d| {
                d.restore_location = Some(current_loc)
            });
            let content_size = (
                geometry.size.w,
                (geometry.size.h
                    - crate::decoration::HEADER_HEIGHT
                    - crate::taskbar::taskbar_height())
                    .max(1),
            );
            toplevel.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Maximized);
                state.size = Some(content_size.into());
            });
            self.space.map_element(item, geometry.loc, true);
        }

        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    /// H36.3: troca o workspace ativo, movendo janelas dos outros pra
    /// off-screen (-1_000_000) e restaurando posições salvas quando
    /// volta. Retorna `true` se a troca aconteceu.
    pub fn switch_workspace(&mut self, target: usize) -> bool {
        if !self.workspaces.switch_to(target) {
            return false;
        }
        self.refresh_workspace_visibility();
        true
    }

    /// Move pra off-screen as janelas dos workspaces inativos e
    /// restaura as do current. Idempotente. Chamado após qualquer
    /// mudança de current ou de listas. H36.3.
    pub fn refresh_workspace_visibility(&mut self) {
        let current_keys: Vec<_> = self.workspaces.current().windows.clone();
        let items: Vec<_> = self.space.elements().cloned().collect();
        for item in items {
            let crate::space_item::SpaceItem::Window(lw) = &item else {
                continue;
            };
            let Some(surf) = lw.wl_surface() else {
                continue;
            };
            let cur_loc = self.space.element_location(&item);
            let belongs_to_current = current_keys.iter().any(|k| k == &surf);
            if belongs_to_current {
                // Restaura via Vec — find + remove pelo PartialEq.
                let saved = self
                    .saved_positions
                    .iter()
                    .position(|(k, _)| k == &surf)
                    .map(|idx| self.saved_positions.remove(idx).1);
                if let Some(pos) = saved {
                    self.space.map_element(item, pos, false);
                }
            } else if let Some(loc) = cur_loc {
                if loc.x > -100_000 {
                    self.saved_positions.push((surf, loc));
                    self.space.map_element(
                        item,
                        smithay::utils::Point::<i32, smithay::utils::Logical>::from((
                            -1_000_000, 0,
                        )),
                        false,
                    );
                }
            }
        }
    }

    /// Próxima posição pra uma janela nova em cascata. Y já é o topo do
    /// LumaWindow (header incluído), sem offset adicional.
    pub fn next_cascade_position(&self) -> Point<i32, Logical> {
        // Conta só janelas reais — wallpaper não entra na cascata.
        let count = self
            .space
            .elements()
            .filter(|i| i.as_window().is_some())
            .count() as i32;

        let output_size = self
            .space
            .output_geometry(self.focused_output())
            .map(|geo| geo.size)
            .unwrap_or((1280, 800).into());

        let max_x_slots =
            ((output_size.w - CASCADE_MARGIN).max(CASCADE_STEP) / CASCADE_STEP).max(1);
        let max_y_slots = ((output_size.h - CASCADE_MARGIN - crate::taskbar::taskbar_height())
            .max(CASCADE_STEP)
            / CASCADE_STEP)
            .max(1);

        let x = (count % max_x_slots) * CASCADE_STEP;
        let y = (count % max_y_slots) * CASCADE_STEP;
        Point::from((x, y))
    }

    pub fn new(
        event_loop: &mut EventLoop<'static, CalloopData>,
        display: &mut Display<LumaState>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let dh = display.handle();

        let compositor_state = CompositorState::new::<LumaState>(&dh);
        let xdg_shell_state = XdgShellState::new::<LumaState>(&dh);
        let xdg_decoration_state = XdgDecorationState::new::<LumaState>(&dh);
        let layer_shell_state = WlrLayerShellState::new::<LumaState>(&dh);
        let shm_state = ShmState::new::<LumaState>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<LumaState>(&dh);
        let data_device_state = DataDeviceState::new::<LumaState>(&dh);

        let mut seat_state: SeatState<LumaState> = SeatState::new();
        let mut seat = seat_state.new_wl_seat(&dh, "winit");
        seat.add_keyboard(Default::default(), 200, 25)?;
        seat.add_pointer();

        let loop_signal = event_loop.get_signal();
        init_wayland_dispatch(event_loop, display)?;

        let cursor = crate::cursor::Cursor::load();
        let mut cursor_buffers: HashMap<&'static str, (MemoryRenderBuffer, Point<i32, Logical>)> =
            HashMap::new();
        if let Some(c) = cursor.as_ref() {
            for &name in crate::cursor::NAMES {
                if let Some(img) = c.image_for(name) {
                    let buffer = MemoryRenderBuffer::from_slice(
                        &img.pixels_rgba,
                        Fourcc::Argb8888,
                        (img.width as i32, img.height as i32),
                        1,
                        Transform::Normal,
                        None,
                    );
                    cursor_buffers.insert(
                        name,
                        (buffer, Point::from((img.xhot as i32, img.yhot as i32))),
                    );
                }
            }
            if cursor_buffers.is_empty() {
                tracing::warn!("nenhum cursor XCursor carregado, cursor não será desenhado");
            }
        } else {
            tracing::warn!("tema XCursor não encontrado, cursor não será desenhado");
        }

        let mut state = Self {
            start_time: Instant::now(),
            display_handle: dh,
            socket_name: String::new(),
            loop_signal,
            space: Space::default(),
            popups: PopupManager::default(),
            outputs: Vec::new(),
            focused_output_name: None,
            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            layer_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            seat,
            pointer_location: Point::from((0.0, 0.0)),
            cursor,
            cursor_buffers,
            cursor_kind: "default",
            cursor_status: CursorImageStatus::default_named(),
            taskbar: crate::taskbar::TaskbarState::new(),
            text: crate::text::TextRenderer::load(),
            menu_open: false,
            menu_fullscreen: crate::menu::render::FullscreenLayer::new(),
            menu_new: crate::menu::state::FullscreenMenuState::new(),
            menu_anim: crate::menu::animation::MenuAnimation::new(),
            greeting_corner: crate::menu::greeting_corner::GreetingCorner::new(),
            menu_search_bar: crate::menu::search_bar::SearchBar::new(),
            favorites_grid: {
                let items = crate::menu::state::CuratedFavorite::defaults();
                crate::menu::favorites_grid::FavoritesGrid::new(&items)
            },
            all_apps_grid: crate::menu::all_apps_grid::AllAppsGrid::new(),
            all_apps_link: crate::menu::all_apps_link::AllAppsLink::default(),
            menu_session_bar: crate::menu::session_bar::SessionBar::new(),
            search_results_layer: crate::menu::search_results::SearchResultsLayer::new(),
            confirm_modal: crate::menu::confirm_modal::ConfirmModal::new(),
            apps: Vec::new(),       // populado abaixo
            app_icons: Vec::new(),  // populado abaixo
            fallback_icon: {
                let theme = crate::theme::current();
                let buffer = crate::gfx::make_app_fallback_icon(
                    48,
                    theme.colors.taskbar_bg_top,
                    theme.colors.accent_amber,
                    8,
                );
                (buffer, smithay::utils::Size::from((48, 48)))
            },
            places: crate::places::load(),
            recently_used: std::collections::HashMap::new(),
            home_index: crate::search::HomeIndex::new(
                crate::theme::current().search.home_index_cache_seconds,
            ),
            workspaces: crate::workspace::WorkspaceManager::new(),
            overview_open: false,
            hot_corner_dwell_start: None,
            pager: crate::pager::PagerState::new(),
            tray: crate::tray::TrayState::new(),
            desktop_visible: false,
            applet_popup: None,
            audio_sinks: Vec::new(),
            audio_sources: Vec::new(),
            audio_streams: Vec::new(),
            audio_last_refresh: None,
            osd: None,
            osd_media: None,
            overview: crate::overview::OverviewState::new(),
            saved_positions: Vec::new(),
            wallpaper: crate::wallpaper::Wallpaper::load(),
            wallpaper_arc: None,
        };
        let mut apps = crate::apps::load(16);
        crate::apps::promote_favorites(&mut apps);
        state.apps = apps;
        state.app_icons = crate::apps::load_icons(&state.apps);
        Ok(state)
    }
}

pub fn init_socket(
    event_loop: &mut EventLoop<'static, CalloopData>,
) -> Result<String, Box<dyn std::error::Error>> {
    let source = ListeningSocketSource::new_auto()?;
    let name = source.socket_name().to_string_lossy().into_owned();

    event_loop
        .handle()
        .insert_source(source, |stream, _, data| {
            match data
                .display
                .handle()
                .insert_client(stream, Arc::new(ClientState::default()))
            {
                Ok(_) => info!("cliente wayland conectado"),
                Err(err) => warn!("erro inserindo cliente wayland: {err}"),
            }
        })
        .map_err(|e| format!("inserir socket source falhou: {e}"))?;

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &name) };
    Ok(name)
}

fn init_wayland_dispatch(
    event_loop: &mut EventLoop<'static, CalloopData>,
    display: &mut Display<LumaState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let poll_fd = display.backend().poll_fd().try_clone_to_owned()?;
    let source = Generic::new(poll_fd, Interest::READ, Mode::Level);

    event_loop
        .handle()
        .insert_source(source, move |_, _, data| {
            data.display
                .dispatch_clients(&mut data.state)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            Ok(PostAction::Continue)
        })
        .map_err(|e| format!("inserir wayland dispatch source falhou: {e}"))?;
    Ok(())
}
