//! Servidor D-Bus do compositor (3b). Expõe `dev.luma.Compositor` no
//! session bus com um método `Ping` que retorna a versão. Roda numa
//! thread dedicada com runtime tokio current_thread — isola o async do
//! zbus do loop síncrono do calloop.
//!
//! Comandos que precisarem mexer no estado do compositor (a partir da
//! 3c) viajam pelo channel `IpcCommand` registrado no calloop. O `Ping`
//! desse sub-passo NÃO usa o sender (responde com versão estática),
//! mas a ponte fica montada e testável aqui.

use smithay::reexports::calloop::channel::Sender;
use std::thread;
use tracing::{error, info};
use zbus::interface;

/// Bus name e interface name são iguais (padrão fdo, ex: NetworkManager).
const BUS_NAME: &str = "dev.luma.Compositor";
const OBJECT_PATH: &str = "/dev/luma/Compositor";

/// Comandos enviados da thread D-Bus pro loop principal do compositor.
/// Cresce na 3c (AddWidget, RemoveWidget, ListWidgets, ...). Hoje só
/// existe pra montar e testar o canal calloop.
#[allow(dead_code)]
pub enum IpcCommand {
    /// Placeholder até a 3c. Mantido pra a variante existir.
    Noop,
}

struct CompositorService {
    /// Sender pro loop principal. Pro `Ping` não é usado, mas em 3c+
    /// cada handler de método empacota um `IpcCommand` + oneshot e
    /// aguarda o loop principal responder.
    _tx: Sender<IpcCommand>,
}

#[interface(name = "dev.luma.Compositor")]
impl CompositorService {
    /// Smoke test do canal (3b). Retorna `"luma-compositor <versão>"`.
    /// Nome em D-Bus: `Ping` (zbus converte snake_case → PascalCase).
    async fn ping(&self) -> String {
        format!("luma-compositor {}", env!("CARGO_PKG_VERSION"))
    }
}

/// Sobe o servidor numa thread dedicada. Não-bloqueante.
pub fn spawn(tx: Sender<IpcCommand>) {
    let builder = thread::Builder::new().name("luma-dbus".into());
    if let Err(err) = builder.spawn(move || run(tx)) {
        error!("D-Bus: não foi possível criar thread do servidor: {err}");
    }
}

fn run(tx: Sender<IpcCommand>) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            error!("D-Bus: runtime tokio falhou: {err}");
            return;
        }
    };
    rt.block_on(async move {
        if let Err(err) = serve(tx).await {
            error!("D-Bus: servidor encerrou com erro: {err}");
        }
    });
}

async fn serve(tx: Sender<IpcCommand>) -> zbus::Result<()> {
    let service = CompositorService { _tx: tx };
    let _conn = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, service)?
        .build()
        .await?;
    info!("D-Bus: {BUS_NAME} registrado no session bus em {OBJECT_PATH}");
    // Mantém a conexão viva pela vida do compositor. Dropar `_conn`
    // libera o nome e desconecta clientes — não queremos isso.
    std::future::pending::<()>().await;
    Ok(())
}
