//! Cliente D-Bus do app. Conecta no session bus e fala com
//! `dev.luma.Compositor`. 3b expôs `Ping`; 3c.2 adiciona
//! `ListAvailableWidgets` e `AddWidget`.

use luma_ipc::WidgetInfo;
use zbus::proxy;

#[proxy(
    interface = "dev.luma.Compositor",
    default_service = "dev.luma.Compositor",
    default_path = "/dev/luma/Compositor"
)]
pub trait Compositor {
    /// Smoke test do canal. Retorna a versão do compositor.
    async fn ping(&self) -> zbus::Result<String>;

    /// Catálogo dos tipos de widget conhecidos pelo compositor (3c.1).
    /// Retorna `Vec<WidgetInfo>` (a(ssss) no wire).
    async fn list_available_widgets(&self) -> zbus::Result<Vec<WidgetInfo>>;

    /// Adiciona uma instância do `type_name` em `target`
    /// (`"panel"` ou `"desktop"`). Erro de domínio
    /// (target inválido, tipo desconhecido) vira `InvalidArgs`.
    async fn add_widget(&self, type_name: &str, target: &str) -> zbus::Result<()>;

    /// Estado atual do sino visual (3a). `true` = piscada dispara em alertas.
    async fn visual_bell_enabled(&self) -> zbus::Result<bool>;

    /// Liga/desliga o sino visual no compositor ao vivo (3a).
    async fn set_visual_bell_enabled(&self, enabled: bool) -> zbus::Result<()>;

    /// Modo seguro pra fotossensibilidade (3c).
    async fn photosafe_enabled(&self) -> zbus::Result<bool>;
    async fn set_photosafe_enabled(&self, enabled: bool) -> zbus::Result<()>;

    /// Intensidade do flash em % 0–100 (3c) — primeiro setting numérico.
    async fn flash_intensity(&self) -> zbus::Result<u8>;
    async fn set_flash_intensity(&self, intensity: u8) -> zbus::Result<()>;

    /// Reduzir movimento (3d).
    async fn reduce_motion(&self) -> zbus::Result<bool>;
    async fn set_reduce_motion(&self, enabled: bool) -> zbus::Result<()>;

    /// Alto contraste (HC parte 2): get/set + o **sinal de mudança** que o
    /// app escuta pra re-resolver a aparência ao vivo.
    async fn high_contrast(&self) -> zbus::Result<bool>;
    async fn set_high_contrast(&self, enabled: bool) -> zbus::Result<()>;

    /// Sinal emitido pelo compositor quando o alto contraste muda. Gera
    /// `receive_high_contrast_changed()` (stream) + o tipo de args.
    #[zbus(signal)]
    fn high_contrast_changed(&self, enabled: bool) -> zbus::Result<()>;
}

/// Conecta no session bus e chama `Ping`. Em sucesso, devolve a
/// string de versão; em falha, propaga o `zbus::Error`.
pub async fn connect_and_ping() -> zbus::Result<String> {
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    proxy.ping().await
}

/// Conecta no session bus e busca o catálogo. Usado quando a transição
/// pra `Connected` acontece (logo após o ping retornar Ok).
pub async fn fetch_available_widgets() -> zbus::Result<Vec<WidgetInfo>> {
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    proxy.list_available_widgets().await
}

/// Conecta no session bus e dispara `AddWidget(type_name, target)`.
/// `target` deve ser `"panel"` ou `"desktop"`.
pub async fn add_widget(type_name: String, target: &'static str) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    proxy.add_widget(&type_name, target).await
}

// ─── Sino visual (3a) ─────────────────────────────────────────────────
//
// O app guarda UMA `zbus::Connection` no estado (estabelecida no boot) e a
// clona pra cada chamada — a `Connection` é `Arc` por dentro, clone é
// barato. Os erros viram `String` (Clone + Debug) pra viajarem na `Message`
// do iced. Sem panic: a UI segue mesmo se o compositor não responder.

/// Estabelece a conexão com o session bus (chamada no boot do app).
pub async fn connect() -> Result<zbus::Connection, String> {
    zbus::Connection::session().await.map_err(|e| e.to_string())
}

/// Lê o estado atual do sino visual do compositor.
pub async fn read_visual_bell(conn: zbus::Connection) -> Result<bool, String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy.visual_bell_enabled().await.map_err(|e| e.to_string())
}

/// Escreve o estado do sino visual no compositor.
pub async fn write_visual_bell(conn: zbus::Connection, enabled: bool) -> Result<(), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .set_visual_bell_enabled(enabled)
        .await
        .map_err(|e| e.to_string())
}

/// Lê os TRÊS settings do sino visual de uma vez (3c), pra o `read-on-open`
/// inicializar a página inteira: `(visual_bell, photosafe, intensidade%)`.
pub async fn read_bell_settings(conn: zbus::Connection) -> Result<(bool, bool, u8), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    let enabled = proxy.visual_bell_enabled().await.map_err(|e| e.to_string())?;
    let photosafe = proxy.photosafe_enabled().await.map_err(|e| e.to_string())?;
    let intensity = proxy.flash_intensity().await.map_err(|e| e.to_string())?;
    Ok((enabled, photosafe, intensity))
}

/// Escreve o modo fotossensibilidade (3c).
pub async fn write_photosafe(conn: zbus::Connection, enabled: bool) -> Result<(), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .set_photosafe_enabled(enabled)
        .await
        .map_err(|e| e.to_string())
}

/// Escreve a intensidade do flash em % (3c). Commitado no soltar do slider.
pub async fn write_flash_intensity(conn: zbus::Connection, intensity: u8) -> Result<(), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .set_flash_intensity(intensity)
        .await
        .map_err(|e| e.to_string())
}

/// Lê o reduzir movimento do compositor (3d). Pro read-on-open do Movimento.
pub async fn read_reduce_motion(conn: zbus::Connection) -> Result<bool, String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy.reduce_motion().await.map_err(|e| e.to_string())
}

/// Escreve o reduzir movimento (3d). Toggle, commitado na hora.
pub async fn write_reduce_motion(conn: zbus::Connection, enabled: bool) -> Result<(), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .set_reduce_motion(enabled)
        .await
        .map_err(|e| e.to_string())
}

// ─── Alto contraste (HC parte 2) ──────────────────────────────────────

/// Lê o alto contraste do compositor (boot + read-on-open).
pub async fn read_high_contrast(conn: zbus::Connection) -> Result<bool, String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy.high_contrast().await.map_err(|e| e.to_string())
}

/// Escreve o alto contraste (toggle). O re-render NÃO vem daqui — vem do
/// sinal de mudança (fonte única). Só dispara o set.
pub async fn write_high_contrast(conn: zbus::Connection, enabled: bool) -> Result<(), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .set_high_contrast(enabled)
        .await
        .map_err(|e| e.to_string())
}

/// **Stream do sinal** `high_contrast_changed` (HC parte 2) — o canal de
/// propagação de mudança de aparência. Conecta uma vez, assina o sinal, e
/// emite o novo `bool` a cada mudança. A `Subscription` do iced roda este
/// stream e re-resolve o tema. Conexão própria (Arc por dentro): o stream
/// é self-contained.
pub async fn high_contrast_signal_stream(
) -> zbus::Result<impl iced::futures::Stream<Item = bool>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_high_contrast_changed().await?;
    Ok(signals.filter_map(|sig| async move {
        // O accessor do arg é um método que devolve &bool; deref pra copiar.
        sig.args().ok().map(|a| *a.enabled())
    }))
}
