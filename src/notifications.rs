//! Servidor de notificações `org.freedesktop.Notifications` via D-Bus.
//! H54.notifications.
//!
//! Comportamento env-gated: só registra quando `LUMA_TAKE_NOTIFICATIONS=1`
//! está setado. Sem isso, o thread retorna no-op silencioso pra não
//! roubar as notificações do KDE/GNOME em modo aninhado. Mesmo padrão
//! seguro do `LUMA_FAKE_SNI`.
//!
//! Quando ativo, mantém o `Arc<Mutex<Option<Notification>>>` que o
//! main loop lê pra renderizar o toast. Cada Notify substitui a
//! notificação anterior (empilhar é fase futura — debt em DEBT.md).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

/// Notificação ativa (visível como toast). H54.notif.2.
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    /// Ícone já rasterizado em BGRA premultiplied (Argb8888 little
    /// endian). `None` se não veio ícone ou decode falhou.
    pub icon: Option<MemoryRenderBuffer>,
    pub icon_size: smithay::utils::Size<i32, smithay::utils::Logical>,
    /// Instante em que a notificação foi recebida — usado pra fade
    /// e auto-dismiss.
    pub received_at: Instant,
    /// Timeout pedido pelo app: `-1` = use default do token; `0` =
    /// nunca expira; `>0` = ms. H54.notif.2.
    pub expire_timeout_ms: i32,
}

/// Slot compartilhado entre thread D-Bus e main loop. None = sem
/// toast visível.
pub type NotificationSlot = Arc<Mutex<Option<Notification>>>;

/// Spawna a thread D-Bus em background (se LUMA_TAKE_NOTIFICATIONS).
/// Retorna o slot compartilhado (sempre — pode ficar vazio se a
/// thread não registrou).
pub fn spawn() -> NotificationSlot {
    let slot: NotificationSlot = Arc::new(Mutex::new(None));
    if std::env::var("LUMA_TAKE_NOTIFICATIONS").is_err() {
        tracing::info!(
            "notifications: nao registrado (LUMA_TAKE_NOTIFICATIONS unset)"
        );
        return slot;
    }
    let slot_for_thread = slot.clone();
    std::thread::Builder::new()
        .name("luma-notifications".into())
        .spawn(move || {
            if let Err(e) = run(slot_for_thread) {
                tracing::warn!("notifications: thread encerrou com erro: {e}");
            }
        })
        .ok();
    slot
}

fn run(slot: NotificationSlot) -> Result<(), Box<dyn std::error::Error>> {
    let server = NotificationServer {
        slot,
        next_id: Arc::new(Mutex::new(1)),
    };
    let _conn = zbus::blocking::connection::Builder::session()?
        .name("org.freedesktop.Notifications")?
        .serve_at("/org/freedesktop/Notifications", server)?
        .build()?;
    tracing::info!(
        "notifications: registrado em /org/freedesktop/Notifications"
    );
    // Mantém a conexão viva enquanto luma roda. zbus dispatcha
    // chamadas em background — a thread só precisa não dropar conn.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// Interface D-Bus exposta pelo servidor. zbus gera o dispatch.
struct NotificationServer {
    slot: NotificationSlot,
    next_id: Arc<Mutex<u32>>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    /// Spec freedesktop §1.1. Retorna lista de capabilities. H54
    /// suporta só `body` (texto multilinha); sem actions/markup/sound.
    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".to_string()]
    }

    /// Spec freedesktop §1.2. Cria/substitui notificação.
    /// `replaces_id` = 0 → cria nova com id incremental.
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            let mut g = self.next_id.lock().unwrap();
            let id = *g;
            *g = g.checked_add(1).unwrap_or(1);
            id
        };
        let (icon, icon_size) = decode_icon(&hints, &app_icon);
        let n = Notification {
            id,
            app_name,
            summary,
            body,
            icon,
            icon_size,
            received_at: Instant::now(),
            expire_timeout_ms: expire_timeout,
        };
        if let Ok(mut g) = self.slot.lock() {
            // Substitui a notificação atual. Empilhar é debt futuro.
            *g = Some(n);
        }
        tracing::info!("notifications: Notify id={id}");
        id
    }

    /// Spec freedesktop §1.3. Fecha por id externo.
    fn close_notification(&self, id: u32) {
        if let Ok(mut g) = self.slot.lock() {
            if let Some(n) = g.as_ref() {
                if n.id == id {
                    *g = None;
                    tracing::info!("notifications: CloseNotification id={id}");
                    return;
                }
            }
        }
        // Id desconhecido — spec permite ignorar silencioso.
    }

    /// Spec freedesktop §1.4. Identificação do servidor.
    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "Luma".into(),
            "Lumina".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }
}

/// Decodifica o ícone da notificação seguindo a ordem da spec:
/// 1. `hints["image-data"]` — struct (iiibiiay) com pixels raw RGBA
///    (GdkPixbuf format).
/// 2. `hints["image-path"]` — caminho absoluto pra PNG/JPG/SVG.
/// 3. `app_icon` — nome XDG OU caminho absoluto.
/// Falha em qualquer ponto = `(None, (0,0))`.
fn decode_icon(
    hints: &HashMap<String, zbus::zvariant::OwnedValue>,
    app_icon: &str,
) -> (
    Option<MemoryRenderBuffer>,
    smithay::utils::Size<i32, smithay::utils::Logical>,
) {
    // (1) image-data — struct(i,i,i,b,i,i,ay).
    if let Some(val) = hints.get("image-data").or_else(|| hints.get("icon_data")) {
        if let Some(buf) = decode_image_data(val.clone()) {
            return buf;
        }
    }
    // (2) image-path — string com caminho absoluto.
    if let Some(val) = hints.get("image-path").or_else(|| hints.get("image_path")) {
        if let Ok(path) = String::try_from(val.clone()) {
            if let Some(buf) = load_icon_path(&path) {
                return buf;
            }
        }
    }
    // (3) app_icon — XDG name ou path absoluto.
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
    // signature: (iiibiiay) - width, height, rowstride, has_alpha,
    // bits_per_sample, channels, data
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
    // Reorganiza RGB(A) → BGRA premultiplied.
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
            // Premultiply.
            let pr = ((b as u16 * a as u16) / 255) as u8;
            let pg = ((g as u16 * a as u16) / 255) as u8;
            let pb_ = ((r as u16 * a as u16) / 255) as u8;
            bgra.push(pr);
            bgra.push(pg);
            bgra.push(pb_);
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
    let ext = p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
    if ext.as_deref() == Some("svg") {
        // Rasteriza no tamanho do ícone do toast (definido pelo token).
        let size = crate::theme::current().notifications.icon_size as u32;
        let buf = crate::menu::icon_loader::load_svg_file(path, size)?;
        return Some((Some(buf), smithay::utils::Size::from((size as i32, size as i32))));
    }
    // PNG/JPG via image crate.
    let img = image::open(p).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    let mut data = rgba.into_raw();
    // RGBA → BGRA premultiplied.
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
