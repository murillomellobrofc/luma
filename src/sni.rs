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

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

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
    /// Object path do DBusMenu desse item (property `Menu`).
    /// Vazio se o app não expõe menu. H47c.
    pub menu_path: String,
}

/// Item de DBusMenu retornado por `get_menu_layout`. Flat (sem
/// children) na v1. H47c.
#[derive(Debug, Clone)]
pub struct DBusMenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    /// `true` quando type=="separator".
    pub separator: bool,
}

/// Lista compartilhada entre thread D-Bus e main loop. Render lê em
/// cada frame; thread escreve em registers/updates.
pub type SniList = Arc<Mutex<Vec<SniItem>>>;

/// Inicia a thread D-Bus em background. Retorna a lista compartilhada
/// (vazia até a thread popular). Se a conexão falhar, a thread loga e
/// sai — o luma continua funcionando, painel só ficará sempre vazio.
///
/// `LUMA_FAKE_SNI=1` injeta 5 itens fake (ícones do tema) só pra
/// validar render + hit-test sem precisar de apps SNI instalados.
/// Click ainda chama Activate/ContextMenu, mas o destino é inválido —
/// log de warn, sem efeito visível.
pub fn spawn() -> SniList {
    let list: SniList = Arc::new(Mutex::new(Vec::new()));
    if std::env::var("LUMA_FAKE_SNI").is_ok() {
        if let Ok(mut g) = list.lock() {
            // Nomes XDG genéricos (freedesktop icon naming spec) +
            // luma SVGs próprios. Garante que algo aparece em todo
            // sistema. H47b.fake.
            for (i, name) in [
                "applications-internet",
                "applications-multimedia",
                "applications-system",
                "applications-graphics",
                "applications-games",
            ]
            .iter()
            .enumerate()
            {
                g.push(SniItem {
                    service: format!(":fake.{}", i),
                    object_path: "/StatusNotifierItem".into(),
                    title: name.to_string(),
                    tooltip: format!("Fake item {} ({})", i + 1, name),
                    icon_name: name.to_string(),
                    icon_pixmaps: Vec::new(),
                    menu_path: String::new(),
                });
            }
        }
        tracing::info!("SNI: LUMA_FAKE_SNI ativo — 5 itens fake injetados");
    }
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
    // H47b.tray.11: signals dos items pra live update (NewIcon, NewTitle,
    // NewToolTip, NewStatus). Wide match — re-fetcha tudo do sender.
    add_match(
        &conn,
        "type='signal',interface='org.kde.StatusNotifierItem'",
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
        match (iface.as_deref(), member.as_deref()) {
            (Some("org.kde.StatusNotifierWatcher"), Some("StatusNotifierItemRegistered")) => {
                if let Ok(name) = msg.body().deserialize::<String>() {
                    tracing::info!("SNI: item registrado: {name}");
                    add_item(&conn, &list, &name);
                }
            }
            (Some("org.kde.StatusNotifierWatcher"), Some("StatusNotifierItemUnregistered")) => {
                if let Ok(name) = msg.body().deserialize::<String>() {
                    tracing::info!("SNI: item removido: {name}");
                    remove_item(&list, &name);
                }
            }
            (Some("org.kde.StatusNotifierItem"), Some(member)) => {
                // H47b.tray.11: any change on a known item -> refresh.
                // sender vem do header; combina com `service` salvo.
                let sender = header.sender().map(|s| s.to_string());
                if let Some(sender) = sender {
                    if matches!(
                        member,
                        "NewIcon" | "NewTitle" | "NewToolTip" | "NewStatus" | "NewAttentionIcon"
                    ) {
                        refresh_by_sender(&conn, &list, &sender);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Re-fetcha properties do(s) item(s) cujo `service` == sender e
/// substitui no list. Usado pelos signals NewIcon/NewTitle/etc.
fn refresh_by_sender(conn: &zbus::blocking::Connection, list: &SniList, sender: &str) {
    let targets: Vec<(String, String)> = match list.lock() {
        Ok(g) => g
            .iter()
            .filter(|i| i.service == sender)
            .map(|i| (i.service.clone(), i.object_path.clone()))
            .collect(),
        Err(_) => return,
    };
    for (service, path) in targets {
        let raw = if path == "/StatusNotifierItem" {
            service
        } else {
            format!("{}{}", service, path)
        };
        add_item(conn, list, &raw);
    }
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
    // H47c: lê o path do DBusMenu (property "Menu" — object path).
    let menu_path = read_menu_path(conn, &service, &path).unwrap_or_default();
    let item = SniItem {
        service: service.clone(),
        object_path: path.clone(),
        title,
        tooltip,
        icon_name,
        icon_pixmaps,
        menu_path,
    };
    if let Ok(mut guard) = list.lock() {
        guard.retain(|i| !(i.service == service && i.object_path == path));
        guard.push(item);
    }
}

/// Manda Activate(x, y) pro item. Click esquerdo. H47b.tray.12.
/// Cada chamada abre conexão nova (fire-and-forget, ~10ms).
pub fn activate(service: &str, path: &str, x: i32, y: i32) {
    let conn = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("SNI: Activate falhou (sem conexao): {e}");
            return;
        }
    };
    if let Err(e) = conn.call_method(
        Some(service),
        path,
        Some("org.kde.StatusNotifierItem"),
        "Activate",
        &(x, y),
    ) {
        tracing::warn!("SNI: Activate({service}{path}) erro: {e}");
    }
}

/// Lê o object path do DBusMenu do item (property `Menu`). H47c.
fn read_menu_path(
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
            &("org.kde.StatusNotifierItem", "Menu"),
        )
        .ok()?;
    let body = reply.body();
    let val: zbus::zvariant::Value = body.deserialize().ok()?;
    // Menu vem como ObjectPath ('o' signature).
    if let zbus::zvariant::Value::ObjectPath(p) = val {
        let s = p.to_string();
        if s.is_empty() || s == "/" {
            None
        } else {
            Some(s)
        }
    } else {
        None
    }
}

/// Lê o layout (flat, sem recursão) do DBusMenu do item via
/// `com.canonical.dbusmenu.GetLayout(parent_id=0, depth=1, props=[])`.
/// Retorna a lista de items filhos do root. H47c.
pub fn get_menu_layout(service: &str, menu_path: &str) -> Vec<DBusMenuItem> {
    let conn = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("SNI: GetLayout sem conexao: {e}");
            return Vec::new();
        }
    };
    // GetLayout(parent_id: i32, recursion_depth: i32, property_names: as)
    // → (revision: u32, layout: (i32, a{sv}, av))
    let property_names: Vec<&str> = vec!["label", "enabled", "type", "visible"];
    let reply = match conn.call_method(
        Some(service),
        menu_path,
        Some("com.canonical.dbusmenu"),
        "GetLayout",
        &(0i32, 1i32, property_names.as_slice()),
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("SNI: GetLayout({service}{menu_path}) erro: {e}");
            return Vec::new();
        }
    };
    // Resposta: (u, (i, a{sv}, av))
    let body = reply.body();
    let parsed: Result<
        (
            u32,
            (
                i32,
                std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
                Vec<zbus::zvariant::OwnedValue>,
            ),
        ),
        _,
    > = body.deserialize();
    let (_revision, (_root_id, _root_props, children)) = match parsed {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("SNI: GetLayout parse erro: {e}");
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for child in children {
        // Cada child é Variant contendo (i, a{sv}, av).
        // Deserializa o conteúdo do variant.
        let inner: Result<
            (
                i32,
                std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
                Vec<zbus::zvariant::OwnedValue>,
            ),
            _,
        > = child.try_into();
        let (id, props, _grandchildren) = match inner {
            Ok(t) => t,
            Err(_) => continue,
        };
        let label = props
            .get("label")
            .and_then(|v| {
                let val: zbus::zvariant::Value = (*v).clone().into();
                String::try_from(val).ok()
            })
            .map(|s| s.replace('_', "")) // strip mnemonic underscores
            .unwrap_or_default();
        let enabled = props
            .get("enabled")
            .and_then(|v| {
                let val: zbus::zvariant::Value = (*v).clone().into();
                bool::try_from(val).ok()
            })
            .unwrap_or(true);
        let visible = props
            .get("visible")
            .and_then(|v| {
                let val: zbus::zvariant::Value = (*v).clone().into();
                bool::try_from(val).ok()
            })
            .unwrap_or(true);
        if !visible {
            continue;
        }
        let typ = props
            .get("type")
            .and_then(|v| {
                let val: zbus::zvariant::Value = (*v).clone().into();
                String::try_from(val).ok()
            })
            .unwrap_or_default();
        let separator = typ == "separator";
        out.push(DBusMenuItem {
            id,
            label,
            enabled,
            separator,
        });
    }
    out
}

/// Manda `Event(id, "clicked", data="", timestamp=0)` pro DBusMenu —
/// dispara a ação ligada ao item. H47c.
pub fn send_menu_event(service: &str, menu_path: &str, item_id: i32) {
    let conn = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("SNI: Event sem conexao: {e}");
            return;
        }
    };
    let data: zbus::zvariant::Value = "".into();
    if let Err(e) = conn.call_method(
        Some(service),
        menu_path,
        Some("com.canonical.dbusmenu"),
        "Event",
        &(item_id, "clicked", &data, 0u32),
    ) {
        tracing::warn!("SNI: Event({service}{menu_path} id={item_id}) erro: {e}");
    }
}

/// Manda ContextMenu(x, y) pro item. Click direito — o app desenha
/// o próprio menu (DBusMenu nativo fica pra H47c). H47b.tray.13.
pub fn context_menu(service: &str, path: &str, x: i32, y: i32) {
    let conn = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("SNI: ContextMenu falhou (sem conexao): {e}");
            return;
        }
    };
    if let Err(e) = conn.call_method(
        Some(service),
        path,
        Some("org.kde.StatusNotifierItem"),
        "ContextMenu",
        &(x, y),
    ) {
        tracing::warn!("SNI: ContextMenu({service}{path}) erro: {e}");
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

/// Carrega o ícone do item SNI em `size×size`. Cascata de fallbacks:
/// 1. `icon_name` via IconIndex (tema XDG, cobre Breeze/Adwaita/hicolor)
/// 2. `icon_name` como SVG dos assets do próprio luma (icon-<name>.svg)
/// 3. Maior pixmap fornecido pelo app via `IconPixmap`
/// H47b.tray.8.
pub fn load_icon(item: &SniItem, size: i32) -> Option<MemoryRenderBuffer> {
    if !item.icon_name.is_empty() {
        if let Some(path) = crate::apps::resolve_icon_path(&item.icon_name) {
            if path.extension().and_then(|e| e.to_str()) == Some("svg") {
                if let Some(buf) = crate::menu::icon_loader::load_svg_file(
                    &path.to_string_lossy(),
                    size as u32,
                ) {
                    return Some(buf);
                }
            }
            if let Ok(img) = image::open(&path) {
                return Some(image_to_buffer(img, size));
            }
        }
        // Fallback: tenta como SVG dos próprios assets do luma.
        let theme = crate::theme::current();
        let local = format!(
            "{}/icon-{}.svg",
            theme.taskbar.system_tray.icons_path, item.icon_name
        );
        if let Some(buf) = crate::menu::icon_loader::load_svg_file(&local, size as u32) {
            return Some(buf);
        }
    }
    let best = item
        .icon_pixmaps
        .iter()
        .max_by_key(|p| p.width.min(p.height))?;
    pixmap_to_buffer(best, size)
}

fn image_to_buffer(img: image::DynamicImage, size: i32) -> MemoryRenderBuffer {
    let resized = img.resize_to_fill(
        size as u32,
        size as u32,
        image::imageops::FilterType::Lanczos3,
    );
    let rgba = resized.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    let mut pixels = rgba.into_raw();
    // RGBA → BGRA premultiplicado.
    for chunk in pixels.chunks_exact_mut(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];
        chunk[0] = ((b as u16 * a as u16) / 255) as u8;
        chunk[1] = ((g as u16 * a as u16) / 255) as u8;
        chunk[2] = ((r as u16 * a as u16) / 255) as u8;
        chunk[3] = a;
    }
    MemoryRenderBuffer::from_slice(&pixels, Fourcc::Argb8888, (w, h), 1, Transform::Normal, None)
}

/// Converte `IconPixmap` (ARGB32 big-endian) pra BGRA premultiplicado.
/// Se o size pedido difere do native, re-encoda via image crate.
fn pixmap_to_buffer(px: &SniPixmap, target_size: i32) -> Option<MemoryRenderBuffer> {
    if px.width <= 0 || px.height <= 0 {
        return None;
    }
    let expected = (px.width * px.height * 4) as usize;
    if px.data.len() != expected {
        return None;
    }
    // SNI pixmap = ARGB big-endian. Converte pra RGBA pra entregar
    // ao image::ImageBuffer.
    let mut rgba = Vec::with_capacity(expected);
    for chunk in px.data.chunks_exact(4) {
        let a = chunk[0];
        let r = chunk[1];
        let g = chunk[2];
        let b = chunk[3];
        rgba.push(r);
        rgba.push(g);
        rgba.push(b);
        rgba.push(a);
    }
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(px.width as u32, px.height as u32, rgba)?;
    let dyn_img = image::DynamicImage::ImageRgba8(img);
    Some(image_to_buffer(dyn_img, target_size))
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
