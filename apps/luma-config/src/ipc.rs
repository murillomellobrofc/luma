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
