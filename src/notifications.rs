//! Servidor de notificações `org.freedesktop.Notifications` via D-Bus
//! + stacking de toasts no canto sup-direito. H54 P3.
//!
//! Comportamento env-gated: só registra como servidor D-Bus quando
//! `LUMA_TAKE_NOTIFICATIONS=1` está setado (não rouba notificações
//! do host KDE em modo aninhado). Pra debug visual sem servidor,
//! `LUMA_FAKE_NOTIFICATION=1` injeta 1 fake;
//! `LUMA_FAKE_NOTIFICATION=N` (N>1) injeta N fakes pra testar stack
//! + queue + reflow.
//!
//! Modelo do stack:
//! - `visible: Vec<NotificationEntry>` — até `MAX_VISIBLE_TOASTS` (4)
//!   renderizados no canto sup-direito, newest on top
//!   (`STACK_NEWEST_ON_TOP = true`).
//! - `queue: VecDeque` — overflow. Quando um visible fecha, pop_front.
//! - `hover_paused: bool` — qualquer hover sobre rect pausa TODOS os
//!   timers (per spec).
//!
//! Timer: elapsed-based via render frame cadence (luma chama
//! request_redraw a cada frame, então acompanha em ~16ms). Calloop
//! one-shot wake-on-idle é debt em DRM (DEBT.md).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

/// Máximo de toasts renderizados simultaneamente; resto vai pra fila.
/// H54.P3.b1.
pub const MAX_VISIBLE_TOASTS: usize = 4;

/// `true` = newer entries no topo (index 0 = mais novo). Token
/// funcional — inverter é trocar pra `false`. H54.P3.b1.
pub const STACK_NEWEST_ON_TOP: bool = true;

pub const ENTRY_ANIM_MS: u64 = 200;
pub const REFLOW_ANIM_MS: u64 = 180;
pub const EXIT_ANIM_MS: u64 = 150;

/// Urgência do notify (hint `urgency: y`). H54.P3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Low,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }
}

/// Entry no stack/queue. Cada entry tem timer próprio + posição
/// animada. H54.P3.
#[derive(Debug, Clone)]
pub struct NotificationEntry {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub icon: Option<MemoryRenderBuffer>,
    pub icon_size: smithay::utils::Size<i32, smithay::utils::Logical>,
    /// Quando entrou no estado visible (ou foi resetada por replaces_id).
    pub received_at: Instant,
    /// Timeout: `Some(ms)` ou `None` se nunca expira (critical +
    /// expire_timeout=0). H54.P3.
    pub timeout_ms: Option<u64>,
    /// Total acumulado de tempo pausado — descontado do elapsed
    /// pro cálculo do remaining. H54.P3.d.
    pub paused_total: Duration,
    /// Quando entrou em hover-pause; None = não pausado agora.
    pub pause_started_at: Option<Instant>,
    /// Posição Y atual (animada).
    pub current_y: f32,
    /// Posição Y alvo (resultado do reflow).
    pub target_y: f32,
    /// Início da última anim de Y (pra interpolar).
    pub anim_started_at: Instant,
    pub anim_start_y: f32,
    /// Estado de saída: Some(closed_at) → fade-out + remoção. H54.P3.c.
    pub closing_since: Option<Instant>,
    /// Reason pro `NotificationClosed` signal quando finalmente
    /// remover: 1=expired, 2=dismissed (user), 3=closed-by-call,
    /// 4=undefined (replaces_id). H54.P4.
    pub closing_reason: u32,
    /// Quando criada: pra slide-in + fade-in nos primeiros ENTRY_ANIM_MS.
    pub entered_at: Instant,
    /// Pares (key, label) das ações visíveis (botões). A key
    /// `"default"` é separada em `default_action` e não vira botão.
    /// H54.P4.
    pub actions: Vec<(String, String)>,
    /// Key da ação "default" (body-click). None se não houver.
    pub default_action: Option<String>,
    /// Hint `resident`: se true, não fecha após `ActionInvoked` —
    /// timer continua normal. H54.P4.
    pub resident: bool,
}

impl NotificationEntry {
    /// Elapsed efetivo pro cálculo de expiração: descontando o tempo
    /// que ficou pausado por hover.
    pub fn effective_elapsed(&self) -> Duration {
        let base = self.received_at.elapsed();
        let in_pause_now = self
            .pause_started_at
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO);
        base.saturating_sub(self.paused_total + in_pause_now)
    }

    /// `true` se passou do timeout (e tem timeout). Critical e
    /// `timeout_ms == None` retorna sempre `false`.
    pub fn is_expired(&self) -> bool {
        match self.timeout_ms {
            Some(t) => self.effective_elapsed().as_millis() as u64 >= t,
            None => false,
        }
    }
}

/// Estado completo do stack de notificações. Compartilhado com a
/// thread D-Bus via `Arc<Mutex<...>>`. H54.P3.
#[derive(Debug, Default)]
pub struct NotificationsState {
    pub visible: Vec<NotificationEntry>,
    pub queue: VecDeque<NotificationEntry>,
    /// `true` quando o cursor está sobre QUALQUER rect de toast.
    /// Atualizado pelo motion handler em pointer.rs.
    pub hover_paused: bool,
    /// Próximo id incremental atribuído quando o app não passa
    /// `replaces_id`.
    pub next_id: u32,
}

pub type NotificationsSlot = Arc<Mutex<NotificationsState>>;

/// Conexão D-Bus compartilhada com a thread server, pra emitir
/// signals (`ActionInvoked`, `NotificationClosed`) a partir do main
/// loop. None até a thread conseguir RequestName (ou pra sempre se
/// LUMA_TAKE_NOTIFICATIONS está unset). H54.P4.
pub type NotificationsConn =
    Arc<Mutex<Option<zbus::blocking::Connection>>>;

/// Emite `ActionInvoked(id, action_key)`. No-op se sem conexão.
pub fn emit_action_invoked(conn: &NotificationsConn, id: u32, action_key: &str) {
    if let Ok(g) = conn.lock() {
        if let Some(c) = g.as_ref() {
            let _ = c.emit_signal(
                None::<&str>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "ActionInvoked",
                &(id, action_key),
            );
        }
    }
}

/// Emite `NotificationClosed(id, reason)`. No-op se sem conexão.
/// Reasons (per spec): 1=expired, 2=dismissed by user, 3=closed by
/// CloseNotification call, 4=undefined/other.
pub fn emit_notification_closed(
    conn: &NotificationsConn,
    id: u32,
    reason: u32,
) {
    if let Ok(g) = conn.lock() {
        if let Some(c) = g.as_ref() {
            let _ = c.emit_signal(
                None::<&str>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "NotificationClosed",
                &(id, reason),
            );
        }
    }
}

/// Spawna a thread D-Bus em background (se LUMA_TAKE_NOTIFICATIONS).
/// Retorna o slot compartilhado. Se LUMA_FAKE_NOTIFICATION é setado,
/// injeta fakes pra teste visual.
pub fn spawn() -> (NotificationsSlot, NotificationsConn) {
    let slot: NotificationsSlot = Arc::new(Mutex::new(NotificationsState {
        visible: Vec::new(),
        queue: VecDeque::new(),
        hover_paused: false,
        next_id: 1,
    }));
    let conn_handle: NotificationsConn = Arc::new(Mutex::new(None));
    if let Ok(val) = std::env::var("LUMA_FAKE_NOTIFICATION") {
        let count: usize = val.parse().unwrap_or(1).max(1);
        for i in 0..count {
            inject_fake(&slot, i);
        }
        tracing::info!(
            "notifications: LUMA_FAKE_NOTIFICATION={count} ativo \
             — {count} toast(s) fake injetado(s)"
        );
    }
    if std::env::var("LUMA_TAKE_NOTIFICATIONS").is_err() {
        tracing::info!(
            "notifications: nao registrado (LUMA_TAKE_NOTIFICATIONS unset)"
        );
        return (slot, conn_handle);
    }
    let slot_for_thread = slot.clone();
    let conn_for_thread = conn_handle.clone();
    std::thread::Builder::new()
        .name("luma-notifications".into())
        .spawn(move || {
            if let Err(e) = run(slot_for_thread, conn_for_thread) {
                tracing::warn!("notifications: thread encerrou com erro: {e}");
            }
        })
        .ok();
    (slot, conn_handle)
}

/// Faz a manutenção do stack: expira, remove fechadas após exit-anim,
/// promove queue→visible. Chamado a cada frame pelo render path.
/// Retorna `Vec<(id, reason)>` das entries removidas — caller emite
/// `NotificationClosed` signal pra cada. H54.P4.
pub fn tick(slot: &NotificationsSlot) -> Vec<(u32, u32)> {
    let mut g = match slot.lock() {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };
    // 1. Marca como closing as que expiraram (reason=1).
    for e in g.visible.iter_mut() {
        if e.closing_since.is_some() {
            continue;
        }
        if e.is_expired() {
            e.closing_since = Some(Instant::now());
            e.closing_reason = 1;
        }
    }
    // 2. Remove as que terminaram a anim de exit. Coleta (id, reason)
    //    pra emitir signal depois.
    let mut closed: Vec<(u32, u32)> = Vec::new();
    g.visible.retain(|e| {
        if let Some(t) = e.closing_since {
            if t.elapsed().as_millis() as u64 > EXIT_ANIM_MS {
                closed.push((e.id, e.closing_reason));
                return false;
            }
        }
        true
    });
    // 3. Promove queue→visible enquanto houver espaço.
    while g.visible.len() < MAX_VISIBLE_TOASTS {
        if let Some(next) = g.queue.pop_front() {
            push_visible(&mut g, next);
        } else {
            break;
        }
    }
    closed
}

/// Insere um entry no slot visible respeitando STACK_NEWEST_ON_TOP.
/// O entry deve ter `entered_at = Instant::now()` setado pelo caller.
fn push_visible(state: &mut NotificationsState, mut entry: NotificationEntry) {
    entry.entered_at = Instant::now();
    entry.anim_started_at = Instant::now();
    if STACK_NEWEST_ON_TOP {
        entry.current_y = -50.0; // start off-top, anim_start_y aplica em reflow
        entry.anim_start_y = entry.current_y;
        state.visible.insert(0, entry);
    } else {
        entry.current_y = 0.0;
        entry.anim_start_y = 0.0;
        state.visible.push(entry);
    }
}

/// Coloca uma notification recém chegada — visible se cabe, queue
/// senão. Retorna o id atribuído.
fn enqueue_or_visible(
    state: &mut NotificationsState,
    mut entry: NotificationEntry,
) -> u32 {
    let id = entry.id;
    if state.visible.len() < MAX_VISIBLE_TOASTS {
        push_visible(state, entry);
    } else {
        entry.entered_at = Instant::now();
        state.queue.push_back(entry);
    }
    id
}

/// Aplica o resultado do `Notify`. Trata replaces_id (in-place
/// update no visible ou queue), hover_paused (se ativo, marca o
/// novo entry como pausado também), e overflow. H54.P3.b.
fn apply_notify(state: &mut NotificationsState, mut entry: NotificationEntry) -> u32 {
    let replaces_id = entry.id;
    // (1) Tenta atualizar in-place no visible.
    if replaces_id != 0 {
        if let Some(existing) = state.visible.iter_mut().find(|e| e.id == replaces_id) {
            existing.app_name = entry.app_name.clone();
            existing.summary = entry.summary.clone();
            existing.body = entry.body.clone();
            existing.icon = entry.icon.clone();
            existing.icon_size = entry.icon_size;
            existing.timeout_ms = entry.timeout_ms;
            existing.received_at = Instant::now();
            existing.paused_total = Duration::ZERO;
            existing.pause_started_at = if state.hover_paused {
                Some(Instant::now())
            } else {
                None
            };
            // Reflow será recomputado no próximo frame.
            return replaces_id;
        }
        // (2) Tenta na queue.
        if let Some(q) = state.queue.iter_mut().find(|e| e.id == replaces_id) {
            q.app_name = entry.app_name.clone();
            q.summary = entry.summary.clone();
            q.body = entry.body.clone();
            q.icon = entry.icon.clone();
            q.icon_size = entry.icon_size;
            q.timeout_ms = entry.timeout_ms;
            return replaces_id;
        }
        // Cai pro caminho de criar novo, mas usando replaces_id.
    }
    // (3) Cria novo. Atribui id incremental se replaces_id == 0.
    if entry.id == 0 {
        entry.id = state.next_id;
        state.next_id = state.next_id.checked_add(1).unwrap_or(1);
    }
    // Se há hover ativo, novo entry já começa pausado.
    if state.hover_paused {
        entry.pause_started_at = Some(Instant::now());
    }
    enqueue_or_visible(state, entry)
}

/// Fecha por id externo (reason=3 = closed-by-call default).
pub fn close_by_id(state: &mut NotificationsState, id: u32) {
    close_by_reason(state, id, 3)
}

/// Fecha por id com reason específica. Reasons (spec):
/// 1=expired, 2=dismissed by user, 3=closed by CloseNotification,
/// 4=undefined (ex: replaces_id evicted). H54.P4.
pub fn close_by_reason(state: &mut NotificationsState, id: u32, reason: u32) {
    if let Some(e) = state.visible.iter_mut().find(|e| e.id == id) {
        if e.closing_since.is_none() {
            e.closing_since = Some(Instant::now());
            e.closing_reason = reason;
        }
        return;
    }
    state.queue.retain(|e| e.id != id);
}

fn run(
    slot: NotificationsSlot,
    conn_handle: NotificationsConn,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = NotificationServer { slot };
    let conn = zbus::blocking::connection::Builder::session()?
        .name("org.freedesktop.Notifications")?
        .serve_at("/org/freedesktop/Notifications", server)?
        .build()?;
    // Compartilha a conexão com o main loop pra emitir signals.
    if let Ok(mut g) = conn_handle.lock() {
        *g = Some(conn.clone());
    }
    tracing::info!(
        "notifications: registrado em /org/freedesktop/Notifications"
    );
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

fn inject_fake(slot: &NotificationsSlot, idx: usize) {
    let icons = [
        "applications-internet",
        "applications-multimedia",
        "applications-system",
        "applications-graphics",
        "applications-games",
        "applications-office",
    ];
    let icon_name = icons[idx % icons.len()];
    let titles = [
        "Mensagem nova",
        "Música tocando",
        "Atualização disponível",
        "Imagem salva",
        "Convite recebido",
        "Lembrete",
    ];
    let bodies = [
        "Olá! Esse é um toast de teste pra validar o stacking.",
        "Spotify · Bossa Nova Mix · faixa 3 de 24",
        "Sistema tem 3 atualizações pendentes. Toque pra revisar.",
        "Captura salva em ~/Imagens/screenshot.png",
        "Reunião amanhã às 14h. Aceite pra confirmar.",
        "Não esquece de revisar o PR antes das 18h.",
    ];
    let (icon_buf, icon_size) =
        decode_icon(&HashMap::new(), icon_name);
    // Alguns fakes ganham actions pra testar buttons + body click.
    let (actions, default_action): (Vec<(String, String)>, Option<String>) =
        match idx % 3 {
            0 => (
                vec![
                    ("reply".to_string(), "Responder".to_string()),
                    ("dismiss".to_string(), "Dispensar".to_string()),
                ],
                Some("default".to_string()),
            ),
            1 => (
                Vec::new(),
                Some("default".to_string()),
            ),
            _ => (Vec::new(), None),
        };
    let entry = NotificationEntry {
        id: 0,
        app_name: titles[idx % titles.len()].to_string(),
        summary: titles[idx % titles.len()].to_string(),
        body: bodies[idx % bodies.len()].to_string(),
        icon: icon_buf,
        icon_size,
        received_at: Instant::now(),
        timeout_ms: Some(5000),
        paused_total: Duration::ZERO,
        pause_started_at: None,
        current_y: 0.0,
        target_y: 0.0,
        anim_started_at: Instant::now(),
        anim_start_y: 0.0,
        closing_since: None,
        closing_reason: 4,
        entered_at: Instant::now(),
        actions,
        default_action,
        resident: false,
    };
    if let Ok(mut g) = slot.lock() {
        apply_notify(&mut g, entry);
    }
}

struct NotificationServer {
    slot: NotificationsSlot,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".to_string(), "actions".to_string()]
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v.clone()).ok())
            .map(Urgency::from_byte)
            .unwrap_or(Urgency::Normal);
        let timeout_ms: Option<u64> = if urgency == Urgency::Critical {
            None
        } else {
            match expire_timeout {
                0 => None,
                t if t > 0 => Some(t as u64),
                _ => Some(crate::theme::current().notifications.timeout_ms),
            }
        };
        let (icon, icon_size) = decode_icon(&hints, &app_icon);
        // H54.P4: parse actions [key, label, key, label, ...]. Separa
        // "default" (body-click) das visíveis. Numero ímpar = ignora
        // sobra (defensivo, sem crash).
        let mut parsed_actions: Vec<(String, String)> = Vec::new();
        let mut default_action: Option<String> = None;
        for chunk in actions.chunks_exact(2) {
            let key = &chunk[0];
            let label = &chunk[1];
            if key == "default" {
                default_action = Some("default".to_string());
            } else {
                parsed_actions.push((key.clone(), label.clone()));
            }
        }
        let resident = hints
            .get("resident")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);
        let entry = NotificationEntry {
            id: replaces_id,
            app_name,
            summary,
            body,
            icon,
            icon_size,
            received_at: Instant::now(),
            timeout_ms,
            paused_total: Duration::ZERO,
            pause_started_at: None,
            current_y: 0.0,
            target_y: 0.0,
            anim_started_at: Instant::now(),
            anim_start_y: 0.0,
            closing_since: None,
            closing_reason: 4,
            entered_at: Instant::now(),
            actions: parsed_actions,
            default_action,
            resident,
        };
        let id = if let Ok(mut g) = self.slot.lock() {
            apply_notify(&mut g, entry)
        } else {
            0
        };
        tracing::info!("notifications: Notify id={id}");
        id
    }

    fn close_notification(&self, id: u32) {
        if let Ok(mut g) = self.slot.lock() {
            close_by_id(&mut g, id);
        }
        tracing::info!("notifications: CloseNotification id={id}");
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "Luma".into(),
            "Lumina".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }
}

/// Decodifica o ícone (mesma cascata da Phase 2). Pública pros fakes.
fn decode_icon(
    hints: &HashMap<String, zbus::zvariant::OwnedValue>,
    app_icon: &str,
) -> (
    Option<MemoryRenderBuffer>,
    smithay::utils::Size<i32, smithay::utils::Logical>,
) {
    if let Some(val) = hints.get("image-data").or_else(|| hints.get("icon_data")) {
        if let Some(buf) = decode_image_data(val.clone()) {
            return buf;
        }
    }
    if let Some(val) = hints.get("image-path").or_else(|| hints.get("image_path")) {
        if let Ok(path) = String::try_from(val.clone()) {
            if let Some(buf) = load_icon_path(&path) {
                return buf;
            }
        }
    }
    if !app_icon.is_empty() {
        let resolved = if app_icon.starts_with('/') {
            Some(std::path::PathBuf::from(app_icon))
        } else {
            crate::apps::resolve_icon_path(app_icon)
        };
        if let Some(path) = resolved {
            if let Some(buf) = load_icon_path(&path.to_string_lossy()) {
                return buf;
            }
        }
    }
    (None, smithay::utils::Size::from((0, 0)))
}

fn decode_image_data(
    val: zbus::zvariant::OwnedValue,
) -> Option<(
    Option<MemoryRenderBuffer>,
    smithay::utils::Size<i32, smithay::utils::Logical>,
)> {
    let tuple: (i32, i32, i32, bool, i32, i32, Vec<u8>) =
        TryFrom::try_from(val).ok()?;
    let (w, h, rowstride, has_alpha, bps, channels, data) = tuple;
    if w <= 0 || h <= 0 || bps != 8 {
        return None;
    }
    let bytes_per_px = if has_alpha { 4 } else { 3 };
    if channels != bytes_per_px {
        return None;
    }
    let mut bgra: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let off = (row as usize) * (rowstride as usize);
        for col in 0..w {
            let p = off + (col as usize) * (bytes_per_px as usize);
            if p + bytes_per_px as usize > data.len() {
                return None;
            }
            let r = data[p];
            let g = data[p + 1];
            let b = data[p + 2];
            let a = if has_alpha { data[p + 3] } else { 255 };
            bgra.push(((b as u16 * a as u16) / 255) as u8);
            bgra.push(((g as u16 * a as u16) / 255) as u8);
            bgra.push(((r as u16 * a as u16) / 255) as u8);
            bgra.push(a);
        }
    }
    let buf = MemoryRenderBuffer::from_slice(
        &bgra,
        Fourcc::Argb8888,
        (w, h),
        1,
        Transform::Normal,
        None,
    );
    Some((Some(buf), smithay::utils::Size::from((w, h))))
}

fn load_icon_path(
    path: &str,
) -> Option<(
    Option<MemoryRenderBuffer>,
    smithay::utils::Size<i32, smithay::utils::Logical>,
)> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return None;
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    if ext.as_deref() == Some("svg") {
        let size = crate::theme::current().notifications.icon_size as u32;
        let buf = crate::menu::icon_loader::load_svg_file(path, size)?;
        return Some((
            Some(buf),
            smithay::utils::Size::from((size as i32, size as i32)),
        ));
    }
    let img = image::open(p).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    let mut data = rgba.into_raw();
    for chunk in data.chunks_exact_mut(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];
        chunk[0] = ((b as u16 * a as u16) / 255) as u8;
        chunk[1] = ((g as u16 * a as u16) / 255) as u8;
        chunk[2] = ((r as u16 * a as u16) / 255) as u8;
        chunk[3] = a;
    }
    let buf = MemoryRenderBuffer::from_slice(
        &data,
        Fourcc::Argb8888,
        (w, h),
        1,
        Transform::Normal,
        None,
    );
    Some((Some(buf), smithay::utils::Size::from((w, h))))
}

/// Ease-out cubic: `1 - (1-t)^3`. Usado pra slide-in, reflow,
/// fade-in. H54.P3.c.
pub fn ease_out_cubic(t: f32) -> f32 {
    let inv = (1.0 - t.clamp(0.0, 1.0)).max(0.0);
    1.0 - inv * inv * inv
}
