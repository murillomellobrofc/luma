//! Cliente SNI (StatusNotifierItem) via D-Bus. H47b.tray.
//!
//! Estratégia: uma thread dedicada possui a `zbus::blocking::Connection`,
//! escuta signals do `org.kde.StatusNotifierWatcher` e mantém uma lista
//! viva de itens em `Arc<Mutex<Vec<SniItem>>>`. O main loop lê esse vetor
//! em cada frame pro render — zero async no caminho de render.
//!
//! Fluxo de bootstrap:
//! 1. `zbus::blocking::Connection::session()`
//! 2. `RequestName("org.kde.StatusNotifierHost-<pid>")`
//! 3. `StatusNotifierWatcher.RegisterStatusNotifierHost(unique_name)`
//! 4. Query `RegisteredStatusNotifierItems` property (lista inicial)
//! 5. Subscribe signals `StatusNotifierItemRegistered/Unregistered`
//! 6. Pra cada item: criar proxy, ler IconName/IconPixmap/ToolTip
//!
//! Cada item registrado vem como string `service[/object_path]`. Quando
//! o object_path é omitido, default é `/StatusNotifierItem`.

use std::sync::{Arc, Mutex};
use std::thread;

/// Pixmap raw ARGB32 BE recebido via `IconPixmap` property.
#[derive(Debug, Clone)]
pub struct SniPixmap {
    pub width: i32,
    pub height: i32,
    /// ARGB32 big-endian (4 bytes por pixel, A primeiro).
    pub data: Vec<u8>,
}

/// Estado de um item SNI registrado. Atualizado pela thread de
/// signals quando o item é registrado ou re-emite NewIcon/NewTooltip.
#[derive(Debug, Clone)]
pub struct SniItem {
    /// Nome completo do bus do item (`:1.123/StatusNotifierItem`).
    /// Usado pra criar proxy de Activate/ContextMenu.
    pub service: String,
    pub object_path: String,
    /// Title legível (fallback pra app name).
    pub title: String,
    /// Tooltip (heading do struct ToolTip, parte mais relevante).
    pub tooltip: String,
    /// Nome XDG do ícone — se preenchido, prioritário sobre pixmap.
    pub icon_name: String,
    /// Pixmaps raw quando o app não fornece icon_name. Vec por tamanho;
    /// o render escolhe o maior que cabe.
    pub icon_pixmaps: Vec<SniPixmap>,
}

/// Lista compartilhada entre thread D-Bus e main loop. Render lê em
/// cada frame; thread escreve em registers/updates.
pub type SniList = Arc<Mutex<Vec<SniItem>>>;

/// Inicia a thread D-Bus em background. Retorna a lista compartilhada
/// (vazia até a thread popular). Se a conexão falhar, a thread loga e
/// sai — o luma continua funcionando, painel só ficará sempre vazio.
pub fn spawn() -> SniList {
    let list: SniList = Arc::new(Mutex::new(Vec::new()));
    let list_for_thread = list.clone();
    thread::Builder::new()
        .name("luma-sni".into())
        .spawn(move || {
            if let Err(e) = run(list_for_thread) {
                tracing::warn!("SNI: thread encerrou com erro: {e}");
            }
        })
        .expect("falhou spawnar thread SNI");
    list
}

/// Loop principal da thread. Conecta, registra o host, escuta signals
/// e re-fetcha properties em mudanças. Bloqueia até crash do bus.
fn run(_list: SniList) -> Result<(), Box<dyn std::error::Error>> {
    // Conexão sync (blocking::Connection) — sem async runtime no caller.
    let conn = zbus::blocking::Connection::session()?;
    let pid = std::process::id();
    let host_name = format!("org.kde.StatusNotifierHost-{}", pid);
    // RequestName via DBus interface manual (sem proxy gen ainda).
    let unique = conn.unique_name().map(|n| n.to_string()).unwrap_or_default();
    tracing::info!("SNI: D-Bus connected, unique={unique}, requesting {host_name}");
    let dbus_reply: u32 = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "RequestName",
            &(host_name.as_str(), 4u32), // 4 = DO_NOT_QUEUE
        )?
        .body()
        .deserialize()?;
    if dbus_reply != 1 && dbus_reply != 4 {
        // 1 = PRIMARY_OWNER, 4 = ALREADY_OWNER. 2/3 = não pegou.
        tracing::warn!("SNI: RequestName retornou {dbus_reply} (esperava 1 ou 4)");
    }
    // Registra como host no Watcher.
    let _ = conn.call_method(
        Some("org.kde.StatusNotifierWatcher"),
        "/StatusNotifierWatcher",
        Some("org.kde.StatusNotifierWatcher"),
        "RegisterStatusNotifierHost",
        &(unique.as_str(),),
    )?;
    tracing::info!("SNI: registrado como host (PID {pid})");
    // Loop de manutenção — thread fica viva. As próximas etapas
    // (tray.6+) populam a lista a partir de signals do Watcher.
    loop {
        thread::sleep(std::time::Duration::from_secs(60));
    }
}
