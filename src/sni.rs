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
fn run(list: SniList) -> Result<(), Box<dyn std::error::Error>> {
    let conn = zbus::blocking::Connection::session()?;
    let pid = std::process::id();
    let host_name = format!("org.kde.StatusNotifierHost-{}", pid);
    let unique = conn.unique_name().map(|n| n.to_string()).unwrap_or_default();
    tracing::info!("SNI: D-Bus connected, unique={unique}, requesting {host_name}");
    let dbus_reply: u32 = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "RequestName",
            &(host_name.as_str(), 4u32),
        )?
        .body()
        .deserialize()?;
    if dbus_reply != 1 && dbus_reply != 4 {
        tracing::warn!("SNI: RequestName retornou {dbus_reply} (esperava 1 ou 4)");
    }
    let _ = conn.call_method(
        Some("org.kde.StatusNotifierWatcher"),
        "/StatusNotifierWatcher",
        Some("org.kde.StatusNotifierWatcher"),
        "RegisterStatusNotifierHost",
        &(unique.as_str(),),
    )?;
    tracing::info!("SNI: registrado como host (PID {pid})");

    // H47b.tray.6: lista inicial via property `RegisteredStatusNotifierItems`.
    let registered: Vec<String> = match conn.call_method(
        Some("org.kde.StatusNotifierWatcher"),
        "/StatusNotifierWatcher",
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &("org.kde.StatusNotifierWatcher", "RegisteredStatusNotifierItems"),
    ) {
        Ok(reply) => reply
            .body()
            .deserialize::<zbus::zvariant::Value>()
            .ok()
            .and_then(|v| {
                // Property vem wrapped em Variant(Array(String)).
                <Vec<String> as TryFrom<zbus::zvariant::Value>>::try_from(v).ok()
            })
            .unwrap_or_default(),
        Err(e) => {
            tracing::warn!("SNI: falha lendo RegisteredStatusNotifierItems: {e}");
            Vec::new()
        }
    };
    tracing::info!("SNI: {} itens iniciais registrados", registered.len());
    for name in &registered {
        add_item(&conn, &list, name);
    }

    // H47b.tray.6: signals. AddMatch via DBus.AddMatch (low-level),
    // depois drena mensagens pela conexão.
    add_match(
        &conn,
        "type='signal',sender='org.kde.StatusNotifierWatcher',interface='org.kde.StatusNotifierWatcher',member='StatusNotifierItemRegistered'",
    )?;
    add_match(
        &conn,
        "type='signal',sender='org.kde.StatusNotifierWatcher',interface='org.kde.StatusNotifierWatcher',member='StatusNotifierItemUnregistered'",
    )?;

    // Loop de drain — message_stream() entrega cada mensagem que casa
    // com os matches. Blocking iterator.
    use zbus::blocking::MessageIterator;
    let it = MessageIterator::from(&conn);
    for msg in it {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("SNI: erro lendo mensagem: {e}");
                continue;
            }
        };
        let header = msg.header();
        let member = header.member().map(|m| m.as_str().to_string());
        let iface = header.interface().map(|i| i.to_string());
        if iface.as_deref() != Some("org.kde.StatusNotifierWatcher") {
            continue;
        }
        match member.as_deref() {
            Some("StatusNotifierItemRegistered") => {
                if let Ok(name) = msg.body().deserialize::<String>() {
                    tracing::info!("SNI: item registrado: {name}");
                    add_item(&conn, &list, &name);
                }
            }
            Some("StatusNotifierItemUnregistered") => {
                if let Ok(name) = msg.body().deserialize::<String>() {
                    tracing::info!("SNI: item removido: {name}");
                    remove_item(&list, &name);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn add_match(
    conn: &zbus::blocking::Connection,
    rule: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "AddMatch",
        &(rule,),
    )?;
    Ok(())
}

/// Parse `service[/object_path]` → (service, path). Default path se
/// omitido é `/StatusNotifierItem`.
fn split_item_name(raw: &str) -> (String, String) {
    if let Some(idx) = raw.find('/') {
        (raw[..idx].to_string(), raw[idx..].to_string())
    } else {
        (raw.to_string(), "/StatusNotifierItem".to_string())
    }
}

/// Lê as 4 properties básicas do item e insere/atualiza no list.
/// Tray.8+ vai expandir pra ler também `IconPixmap`.
fn add_item(conn: &zbus::blocking::Connection, list: &SniList, raw_name: &str) {
    let (service, path) = split_item_name(raw_name);
    let title = read_string_prop(conn, &service, &path, "Title").unwrap_or_default();
    let icon_name = read_string_prop(conn, &service, &path, "IconName").unwrap_or_default();
    // ToolTip é um struct (s, a(iiay), s, s). Por enquanto pegamos
    // só o título textual (4º field). Pixmap dele fica pra depois.
    let tooltip = read_tooltip(conn, &service, &path).unwrap_or_default();
    let icon_pixmaps = if icon_name.is_empty() {
        read_pixmaps(conn, &service, &path).unwrap_or_default()
    } else {
        Vec::new()
    };
    let item = SniItem {
        service: service.clone(),
        object_path: path.clone(),
        title,
        tooltip,
        icon_name,
        icon_pixmaps,
    };
    if let Ok(mut guard) = list.lock() {
        guard.retain(|i| !(i.service == service && i.object_path == path));
        guard.push(item);
    }
}

fn remove_item(list: &SniList, raw_name: &str) {
    let (service, path) = split_item_name(raw_name);
    if let Ok(mut guard) = list.lock() {
        guard.retain(|i| !(i.service == service && i.object_path == path));
    }
}

fn read_string_prop(
    conn: &zbus::blocking::Connection,
    service: &str,
    path: &str,
    prop: &str,
) -> Option<String> {
    let reply = conn
        .call_method(
            Some(service),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.kde.StatusNotifierItem", prop),
        )
        .ok()?;
    let body = reply.body();
    let val: zbus::zvariant::Value = body.deserialize().ok()?;
    String::try_from(val).ok()
}

fn read_tooltip(
    conn: &zbus::blocking::Connection,
    service: &str,
    path: &str,
) -> Option<String> {
    let reply = conn
        .call_method(
            Some(service),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.kde.StatusNotifierItem", "ToolTip"),
        )
        .ok()?;
    // Properties.Get retorna `v`. Deserializa como Value e converte
    // pra tupla (icon_name, icon_pixmaps, title, body) — só usamos o
    // title.
    let body = reply.body();
    let val: zbus::zvariant::Value = body.deserialize().ok()?;
    let tup: (String, Vec<(i32, i32, Vec<u8>)>, String, String) = val.try_into().ok()?;
    Some(tup.2)
}

fn read_pixmaps(
    conn: &zbus::blocking::Connection,
    service: &str,
    path: &str,
) -> Option<Vec<SniPixmap>> {
    let reply = conn
        .call_method(
            Some(service),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.kde.StatusNotifierItem", "IconPixmap"),
        )
        .ok()?;
    let body = reply.body();
    let val: zbus::zvariant::Value = body.deserialize().ok()?;
    // Array de struct (i, i, ay).
    let array: Vec<(i32, i32, Vec<u8>)> = val.try_into().ok()?;
    Some(
        array
            .into_iter()
            .map(|(w, h, data)| SniPixmap { width: w, height: h, data })
            .collect(),
    )
}
