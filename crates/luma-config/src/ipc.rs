//! Cliente D-Bus do app (3b). Conecta no session bus e fala com
//! `dev.luma.Compositor`. Sub-passo ii expõe só `Ping`; em 3c+ o trait
//! cresce com listar/adicionar widgets etc.

use zbus::proxy;

#[proxy(
    interface = "dev.luma.Compositor",
    default_service = "dev.luma.Compositor",
    default_path = "/dev/luma/Compositor"
)]
pub trait Compositor {
    /// Smoke test do canal. Retorna a string de versão do compositor
    /// (ex.: `"luma-compositor 0.3.0"`).
    async fn ping(&self) -> zbus::Result<String>;
}

/// Conecta no session bus e chama `Ping` uma vez. Em sucesso, devolve
/// a string de versão; em falha (compositor não tá no ar, nome não
/// registrado, etc.), propaga o `zbus::Error`.
pub async fn connect_and_ping() -> zbus::Result<String> {
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    proxy.ping().await
}
