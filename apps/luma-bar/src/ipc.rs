//! Cliente D-Bus mínimo da `luma-bar`: só o que a barra precisa pra seguir o
//! acento e o alto contraste ao vivo — get no boot + stream do sinal. Mesmo
//! `dev.luma.Compositor` que o `luma-config` fala; aqui é um subconjunto
//! (sem writes — a barra só consome). O compositor é a fonte.

use zbus::proxy;

#[proxy(
    interface = "dev.luma.Compositor",
    default_service = "dev.luma.Compositor",
    default_path = "/dev/luma/Compositor"
)]
pub trait Compositor {
    /// Id do acento curado atual.
    async fn accent(&self) -> zbus::Result<String>;
    #[zbus(signal)]
    fn accent_changed(&self, accent: &str) -> zbus::Result<()>;

    /// Alto contraste atual.
    async fn high_contrast(&self) -> zbus::Result<bool>;
    #[zbus(signal)]
    fn high_contrast_changed(&self, enabled: bool) -> zbus::Result<()>;

    /// Janelas abertas (passo 2 + 7): `(id, título, app_id, focada, minimizada)`.
    async fn list_windows(&self) -> zbus::Result<Vec<(u64, String, String, bool, bool)>>;
    /// Foca/levanta (e restaura se minimizada) a janela pela id.
    async fn activate_window(&self, id: u64) -> zbus::Result<()>;
    /// Minimiza a janela (passo 7).
    async fn minimize_window(&self, id: u64) -> zbus::Result<()>;
    /// Fecha a janela (passo 7).
    async fn close_window(&self, id: u64) -> zbus::Result<()>;
    /// Mostrar área de trabalho — toggle com memória (passo 7).
    async fn show_desktop(&self) -> zbus::Result<()>;

    /// Estado da bateria (passo 3): `(present, capacity 0–100, charging)`.
    async fn battery(&self) -> zbus::Result<(bool, u8, bool)>;
    #[zbus(signal)]
    fn battery_changed(&self, present: bool, capacity: u8, charging: bool) -> zbus::Result<()>;

    /// Volume (passo 4): `(level 0–100, muted)`. Mão dupla: set + toggle.
    async fn volume(&self) -> zbus::Result<(u8, bool)>;
    async fn set_volume(&self, level: u8) -> zbus::Result<()>;
    async fn toggle_mute(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn volume_changed(&self, level: u8, muted: bool) -> zbus::Result<()>;

    /// Rede (passo 5): nome do ícone do estado ("ethernet-on", "wifi-full"…).
    async fn network(&self) -> zbus::Result<String>;
    #[zbus(signal)]
    fn network_changed(&self, icon: &str) -> zbus::Result<()>;
}

/// Lê o acento no boot (pra a barra abrir já na cor salva).
pub async fn read_accent() -> Result<String, String> {
    let conn = zbus::Connection::session().await.map_err(|e| e.to_string())?;
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy.accent().await.map_err(|e| e.to_string())
}

/// Lê o alto contraste no boot.
pub async fn read_high_contrast() -> Result<bool, String> {
    let conn = zbus::Connection::session().await.map_err(|e| e.to_string())?;
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy.high_contrast().await.map_err(|e| e.to_string())
}

/// Stream do sinal `accent_changed` — emite o id novo a cada mudança.
pub async fn accent_signal_stream() -> zbus::Result<impl iced::futures::Stream<Item = String>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_accent_changed().await?;
    Ok(signals.filter_map(|sig| async move { sig.args().ok().map(|a| a.accent().to_string()) }))
}

/// Stream do sinal `high_contrast_changed`.
pub async fn high_contrast_signal_stream(
) -> zbus::Result<impl iced::futures::Stream<Item = bool>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_high_contrast_changed().await?;
    Ok(signals.filter_map(|sig| async move { sig.args().ok().map(|a| *a.enabled()) }))
}

// ─── Janelas / taskbar (passo 2) ──────────────────────────────────────

/// Lê a lista de janelas (poll a cada tick). Erro vira lista vazia — a barra
/// só esvazia, sem quebrar.
pub async fn read_windows() -> Vec<(u64, String, String, bool, bool)> {
    async fn inner() -> zbus::Result<Vec<(u64, String, String, bool, bool)>> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.list_windows().await
    }
    inner().await.unwrap_or_default()
}

/// Helper genérico pros comandos de janela fire-and-forget (passo 2 + 7).
async fn window_cmd(label: &str, fut: impl std::future::Future<Output = zbus::Result<()>>) {
    if let Err(e) = fut.await {
        eprintln!("luma-bar: {label} falhou: {e}");
    }
}

/// Ativa (foca/levanta/restaura) a janela pela id.
pub async fn activate_window(id: u64) {
    let conn = zbus::Connection::session().await;
    window_cmd("activate_window", async move {
        CompositorProxy::new(&conn?).await?.activate_window(id).await
    })
    .await;
}

/// Minimiza a janela pela id (passo 7).
pub async fn minimize_window(id: u64) {
    let conn = zbus::Connection::session().await;
    window_cmd("minimize_window", async move {
        CompositorProxy::new(&conn?).await?.minimize_window(id).await
    })
    .await;
}

/// Fecha a janela pela id (passo 7).
pub async fn close_window(id: u64) {
    let conn = zbus::Connection::session().await;
    window_cmd("close_window", async move {
        CompositorProxy::new(&conn?).await?.close_window(id).await
    })
    .await;
}

/// Mostrar área de trabalho — toggle (passo 7).
pub async fn show_desktop() {
    let conn = zbus::Connection::session().await;
    window_cmd("show_desktop", async move {
        CompositorProxy::new(&conn?).await?.show_desktop().await
    })
    .await;
}

// ─── Bateria (passo 3) ────────────────────────────────────────────────

/// Lê o estado da bateria no boot: `(present, capacity, charging)`. Erro =
/// sem bateria.
pub async fn read_battery() -> (bool, u8, bool) {
    async fn inner() -> zbus::Result<(bool, u8, bool)> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.battery().await
    }
    inner().await.unwrap_or((false, 0, false))
}

/// Stream do sinal `battery_changed` — `(present, capacity, charging)`.
pub async fn battery_signal_stream(
) -> zbus::Result<impl iced::futures::Stream<Item = (bool, u8, bool)>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_battery_changed().await?;
    Ok(signals.filter_map(|sig| async move {
        sig.args().ok().map(|a| (*a.present(), *a.capacity(), *a.charging()))
    }))
}

// ─── Volume (passo 4) ─────────────────────────────────────────────────

/// Lê o volume no boot: `(level 0–100, muted)`.
pub async fn read_volume() -> (u8, bool) {
    async fn inner() -> zbus::Result<(u8, bool)> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.volume().await
    }
    inner().await.unwrap_or((0, false))
}

/// Seta o volume (scroll). Erro só loga.
pub async fn set_volume(level: u8) {
    async fn inner(level: u8) -> zbus::Result<()> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.set_volume(level).await
    }
    if let Err(e) = inner(level).await {
        eprintln!("luma-bar: set_volume({level}) falhou: {e}");
    }
}

/// Alterna mudo (clique). Erro só loga.
pub async fn toggle_mute() {
    async fn inner() -> zbus::Result<()> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.toggle_mute().await
    }
    if let Err(e) = inner().await {
        eprintln!("luma-bar: toggle_mute falhou: {e}");
    }
}

/// Stream do sinal `volume_changed` — `(level, muted)`.
pub async fn volume_signal_stream() -> zbus::Result<impl iced::futures::Stream<Item = (u8, bool)>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_volume_changed().await?;
    Ok(signals.filter_map(|sig| async move { sig.args().ok().map(|a| (*a.level(), *a.muted())) }))
}

// ─── Rede (passo 5) ───────────────────────────────────────────────────

/// Lê o nome do ícone de rede no boot.
pub async fn read_network() -> String {
    async fn inner() -> zbus::Result<String> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.network().await
    }
    inner().await.unwrap_or_default()
}

/// Stream do sinal `network_changed` — o nome do ícone novo.
pub async fn network_signal_stream() -> zbus::Result<impl iced::futures::Stream<Item = String>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_network_changed().await?;
    Ok(signals.filter_map(|sig| async move { sig.args().ok().map(|a| a.icon().to_string()) }))
}
