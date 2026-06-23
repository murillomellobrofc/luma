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

    /// Abre/fecha o luma-menu (botão da barra / Super). Emite `menu_toggle`.
    async fn show_menu(&self) -> zbus::Result<()>;

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

    /// Bluetooth: nome do ícone do estado ("bluetooth-on"/"bluetooth-off"/"").
    async fn bluetooth(&self) -> zbus::Result<String>;
    #[zbus(signal)]
    fn bluetooth_changed(&self, icon: &str) -> zbus::Result<()>;

    /// Mixer: dispositivos de áudio `(id, nome, vol% 0–150, mudo, é_entrada,
    /// é_default)`. Lido ao abrir o popup + após cada ajuste.
    async fn audio_devices(&self) -> zbus::Result<Vec<(u32, String, u32, bool, bool, bool)>>;
    /// Define o volume (0–150%) de um dispositivo pela id.
    async fn set_device_volume(&self, id: u32, percent: u32) -> zbus::Result<()>;
    /// Alterna o mudo de um dispositivo pela id.
    async fn set_device_mute(&self, id: u32) -> zbus::Result<()>;

    /// Bluetooth popup: `(ligado, tem_hardware, [(mac, nome, pareado,
    /// conectado)])`.
    async fn bluetooth_state(&self) -> zbus::Result<(bool, bool, Vec<(String, String, bool, bool)>)>;
    /// Liga/desliga o controller.
    async fn set_bluetooth_power(&self, on: bool) -> zbus::Result<()>;
    /// Conecta um dispositivo pareado pela MAC.
    async fn connect_device(&self, mac: String) -> zbus::Result<()>;
    /// Desconecta um dispositivo pela MAC.
    async fn disconnect_device(&self, mac: String) -> zbus::Result<()>;

    /// Varredura (modo adicionar): liga/desliga a procura.
    async fn start_scan(&self) -> zbus::Result<()>;
    async fn stop_scan(&self) -> zbus::Result<()>;
    /// Lista do modo varredura: pareados + descobertos `(mac, nome, pareado,
    /// conectado)`.
    async fn scan_devices(&self) -> zbus::Result<Vec<(String, String, bool, bool)>>;
    /// Pareia um device pela MAC (com o agent do código).
    async fn pair_device(&self, mac: String) -> zbus::Result<()>;
    /// Responde a confirmação de código (confirma/cancela).
    async fn confirm_pairing(&self, ok: bool) -> zbus::Result<()>;
    /// Fase do pareamento `(fase, passkey, mensagem)`.
    async fn pairing_state(&self) -> zbus::Result<(u8, u32, String)>;
    /// Volta a fase pra ociosa.
    async fn reset_pairing(&self) -> zbus::Result<()>;

    /// Rede popup: `(wifi_ligado, tem_placa, cabeada ou "", [(ssid, sinal,
    /// protegida, conectada)])`.
    async fn network_devices(&self) -> zbus::Result<(bool, bool, String, Vec<(String, u8, bool, bool)>)>;
    /// Conecta numa rede wi-fi pelo SSID.
    async fn connect_wifi(&self, ssid: String) -> zbus::Result<()>;
    /// Liga/desliga o rádio wi-fi.
    async fn set_wifi_radio(&self, on: bool) -> zbus::Result<()>;
    /// Pede um rescan das redes.
    async fn wifi_rescan(&self) -> zbus::Result<()>;
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

/// Abre/fecha o luma-menu (botão da barra). Fire-and-forget.
pub async fn show_menu() {
    let conn = zbus::Connection::session().await;
    window_cmd("show_menu", async move {
        CompositorProxy::new(&conn?).await?.show_menu().await
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

// ─── Mixer (popup do volume) ──────────────────────────────────────────

/// Lê a lista de dispositivos de áudio (saídas + entradas) com vol/mudo.
pub async fn read_audio_devices() -> Vec<(u32, String, u32, bool, bool, bool)> {
    async fn inner() -> zbus::Result<Vec<(u32, String, u32, bool, bool, bool)>> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.audio_devices().await
    }
    inner().await.unwrap_or_default()
}

/// Define o volume (0–150%) de um dispositivo. Depois relê a lista.
pub async fn set_device_volume(id: u32, percent: u32) -> Vec<(u32, String, u32, bool, bool, bool)> {
    async fn inner(id: u32, percent: u32) -> zbus::Result<()> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.set_device_volume(id, percent).await
    }
    if let Err(e) = inner(id, percent).await {
        eprintln!("luma-bar: set_device_volume({id},{percent}) falhou: {e}");
    }
    read_audio_devices().await
}

/// Alterna o mudo de um dispositivo. Depois relê a lista.
pub async fn set_device_mute(id: u32) -> Vec<(u32, String, u32, bool, bool, bool)> {
    async fn inner(id: u32) -> zbus::Result<()> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.set_device_mute(id).await
    }
    if let Err(e) = inner(id).await {
        eprintln!("luma-bar: set_device_mute({id}) falhou: {e}");
    }
    read_audio_devices().await
}

// ─── Bluetooth (popup) ────────────────────────────────────────────────

/// Estado do bluetooth: `(ligado, tem_hardware, [(mac, nome, pareado,
/// conectado)])`.
pub type BtState = (bool, bool, Vec<(String, String, bool, bool)>);

pub async fn read_bluetooth_state() -> BtState {
    async fn inner() -> zbus::Result<BtState> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.bluetooth_state().await
    }
    inner().await.unwrap_or((false, false, Vec::new()))
}

/// Liga/desliga + relê (com um respiro pro bluetoothctl assentar).
pub async fn set_bluetooth_power(on: bool) -> BtState {
    async fn inner(on: bool) -> zbus::Result<()> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.set_bluetooth_power(on).await
    }
    if let Err(e) = inner(on).await {
        eprintln!("luma-bar: set_bluetooth_power falhou: {e}");
    }
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    read_bluetooth_state().await
}

/// Conecta + relê (connect leva alguns segundos; relê depois pra refletir).
pub async fn connect_device(mac: String) -> BtState {
    async fn inner(mac: String) -> zbus::Result<()> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.connect_device(mac).await
    }
    if let Err(e) = inner(mac).await {
        eprintln!("luma-bar: connect_device falhou: {e}");
    }
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    read_bluetooth_state().await
}

/// Desconecta + relê.
pub async fn disconnect_device(mac: String) -> BtState {
    async fn inner(mac: String) -> zbus::Result<()> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.disconnect_device(mac).await
    }
    if let Err(e) = inner(mac).await {
        eprintln!("luma-bar: disconnect_device falhou: {e}");
    }
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    read_bluetooth_state().await
}

// ─── Bluetooth: pareamento (refino) ───────────────────────────────────

/// Fase do pareamento `(fase, passkey, mensagem)`.
pub type PairState = (u8, u32, String);

async fn proxy() -> zbus::Result<CompositorProxy<'static>> {
    let conn = zbus::Connection::session().await?;
    CompositorProxy::new(&conn).await
}

/// Liga a varredura (modo adicionar).
pub async fn start_scan() {
    if let Ok(p) = proxy().await {
        let _ = p.start_scan().await;
    }
}

/// Desliga a varredura e zera a fase.
pub async fn stop_scan() {
    if let Ok(p) = proxy().await {
        let _ = p.stop_scan().await;
        let _ = p.reset_pairing().await;
    }
}

/// Lê a lista do modo varredura (pareados + descobertos).
pub async fn read_scan_devices() -> Vec<(String, String, bool, bool)> {
    match proxy().await {
        Ok(p) => p.scan_devices().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Lê a fase do pareamento.
pub async fn read_pairing_state() -> PairState {
    match proxy().await {
        Ok(p) => p.pairing_state().await.unwrap_or((0, 0, String::new())),
        Err(_) => (0, 0, String::new()),
    }
}

/// Dispara o pareamento de um device (fire-and-forget; acompanhe por
/// `read_pairing_state`).
pub async fn pair_device(mac: String) {
    if let Ok(p) = proxy().await {
        let _ = p.pair_device(mac).await;
    }
}

/// Responde a confirmação de código.
pub async fn confirm_pairing(ok: bool) {
    if let Ok(p) = proxy().await {
        let _ = p.confirm_pairing(ok).await;
    }
}

// ─── Rede: popup (conexões) ───────────────────────────────────────────

/// `(wifi_ligado, tem_placa, cabeada, [(ssid, sinal, protegida, conectada)])`.
pub type NetState = (bool, bool, String, Vec<(String, u8, bool, bool)>);

/// Lê o estado de rede pro popup. Pede um rescan antes (lista mais fresca).
pub async fn read_network_devices() -> NetState {
    if let Ok(p) = proxy().await {
        let _ = p.wifi_rescan().await;
        return p
            .network_devices()
            .await
            .unwrap_or((false, false, String::new(), Vec::new()));
    }
    (false, false, String::new(), Vec::new())
}

/// Conecta numa rede wi-fi + relê (connect leva alguns segundos).
pub async fn connect_wifi(ssid: String) -> NetState {
    if let Ok(p) = proxy().await {
        let _ = p.connect_wifi(ssid).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    read_network_devices().await
}

/// Liga/desliga o rádio wi-fi + relê.
pub async fn set_wifi_radio(on: bool) -> NetState {
    if let Ok(p) = proxy().await {
        let _ = p.set_wifi_radio(on).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    read_network_devices().await
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

pub async fn read_bluetooth() -> String {
    async fn inner() -> zbus::Result<String> {
        let conn = zbus::Connection::session().await?;
        let proxy = CompositorProxy::new(&conn).await?;
        proxy.bluetooth().await
    }
    inner().await.unwrap_or_default()
}

/// Stream do sinal `bluetooth_changed` — o nome do ícone novo.
pub async fn bluetooth_signal_stream() -> zbus::Result<impl iced::futures::Stream<Item = String>> {
    use iced::futures::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = CompositorProxy::new(&conn).await?;
    let signals = proxy.receive_bluetooth_changed().await?;
    Ok(signals.filter_map(|sig| async move { sig.args().ok().map(|a| a.icon().to_string()) }))
}
