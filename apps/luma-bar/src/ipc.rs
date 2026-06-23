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

    /// Janelas abertas pra a taskbar (passo 2): `(id, título, app_id, focada)`.
    async fn list_windows(&self) -> zbus::Result<Vec<(u64, String, String, bool)>>;
    /// Foca/levanta a janela pela id.
    async fn activate_window(&self, id: u64) -> zbus::Result<()>;

    /// Estado da bateria (passo 3): `(present, capacity 0–100, charging)`.
    async fn battery(&self) -> zbus::Result<(bool, u8, bool)>;
    #[zbus(signal)]
    fn battery_changed(&self, present: bool, capacity: u8, charging: bool) -> zbus::Result<()>;
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
pub async fn read_windows() -> Vec<(u64, String, String, bool)> {
    async fn inner() -> zbus::Result<Vec<(u64, String, String, bool)>> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.list_windows().await
    }
    inner().await.unwrap_or_default()
}

/// Ativa (foca/levanta) a janela pela id. Erro só loga.
pub async fn activate_window(id: u64) {
    async fn inner(id: u64) -> zbus::Result<()> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.activate_window(id).await
    }
    if let Err(e) = inner(id).await {
        eprintln!("luma-bar: activate_window({id}) falhou: {e}");
    }
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
