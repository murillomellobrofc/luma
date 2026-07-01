//! Cliente D-Bus mínimo do luma-menu: o gatilho `menu_toggle` + acento/HC ao
//! vivo (pra o menu nascer vestindo o tema). Mesmo `dev.luma.Compositor`.

use zbus::proxy;

#[proxy(
    interface = "dev.luma.Compositor",
    default_service = "dev.luma.Compositor",
    default_path = "/dev/luma/Compositor"
)]
pub trait Compositor {
    /// Sinal de toggle do menu (emitido pelo `ShowMenu`).
    #[zbus(signal)]
    fn menu_toggle(&self) -> zbus::Result<()>;

    /// Sinal do context menu do desktop: `(x, y, out_w, out_h)` do clique.
    #[zbus(signal)]
    fn desktop_menu(&self, x: i32, y: i32, out_w: i32, out_h: i32) -> zbus::Result<()>;

    async fn accent(&self) -> zbus::Result<String>;
    #[zbus(signal)]
    fn accent_changed(&self, accent: &str) -> zbus::Result<()>;

    async fn high_contrast(&self) -> zbus::Result<bool>;
    #[zbus(signal)]
    fn high_contrast_changed(&self, enabled: bool) -> zbus::Result<()>;

    /// Avisa o compositor que o context menu do desktop fechou.
    async fn desktop_menu_closed(&self) -> zbus::Result<()>;

    /// Sessão (passo 3): desligar/reiniciar/suspender/bloquear.
    async fn power_off(&self) -> zbus::Result<()>;
    async fn reboot(&self) -> zbus::Result<()>;
    async fn suspend(&self) -> zbus::Result<()>;
    async fn lock(&self) -> zbus::Result<()>;
}

async fn session_proxy() -> zbus::Result<CompositorProxy<'static>> {
    let conn = zbus::Connection::session().await?;
    CompositorProxy::new(&conn).await
}

pub async fn power_off() {
    if let Ok(p) = session_proxy().await {
        let _ = p.power_off().await;
    }
}

pub async fn reboot() {
    if let Ok(p) = session_proxy().await {
        let _ = p.reboot().await;
    }
}

pub async fn suspend() {
    if let Ok(p) = session_proxy().await {
        let _ = p.suspend().await;
    }
}

pub async fn lock() {
    if let Ok(p) = session_proxy().await {
        let _ = p.lock().await;
    }
}

/// Avisa o compositor que o context menu do desktop fechou.
pub async fn desktop_menu_closed() {
    if let Ok(p) = session_proxy().await {
        let _ = p.desktop_menu_closed().await;
    }
}

/// Stream do sinal `desktop_menu` — `(x, y, out_w, out_h)` do clique-direito.
pub async fn desktop_menu_stream(
) -> zbus::Result<impl iced::futures::Stream<Item = (i32, i32, i32, i32)>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_desktop_menu().await?;
    Ok(signals.filter_map(|sig| async move {
        sig.args()
            .ok()
            .map(|a| (*a.x(), *a.y(), *a.out_w(), *a.out_h()))
    }))
}

/// Stream do sinal `menu_toggle` — cada item é um "toggle" (sem dados).
pub async fn menu_toggle_stream() -> zbus::Result<impl iced::futures::Stream<Item = ()>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_menu_toggle().await?;
    Ok(signals.map(|_| ()))
}

pub async fn read_accent() -> Result<String, String> {
    let conn = zbus::Connection::session().await.map_err(|e| e.to_string())?;
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy.accent().await.map_err(|e| e.to_string())
}

pub async fn read_high_contrast() -> Result<bool, String> {
    let conn = zbus::Connection::session().await.map_err(|e| e.to_string())?;
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy.high_contrast().await.map_err(|e| e.to_string())
}

pub async fn accent_signal_stream() -> zbus::Result<impl iced::futures::Stream<Item = String>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_accent_changed().await?;
    Ok(signals.filter_map(|sig| async move { sig.args().ok().map(|a| a.accent().to_string()) }))
}

pub async fn high_contrast_signal_stream(
) -> zbus::Result<impl iced::futures::Stream<Item = bool>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_high_contrast_changed().await?;
    Ok(signals.filter_map(|sig| async move { sig.args().ok().map(|a| *a.enabled()) }))
}
