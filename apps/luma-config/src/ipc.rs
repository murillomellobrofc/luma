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

    /// Alvos de toque (parte 2): os dois controles + o efetivo (read-only).
    async fn larger_targets(&self) -> zbus::Result<bool>;
    async fn set_larger_targets(&self, enabled: bool) -> zbus::Result<()>;
    async fn min_target_size(&self) -> zbus::Result<u16>;
    async fn set_min_target_size(&self, size: u16) -> zbus::Result<()>;
    async fn hit_target_min(&self) -> zbus::Result<u16>;

    /// Sinal de mudança dos alvos — carrega o alvo EFETIVO (px).
    #[zbus(signal)]
    fn targets_changed(&self, effective: u16) -> zbus::Result<()>;

    /// Realçar foco do teclado (Teclado passo 4): get/set + sinal de mudança,
    /// mesmo padrão do alto contraste. O app re-renderiza o anel mais forte.
    async fn focus_highlight(&self) -> zbus::Result<bool>;
    async fn set_focus_highlight(&self, enabled: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    fn focus_highlight_changed(&self, enabled: bool) -> zbus::Result<()>;
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

// ─── Alvos de toque (parte 2) ─────────────────────────────────────────

/// Lê o alvo de toque EFETIVO (px) — pro boot aplicar o tamanho certo.
pub async fn read_hit_target_min(conn: zbus::Connection) -> Result<u16, String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy.hit_target_min().await.map_err(|e| e.to_string())
}

/// Lê os dois controles (larger, min_size) — pro read-on-open inicializar
/// o toggle + o slider da página Alvos de toque.
pub async fn read_target_settings(conn: zbus::Connection) -> Result<(bool, u16), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    let larger = proxy.larger_targets().await.map_err(|e| e.to_string())?;
    let size = proxy.min_target_size().await.map_err(|e| e.to_string())?;
    Ok((larger, size))
}

/// Escreve "alvos maiores" (toggle, commita na hora).
pub async fn write_larger_targets(conn: zbus::Connection, enabled: bool) -> Result<(), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .set_larger_targets(enabled)
        .await
        .map_err(|e| e.to_string())
}

/// Escreve o tamanho do alvo (slider, commitado no soltar).
pub async fn write_min_target_size(conn: zbus::Connection, size: u16) -> Result<(), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .set_min_target_size(size)
        .await
        .map_err(|e| e.to_string())
}

/// Stream do sinal `targets_changed` — emite o alvo EFETIVO (px) a cada
/// mudança. A `Subscription` aplica via `set_hit_target_min` e re-layouta.
pub async fn targets_signal_stream(
) -> zbus::Result<impl iced::futures::Stream<Item = u16>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_targets_changed().await?;
    Ok(signals.filter_map(|sig| async move {
        sig.args().ok().map(|a| *a.effective())
    }))
}

// ─── Realçar foco do teclado (Teclado passo 4) ────────────────────────

/// Lê o realçar foco do compositor (boot + read-on-open).
pub async fn read_focus_highlight(conn: zbus::Connection) -> Result<bool, String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy.focus_highlight().await.map_err(|e| e.to_string())
}

/// Escreve o realçar foco (toggle). O re-render vem do SINAL (fonte única).
pub async fn write_focus_highlight(conn: zbus::Connection, enabled: bool) -> Result<(), String> {
    let proxy = CompositorProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .set_focus_highlight(enabled)
        .await
        .map_err(|e| e.to_string())
}

/// Stream do sinal `focus_highlight_changed` — emite o novo `bool` a cada
/// mudança. A `Subscription` aplica via `set_focus_highlight` e re-renderiza.
pub async fn focus_highlight_signal_stream(
) -> zbus::Result<impl iced::futures::Stream<Item = bool>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_focus_highlight_changed().await?;
    Ok(signals.filter_map(|sig| async move { sig.args().ok().map(|a| *a.enabled()) }))
}
