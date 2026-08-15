//! Servidor D-Bus do compositor. Expõe `dev.luma.Compositor` no
//! session bus. Roda numa thread dedicada com runtime tokio
//! current_thread — isola o async do zbus do loop síncrono do calloop.
//!
//! Comandos que precisam mexer no estado do compositor (AddWidget,
//! 3c.1; outros nas próximas fatias) viajam pelo channel `IpcCommand`
//! registrado no calloop. Cada comando carrega um `oneshot::Sender`
//! pra resposta: a thread D-Bus despacha o comando + `await` no
//! receiver, o loop principal toca o estado e responde pelo sender.
//! Sem race — só o loop principal toca `LumaState`.

use crate::notifications::NotificationsSlot;
use luma_ipc::{WidgetInfo, WidgetTarget};
use smithay::reexports::calloop::channel::Sender;
use std::thread;
use tracing::{error, info, warn};
use zbus::interface;
use zbus::object_server::SignalEmitter;

/// Bus name e interface name são iguais (padrão fdo, ex: NetworkManager).
const BUS_NAME: &str = "dev.luma.Compositor";
const OBJECT_PATH: &str = "/dev/luma/Compositor";

/// Comandos enviados da thread D-Bus pro loop principal do compositor.
/// Cada variante carrega um `reply` oneshot pra resposta do loop.
pub enum IpcCommand {
    AddWidget {
        type_name: String,
        target: WidgetTarget,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// Ativa (foca + levanta) a janela com este id. Fire-and-forget — a
    /// taskbar (luma-bar) não espera resposta. luma-bar passo 2.
    ActivateWindow(u64),
    /// Minimiza a janela (luma-bar passo 7).
    MinimizeWindow(u64),
    /// Fecha (send_close) a janela (luma-bar passo 7).
    CloseWindow(u64),
    /// Mostrar área de trabalho — toggle com memória (luma-bar passo 7).
    ShowDesktop,
    /// Seta o volume do default sink (0–150%). luma-bar passo 4 (scroll).
    SetVolume(u8),
    /// Alterna mudo do default sink. luma-bar passo 4 (clique).
    ToggleMute,
    /// Bloqueia a sessão (menu passo 3). Roteia pro lock que já existe.
    Lock,
    /// Ação do context menu do desktop pedida pelo cliente (organizar/colar/
    /// criar/wallpaper). Reusa o dispatch do menu velho.
    CtxAction(crate::context_menu::ContextAction),
    /// Vai pra área de trabalho de índice N (0-based). Auto-cria se não existe.
    WorkspaceSwitch(usize),
    /// Cria uma nova área de trabalho vazia e vai pra ela.
    WorkspaceCreate,
    /// Move a janela (pela id) pra área de índice `target` (0-based).
    WorkspaceMoveWindow { window_id: u64, target: usize },
}

/// Foto do estado de workspace pro D-Bus (motor de workspace). Escrita por
/// frame pelo compositor (`update_workspace_slot`), lida pelo `workspace_state`.
/// `windows[i]` = ids das janelas da área de índice `i`.
#[derive(Clone, Debug, Default)]
pub struct WorkspaceSnapshot {
    pub current: u32,
    pub count: u32,
    pub windows: Vec<Vec<u64>>,
}

/// Snapshot de workspace compartilhado (`Arc<Mutex>`), mesmo padrão do
/// `WindowsSlot`. O cliente (central/pager) vai ler daqui.
pub type WorkspaceSlot = std::sync::Arc<std::sync::Mutex<WorkspaceSnapshot>>;

/// Uma janela aberta, pra a taskbar (luma-bar passo 2). `id` é estável por
/// janela (contador do compositor). Estado mínimo: título, app_id, foco.
#[derive(Clone, Debug)]
pub struct WindowEntry {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub focused: bool,
    pub minimized: bool,
}

/// Lista de janelas compartilhada (`Arc<Mutex>`), escrita pelo compositor a
/// cada frame e lida pelo `list_windows` do D-Bus. Mesmo padrão cross-thread
/// do `NotificationsSlot`.
pub type WindowsSlot = std::sync::Arc<std::sync::Mutex<Vec<WindowEntry>>>;

/// Todos os `watch::Receiver` que a thread do D-Bus escuta pra emitir sinal.
///
/// Vieram como parâmetros soltos de `spawn`/`run`/`serve` até virarem oito —
/// e três deles são `Receiver<u64>` (`menu_toggle`, `workspace_changed`,
/// `windows_changed`). Em lista posicional, trocar dois de lugar compila
/// limpo e o bug só aparece em runtime, como sinal errado. Nomeado, não.
pub struct SignalSources {
    pub battery: tokio::sync::watch::Receiver<(bool, u8, bool)>,
    pub volume: tokio::sync::watch::Receiver<(u8, bool)>,
    pub network: tokio::sync::watch::Receiver<String>,
    pub bluetooth: tokio::sync::watch::Receiver<String>,
    pub menu_toggle: tokio::sync::watch::Receiver<u64>,
    pub desktop_menu: tokio::sync::watch::Receiver<(u64, i32, i32, i32, i32)>,
    pub workspace_changed: tokio::sync::watch::Receiver<u64>,
    /// Lista de janelas mudou (zona C da barra) — a barra relê `list_windows`.
    pub windows_changed: tokio::sync::watch::Receiver<u64>,
    /// Show-desktop ligado/desligado (zona F da barra).
    pub desktop_visible: tokio::sync::watch::Receiver<bool>,
}

/// Converte o `BatteryInfo` interno na tupla do D-Bus `(present, capacity,
/// charging)` (luma-bar passo 3). `None` (sem bateria) = `(false, 0, false)`.
pub fn battery_tuple(info: Option<crate::tray::BatteryInfo>) -> (bool, u8, bool) {
    match info {
        Some(b) => (true, b.capacity, b.charging),
        None => (false, 0, false),
    }
}

struct CompositorService {
    /// Catálogo dos tipos de widget conhecidos no boot. Imutável após
    /// init — o registry não cresce em runtime hoje (third-party é
    /// pós-v1). Cópia clonada do lado do calloop pra evitar mexer no
    /// registry de outra thread.
    available_widgets: Vec<WidgetInfo>,
    /// Sender pro loop principal. Cada handler que muta estado
    /// empacota `IpcCommand` + oneshot e aguarda resposta.
    cmd_tx: Sender<IpcCommand>,
    /// Estado de notificações compartilhado (`Arc<Mutex>`). O sino visual
    /// (`flash_enabled`) é lido/escrito direto daqui — esse slot já é
    /// cross-thread por design (o próprio subsistema de notif. roda noutra
    /// thread), então não precisa do roundtrip pelo calloop. 3a.
    notifications: NotificationsSlot,
    /// Lista de janelas compartilhada (luma-bar passo 2). O compositor
    /// escreve a cada frame; `list_windows` lê direto, sem roundtrip.
    windows: WindowsSlot,
    /// Receptor `watch` do estado da bateria (luma-bar passo 3). O `battery`
    /// lê o último valor (`borrow`); o loop do sinal usa outro clone.
    battery: tokio::sync::watch::Receiver<(bool, u8, bool)>,
    /// Receptor `watch` do volume (luma-bar passo 4): `(level 0–100, muted)`.
    volume: tokio::sync::watch::Receiver<(u8, bool)>,
    /// Receptor `watch` da rede (luma-bar passo 5): nome do ícone do estado.
    network: tokio::sync::watch::Receiver<String>,
    /// Receptor `watch` do bluetooth: nome do ícone do estado (on/off/"").
    bluetooth: tokio::sync::watch::Receiver<String>,
    /// Estado de pareamento compartilhado com o agent BlueZ (system bus).
    pairing: crate::bt_agent::PairingSlot,
    /// Conexão persistente do system bus (a do agent) — usada pra Start/Stop
    /// Discovery na MESMA conexão (BlueZ amarra discovery ao sender).
    bt_system: Option<zbus::Connection>,
    /// Lista de itens da bandeja (SNI host), compartilhada com o main loop.
    sni: crate::sni::SniList,
    /// Snapshot de workspace compartilhado (motor de workspace). O compositor
    /// escreve por frame; `workspace_state` lê direto, sem roundtrip.
    workspaces: WorkspaceSlot,
    /// Receptor `watch` do show-desktop (zona F da barra). O `watch` guarda o
    /// último valor, então o método lê com `borrow()` — sem roundtrip.
    desktop_visible: tokio::sync::watch::Receiver<bool>,
}

#[interface(name = "dev.luma.Compositor")]
impl CompositorService {
    /// Smoke test do canal (3b). Retorna `"luma-compositor <versão>"`.
    async fn ping(&self) -> String {
        format!("luma-compositor {}", env!("CARGO_PKG_VERSION"))
    }

    /// Devolve o catálogo de tipos de widget que o compositor sabe
    /// instanciar (3c.1). Cada entrada vem com display_name,
    /// description e icon_name pro picker do app montar os cards.
    async fn list_available_widgets(&self) -> Vec<WidgetInfo> {
        self.available_widgets.clone()
    }

    /// Adiciona um widget ao painel ou desktop (3c.1). `target` é
    /// `"panel"` ou `"desktop"`; outros valores = `InvalidArgs`.
    /// Tipo desconhecido = `InvalidArgs`. Posição no desktop = default
    /// fixo (canto sup-esq com margem); usuário move depois.
    async fn add_widget(
        &self,
        type_name: String,
        target: String,
    ) -> zbus::fdo::Result<()> {
        let target = WidgetTarget::from_str(&target).ok_or_else(|| {
            zbus::fdo::Error::InvalidArgs(format!("target inválido: {target}"))
        })?;

        if !self
            .available_widgets
            .iter()
            .any(|w| w.type_name == type_name)
        {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "widget desconhecido: {type_name}"
            )));
        }

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(IpcCommand::AddWidget {
                type_name,
                target,
                reply: reply_tx,
            })
            .map_err(|_| {
                zbus::fdo::Error::Failed("loop do compositor indisponível".into())
            })?;

        reply_rx
            .await
            .map_err(|_| zbus::fdo::Error::Failed("sem resposta do compositor".into()))?
            .map_err(zbus::fdo::Error::Failed)
    }

    /// Estado atual do **sino visual** (a piscada de tela que substitui o
    /// som de alerta): `true` = a piscada dispara em alertas. 3a — primeiro
    /// wiring real. Lê direto do slot compartilhado de notificações.
    async fn visual_bell_enabled(&self) -> bool {
        match self.notifications.lock() {
            Ok(g) => g.flash_enabled,
            Err(_) => false,
        }
    }

    /// Liga/desliga o **sino visual**. Gira o `flash_enabled` (memória, 3a)
    /// e persiste no disco (3b). O gate de alerta (winit.rs) checa o flag.
    async fn set_visual_bell_enabled(&self, enabled: bool) {
        if let Ok(mut g) = self.notifications.lock() {
            g.flash_enabled = enabled;
            info!("D-Bus: sino visual = {enabled}");
        } else {
            error!("D-Bus: lock de notificações envenenado ao setar sino visual");
        }
        persist_accessibility(&self.notifications);
    }

    /// Estado do **modo seguro pra fotossensibilidade** (3c). `true` = a
    /// intensidade efetiva é capada no teto seguro, sem estrobo.
    async fn photosafe_enabled(&self) -> bool {
        match self.notifications.lock() {
            Ok(g) => g.flash_photosafe,
            Err(_) => true,
        }
    }

    /// Liga/desliga o modo seguro pra fotossensibilidade (3c). Memória +
    /// disco. Com ON, o cap de segurança vale na aplicação do flash.
    async fn set_photosafe_enabled(&self, enabled: bool) {
        if let Ok(mut g) = self.notifications.lock() {
            g.flash_photosafe = enabled;
            info!("D-Bus: photosafe = {enabled}");
        } else {
            error!("D-Bus: lock envenenado ao setar photosafe");
        }
        persist_accessibility(&self.notifications);
    }

    /// **Intensidade do flash** em % (3c, 0–100). Primeiro setting numérico
    /// do stack. Escala a força da piscada (com o cap do photosafe).
    async fn flash_intensity(&self) -> u8 {
        match self.notifications.lock() {
            Ok(g) => g.flash_intensity,
            Err(_) => 60,
        }
    }

    /// Seta a intensidade do flash (3c). Clampa em 0–100 por segurança.
    /// Memória + disco.
    async fn set_flash_intensity(&self, intensity: u8) {
        let clamped = intensity.min(100);
        if let Ok(mut g) = self.notifications.lock() {
            g.flash_intensity = clamped;
            info!("D-Bus: intensidade do flash = {clamped}%");
        } else {
            error!("D-Bus: lock envenenado ao setar intensidade");
        }
        persist_accessibility(&self.notifications);
    }

    /// Estado do **reduzir movimento** (3d). `true` = animações instantâneas.
    async fn reduce_motion(&self) -> bool {
        match self.notifications.lock() {
            Ok(g) => g.reduce_motion,
            Err(_) => false,
        }
    }

    /// Liga/desliga o **reduzir movimento** (3d). Memória + disco. O render
    /// checa esse flag pra pular/instantaneizar as animações.
    async fn set_reduce_motion(&self, enabled: bool) {
        if let Ok(mut g) = self.notifications.lock() {
            g.reduce_motion = enabled;
            info!("D-Bus: reduzir movimento = {enabled}");
        } else {
            error!("D-Bus: lock envenenado ao setar reduzir movimento");
        }
        persist_accessibility(&self.notifications);
    }

    /// Estado do **alto contraste** (HC parte 2).
    async fn high_contrast(&self) -> bool {
        match self.notifications.lock() {
            Ok(g) => g.high_contrast,
            Err(_) => false,
        }
    }

    /// Liga/desliga o **alto contraste** (HC parte 2). Memória + disco +
    /// **emite o sinal de mudança** pros apps re-resolverem a aparência ao
    /// vivo. É a primeira config que muda a APARÊNCIA, daí o sinal — esse
    /// canal serve a fase de personalização inteira. A própria chrome do
    /// compositor também vira HC (via `theme::set_high_contrast` + render
    /// contínuo re-resolvendo a decoração).
    async fn set_high_contrast(
        &self,
        enabled: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        if let Ok(mut g) = self.notifications.lock() {
            g.high_contrast = enabled;
            info!("D-Bus: alto contraste = {enabled}");
        } else {
            error!("D-Bus: lock envenenado ao setar alto contraste");
        }
        persist_accessibility(&self.notifications);
        // Chrome do compositor: gira o atomic do tema; o render contínuo
        // re-resolve a decoração (gradiente/título rebuiltam, accent e
        // glyphs viram) na próxima frame.
        crate::theme::set_high_contrast(enabled);
        if let Err(err) = emitter.high_contrast_changed(enabled).await {
            error!("D-Bus: falha ao emitir sinal de alto contraste: {err}");
        }
    }

    /// **Sinal de mudança** do alto contraste (HC parte 2). Emitido pelo
    /// `set_high_contrast`; os apps escutam via `Subscription` e re-resolvem
    /// os tokens. Sem corpo — o macro gera o emit.
    #[zbus(signal)]
    async fn high_contrast_changed(
        emitter: &SignalEmitter<'_>,
        enabled: bool,
    ) -> zbus::Result<()>;

    /// Estado do **realçar foco do teclado** (Teclado passo 4).
    async fn focus_highlight(&self) -> bool {
        match self.notifications.lock() {
            Ok(g) => g.focus_highlight,
            Err(_) => false,
        }
    }

    /// Liga/desliga o **realçar foco** (passo 4). Memória + disco + **emite o
    /// sinal** pros apps engrossarem o anel de foco ao vivo. O compositor não
    /// desenha foco — só guarda/serve/notifica (diferente do `high_contrast`,
    /// que também gira a chrome).
    async fn set_focus_highlight(
        &self,
        enabled: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        if let Ok(mut g) = self.notifications.lock() {
            g.focus_highlight = enabled;
            info!("D-Bus: realçar foco = {enabled}");
        } else {
            error!("D-Bus: lock envenenado ao setar realçar foco");
        }
        persist_accessibility(&self.notifications);
        if let Err(err) = emitter.focus_highlight_changed(enabled).await {
            error!("D-Bus: falha ao emitir sinal de realçar foco: {err}");
        }
    }

    /// **Sinal de mudança** do realçar foco (passo 4). Os apps escutam via
    /// `Subscription` e re-renderizam o anel mais forte. Sem corpo.
    #[zbus(signal)]
    async fn focus_highlight_changed(
        emitter: &SignalEmitter<'_>,
        enabled: bool,
    ) -> zbus::Result<()>;

    /// Id do **acento curado** escolhido (modelo Cinnamon).
    async fn accent(&self) -> String {
        match self.notifications.lock() {
            Ok(g) => g.accent.clone(),
            Err(_) => "amber".to_string(),
        }
    }

    /// Escolhe o **acento curado** (passo "acento curado"). Memória + disco +
    /// **emite o sinal** pros apps re-resolverem o token de accent ao vivo. O
    /// compositor só guarda/serve/notifica — quem pinta é o app. Aceita
    /// qualquer id; o app valida (id desconhecido → âmbar).
    async fn set_accent(
        &self,
        accent: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        if let Ok(mut g) = self.notifications.lock() {
            g.accent = accent.to_string();
            info!("D-Bus: acento = {accent}");
        } else {
            error!("D-Bus: lock envenenado ao setar acento");
        }
        persist_accessibility(&self.notifications);
        if let Err(err) = emitter.accent_changed(accent).await {
            error!("D-Bus: falha ao emitir sinal de acento: {err}");
        }
    }

    /// **Sinal de mudança** do acento. Os apps escutam via `Subscription` e
    /// re-resolvem o token de accent. Sem corpo.
    #[zbus(signal)]
    async fn accent_changed(emitter: &SignalEmitter<'_>, accent: &str) -> zbus::Result<()>;

    // ─── Janelas / taskbar (luma-bar passo 2) ─────────────────────────

    /// **Sinal de mudança** da lista de janelas — abrir, fechar, focar,
    /// minimizar ou trocar de título. Sem args: a barra relê `list_windows`
    /// (mesmo contrato do `workspace_changed`). Emitido só na borda, via
    /// fingerprint do lado do compositor — não por frame.
    #[zbus(signal)]
    async fn windows_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Lista as janelas abertas pra a taskbar: `(id, título, app_id, focada)`.
    /// O compositor mantém isso atualizado por frame no slot compartilhado; a
    /// luma-bar relê no `windows_changed`. `activate_window` foca pela id.
    async fn list_windows(&self) -> Vec<(u64, String, String, bool, bool)> {
        match self.windows.lock() {
            Ok(g) => g
                .iter()
                .map(|w| {
                    (
                        w.id,
                        w.title.clone(),
                        w.app_id.clone(),
                        w.focused,
                        w.minimized,
                    )
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Ativa (foca + levanta, e desminimiza se preciso) a janela. Id
    /// desconhecida = no-op no compositor.
    async fn activate_window(&self, id: u64) {
        if self.cmd_tx.send(IpcCommand::ActivateWindow(id)).is_err() {
            error!("D-Bus: calloop fechado ao ativar janela {id}");
        }
    }

    /// Minimiza a janela (luma-bar passo 7).
    async fn minimize_window(&self, id: u64) {
        if self.cmd_tx.send(IpcCommand::MinimizeWindow(id)).is_err() {
            error!("D-Bus: calloop fechado ao minimizar janela {id}");
        }
    }

    /// Fecha a janela (luma-bar passo 7).
    async fn close_window(&self, id: u64) {
        if self.cmd_tx.send(IpcCommand::CloseWindow(id)).is_err() {
            error!("D-Bus: calloop fechado ao fechar janela {id}");
        }
    }

    /// Mostrar área de trabalho — toggle com memória (luma-bar passo 7).
    async fn show_desktop(&self) {
        if self.cmd_tx.send(IpcCommand::ShowDesktop).is_err() {
            error!("D-Bus: calloop fechado ao mostrar área de trabalho");
        }
    }

    /// O modo "mostrar área de trabalho" está ligado? A zona F da barra usa
    /// pra acender o botão em âmbar e trocar pro ícone `-active`. Lê o último
    /// valor do `watch` (o estado vive em `LumaState::desktop_visible`).
    async fn desktop_visible(&self) -> bool {
        *self.desktop_visible.borrow()
    }

    /// **Sinal**: o show-desktop foi ligado ou desligado — pelo botão da barra,
    /// pelo Super+D, ou pelo Ctrl+Alt+D. Todos passam por `toggle_show_desktop`.
    #[zbus(signal)]
    async fn desktop_visible_changed(
        emitter: &SignalEmitter<'_>,
        visible: bool,
    ) -> zbus::Result<()>;

    // ─── Bateria / applet (luma-bar passo 3) ──────────────────────────

    /// Estado atual da bateria: `(present, capacity 0–100, charging)`. Lê o
    /// último valor do `watch`; a barra lê no boot e depois escuta o sinal.
    async fn battery(&self) -> (bool, u8, bool) {
        *self.battery.borrow()
    }

    /// **Sinal de mudança** da bateria (por sinal, sem poll — o estado muda
    /// pouco). Emitido pelo loop do servidor quando o `watch` muda.
    #[zbus(signal)]
    async fn battery_changed(
        emitter: &SignalEmitter<'_>,
        present: bool,
        capacity: u8,
        charging: bool,
    ) -> zbus::Result<()>;

    // ─── Volume / applet (luma-bar passo 4) ───────────────────────────

    /// Estado atual do volume: `(level 0–100, muted)`. A barra lê no boot e
    /// depois escuta o sinal.
    async fn volume(&self) -> (u8, bool) {
        *self.volume.borrow()
    }

    /// Seta o volume do default sink (0–150%). Mão dupla: a barra manda no
    /// scroll. Fire-and-forget pro calloop; o sinal reflete o novo estado.
    async fn set_volume(&self, level: u8) {
        if self.cmd_tx.send(IpcCommand::SetVolume(level)).is_err() {
            error!("D-Bus: calloop fechado ao setar volume");
        }
    }

    /// Alterna mudo do default sink. A barra manda no clique.
    async fn toggle_mute(&self) {
        if self.cmd_tx.send(IpcCommand::ToggleMute).is_err() {
            error!("D-Bus: calloop fechado ao alternar mudo");
        }
    }

    /// **Sinal de mudança** do volume — emitido pelo loop quando o `watch`
    /// muda (tecla, set, ou mudança externa pega no refresh).
    #[zbus(signal)]
    async fn volume_changed(
        emitter: &SignalEmitter<'_>,
        level: u8,
        muted: bool,
    ) -> zbus::Result<()>;

    // ─── Mixer de áudio (popup do volume) ─────────────────────────────

    /// Lista os dispositivos de áudio (saídas + entradas) com volume/mudo de
    /// cada um, pro popup do mixer da luma-bar. Cada item:
    /// `(id, nome, volume% 0–150, mudo, é_entrada, é_default)`. Lê via wpctl.
    async fn audio_devices(&self) -> Vec<(u32, String, u32, bool, bool, bool)> {
        let (sinks, sources) = crate::audio::list_devices();
        let mut out = Vec::with_capacity(sinks.len() + sources.len());
        for d in sinks {
            out.push((
                d.id,
                d.description,
                (d.volume * 100.0).round() as u32,
                d.muted,
                false,
                d.is_default,
            ));
        }
        for d in sources {
            out.push((
                d.id,
                d.description,
                (d.volume * 100.0).round() as u32,
                d.muted,
                true,
                d.is_default,
            ));
        }
        out
    }

    /// Define o volume (0–150%) de um dispositivo pela id (mixer).
    async fn set_device_volume(&self, id: u32, percent: u32) {
        crate::audio::set_volume(id, percent.min(150) as u8);
    }

    /// Alterna o mudo de um dispositivo pela id (mixer).
    async fn set_device_mute(&self, id: u32) {
        crate::audio::toggle_mute(id);
    }

    // ─── Rede / applet (luma-bar passo 5) ─────────────────────────────

    /// Nome do ícone do estado de rede atual ("ethernet-on", "wifi-full",
    /// "wifi-low", "wifi-none", "ethernet-off", "wifi-off"). A barra mapeia
    /// pro SVG. Lê no boot + escuta o sinal.
    async fn network(&self) -> String {
        self.network.borrow().clone()
    }

    /// **Sinal de mudança** da rede — emitido quando o `watch` muda (refresh
    /// do tray pega conectar/desconectar/trocar).
    #[zbus(signal)]
    async fn network_changed(emitter: &SignalEmitter<'_>, icon: &str) -> zbus::Result<()>;

    // ─── Bluetooth / applet ───────────────────────────────────────────

    /// Nome do ícone do estado de bluetooth ("bluetooth-on", "bluetooth-off",
    /// ou "" se não há controller). A barra mapeia pro SVG. Mesmo molde da
    /// rede: lê no boot + escuta o sinal.
    async fn bluetooth(&self) -> String {
        self.bluetooth.borrow().clone()
    }

    /// **Sinal de mudança** do bluetooth — emitido quando o `watch` muda.
    #[zbus(signal)]
    async fn bluetooth_changed(emitter: &SignalEmitter<'_>, icon: &str) -> zbus::Result<()>;

    /// Estado do bluetooth pro popup: `(ligado, tem_hardware, dispositivos)`,
    /// cada dispositivo `(mac, nome, pareado, conectado)`. Lê via BlueZ
    /// (bluetoothctl). Lista os pareados quando ligado.
    async fn bluetooth_state(&self) -> (bool, bool, Vec<(String, String, bool, bool)>) {
        let st = crate::bluetooth::read_state(false);
        let devices = st
            .devices
            .into_iter()
            .map(|d| (d.mac, d.name, d.paired, d.connected))
            .collect();
        (st.enabled, st.has_hardware, devices)
    }

    /// Liga/desliga o controller de bluetooth (toggle do popup).
    async fn set_bluetooth_power(&self, on: bool) {
        crate::bluetooth::set_enabled(on);
    }

    /// Conecta um dispositivo pareado pela MAC.
    async fn connect_device(&self, mac: String) {
        crate::bluetooth::connect(&mac);
    }

    /// Desconecta um dispositivo pela MAC.
    async fn disconnect_device(&self, mac: String) {
        crate::bluetooth::disconnect(&mac);
    }

    // ─── Pareamento (refino) ──────────────────────────────────────────

    /// Começa a varredura (modo "adicionar dispositivo"). Os achados aparecem
    /// no `scan_devices` enquanto procura. Usa a conexão persistente do agent.
    async fn start_scan(&self) {
        if let Some(conn) = &self.bt_system {
            if let Err(e) = crate::bluetooth::set_discovery(conn, true).await {
                warn!("D-Bus: StartDiscovery falhou: {e}");
            }
        }
    }

    /// Para a varredura (sai do modo / fecha o popup).
    async fn stop_scan(&self) {
        if let Some(conn) = &self.bt_system {
            if let Err(e) = crate::bluetooth::set_discovery(conn, false).await {
                warn!("D-Bus: StopDiscovery falhou: {e}");
            }
        }
    }

    /// Pareia um device pela MAC (via Device1.Pair + nosso agent pro código).
    /// Acompanhe o progresso por `PairingState`.
    async fn pair_device(&self, mac: String) {
        crate::bluetooth::pair(&mac, self.pairing.clone());
    }

    /// Responde o pedido de confirmação de código (confirma/cancela).
    async fn confirm_pairing(&self, ok: bool) {
        crate::bt_agent::answer(&self.pairing, ok);
    }

    /// Estado do pareamento: `(fase, passkey, mensagem)`. Fase: 0 ocioso,
    /// 1 pareando, 2 confere o código, 3 deu certo, 4 falhou.
    async fn pairing_state(&self) -> (u8, u32, String) {
        crate::bt_agent::phase_tuple(&self.pairing)
    }

    /// Dispositivos do modo varredura: pareados + descobertos `(mac, nome,
    /// pareado, conectado)`. Cresce ao vivo enquanto o scan roda.
    async fn scan_devices(&self) -> Vec<(String, String, bool, bool)> {
        crate::bluetooth::read_state(true)
            .devices
            .into_iter()
            .map(|d| (d.mac, d.name, d.paired, d.connected))
            .collect()
    }

    /// Limpa a fase de pareamento de volta pra ociosa (ao sair do modo).
    async fn reset_pairing(&self) {
        crate::bt_agent::reset(&self.pairing);
    }

    // ─── Rede: popup (conexões) ───────────────────────────────────────

    /// Estado de rede pro popup: `(wifi_ligado, tem_placa_wifi, device_cabeado
    /// ou "", [(ssid, sinal 0–100, protegida, conectada)])`. Lê via nmcli.
    async fn network_devices(&self) -> (bool, bool, String, Vec<(String, u8, bool, bool)>) {
        let st = crate::wifi::read_state();
        let eth = st.ethernet.map(|e| e.device).unwrap_or_default();
        let nets = st
            .networks
            .into_iter()
            .map(|n| {
                let protected = n.is_protected();
                (n.ssid, n.signal, protected, n.in_use)
            })
            .collect();
        (st.enabled, st.has_hardware, eth, nets)
    }

    /// Conecta numa rede wi-fi pelo SSID (rede aberta ou com perfil salvo;
    /// senha de rede nova é refino). Fire-and-forget; relê depois.
    async fn connect_wifi(&self, ssid: String) {
        let outbox = std::sync::Arc::new(std::sync::Mutex::new(None));
        crate::wifi::connect_async(outbox, ssid, None);
    }

    /// Liga/desliga o rádio wi-fi.
    async fn set_wifi_radio(&self, on: bool) {
        crate::wifi::set_enabled(on);
    }

    /// Pede um rescan das redes (ao abrir o popup).
    async fn wifi_rescan(&self) {
        crate::wifi::request_scan();
    }

    // ─── Notificações: sino + central ─────────────────────────────────

    /// Estado pro sino: `(ícone, contagem, dnd, som)`. Ícone: "bell-dnd" se
    /// DND, "bell-alert" se há notificação/toast, senão "bell-normal".
    async fn notification_status(&self) -> (String, u32, bool, bool) {
        match self.notifications.lock() {
            Ok(g) => {
                let count = g.history.len() as u32;
                let icon = if g.dnd_enabled {
                    "bell-dnd"
                } else if count > 0 || !g.visible.is_empty() {
                    "bell-alert"
                } else {
                    "bell-normal"
                };
                (icon.to_string(), count, g.dnd_enabled, g.sound_enabled)
            }
            Err(_) => ("bell-normal".to_string(), 0, false, false),
        }
    }

    /// Lista da central (histórico), mais recente primeiro: `(id, app,
    /// resumo, corpo)`.
    async fn notifications(&self) -> Vec<(u32, String, String, String)> {
        match self.notifications.lock() {
            Ok(g) => g
                .history
                .iter()
                .rev()
                .map(|h| (h.id, h.app_name.clone(), h.summary.clone(), h.body.clone()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Alterna o Não Perturbe.
    async fn toggle_dnd(&self) {
        if let Ok(mut g) = self.notifications.lock() {
            crate::notifications::toggle_dnd(&mut g);
        }
    }

    /// Alterna o som das notificações.
    async fn toggle_notification_sound(&self) {
        if let Ok(mut g) = self.notifications.lock() {
            crate::notifications::toggle_sound(&mut g);
        }
    }

    /// Limpa a central (histórico).
    async fn clear_notifications(&self) {
        if let Ok(mut g) = self.notifications.lock() {
            g.history.clear();
        }
    }

    // ─── Sessão (menu passo 3) ────────────────────────────────────────

    /// Desliga a máquina (via systemctl → logind/polkit). Irreversível — o
    /// menu confirma antes.
    async fn power_off(&self) {
        session_action("poweroff");
    }

    /// Reinicia a máquina. Irreversível — o menu confirma antes.
    async fn reboot(&self) {
        session_action("reboot");
    }

    /// Suspende a máquina (reversível).
    async fn suspend(&self) {
        session_action("suspend");
    }

    /// Bloqueia a sessão (a tela de lock do Luma). Via calloop (mexe no state).
    async fn lock(&self) {
        if self.cmd_tx.send(IpcCommand::Lock).is_err() {
            error!("D-Bus: calloop fechado ao bloquear");
        }
    }

    // ─── Bandeja (SNI host) ───────────────────────────────────────────

    /// Itens da bandeja pra barra: `(service, object_path, título, tooltip,
    /// caminho_do_ícone, menu_path)`. O ícone já vem resolvido pra um arquivo
    /// (nome temático ou pixmap rasterizado); "" = sem ícone. `menu_path` é o
    /// objeto DBusMenu do item (vazio = sem menu de clique-direito).
    async fn tray_items(&self) -> Vec<(String, String, String, String, String, String)> {
        match self.sni.lock() {
            Ok(g) => g
                .iter()
                .map(|it| {
                    (
                        it.service.clone(),
                        it.object_path.clone(),
                        it.title.clone(),
                        it.tooltip.clone(),
                        crate::sni::item_icon_path(it),
                        it.menu_path.clone(),
                    )
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Clique esquerdo num item da bandeja → manda `Activate` pro app.
    async fn tray_activate(&self, service: String, path: String) {
        crate::sni::activate(&service, &path, 0, 0);
    }

    /// Árvore (flat) do menu de clique-direito de um item: `(id, label,
    /// habilitado, separador)`. Chama AboutToShow antes (labels dinâmicos).
    /// Roda o dbusmenu (zbus blocking) num thread de blocking pra não travar
    /// a runtime async.
    async fn tray_menu(&self, service: String, menu_path: String) -> Vec<(i32, String, bool, bool)> {
        let items = tokio::task::spawn_blocking(move || {
            crate::sni::about_to_show(&service, &menu_path);
            crate::sni::get_menu_layout(&service, &menu_path)
        })
        .await
        .unwrap_or_default();
        items
            .into_iter()
            .map(|it| (it.id, it.label, it.enabled, it.separator))
            .collect()
    }

    /// Clique num item do menu → manda `Event(clicked)` pro app (o app age).
    async fn tray_menu_event(&self, service: String, menu_path: String, id: i32) {
        crate::sni::send_menu_event(&service, &menu_path, id);
    }

    // ─── Menu (luma-menu passo 1) ─────────────────────────────────────

    /// Pede o toggle do menu (luma-menu). Emite `menu_toggle`; o luma-menu
    /// abre/fecha a surface overlay. Gatilho atual: busctl; no passo 5 vira o
    /// Super. Stateless (o menu guarda aberto/fechado).
    async fn show_menu(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        if let Err(err) = emitter.menu_toggle().await {
            error!("D-Bus: falha ao emitir menu_toggle: {err}");
        }
    }

    /// **Sinal** de toggle do menu (luma-menu passo 1).
    #[zbus(signal)]
    async fn menu_toggle(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// O cliente avisa que fechou o context menu do desktop — libera o
    /// clique-direito a voltar ao comportamento normal (não reabre por cima).
    async fn desktop_menu_closed(&self) {
        crate::state::DESKTOP_MENU_OPEN.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    // ─── Ações do context menu do desktop (cliente → compositor) ──────

    /// Organiza os ícones do desktop: 0=nome, 1=tipo, 2=data.
    async fn arrange_icons(&self, kind: u8) {
        use crate::context_menu::{ArrangeKind, ContextAction};
        let k = match kind {
            1 => ArrangeKind::ByKind,
            2 => ArrangeKind::ByDate,
            _ => ArrangeKind::ByName,
        };
        let _ = self.cmd_tx.send(IpcCommand::CtxAction(ContextAction::ArrangeIcons(k)));
    }

    /// Cola o clipboard no desktop.
    async fn paste_desktop(&self) {
        let _ = self
            .cmd_tx
            .send(IpcCommand::CtxAction(crate::context_menu::ContextAction::ClipboardPaste));
    }

    /// Cria no desktop: 0=pasta, 1=arquivo de texto, 2=documento markdown.
    async fn create_new(&self, kind: u8) {
        use crate::context_menu::{ContextAction, NewKind};
        let k = match kind {
            1 => NewKind::TextFile,
            2 => NewKind::Markdown,
            _ => NewKind::Folder,
        };
        let _ = self.cmd_tx.send(IpcCommand::CtxAction(ContextAction::CreateNew(k)));
    }

    /// Abre o seletor de papel de parede (troca o wallpaper).
    async fn change_wallpaper(&self) {
        let _ = self.cmd_tx.send(IpcCommand::CtxAction(
            crate::context_menu::ContextAction::ChangeWallpaperPick,
        ));
    }

    /// **Sinal** do context menu do desktop: clique-direito no fundo. Passa a
    /// posição do clique `(x, y)` e o tamanho do output `(out_w, out_h)` pro
    /// cliente ancorar no ponto + proteger a borda.
    #[zbus(signal)]
    async fn desktop_menu(
        emitter: &SignalEmitter<'_>,
        x: i32,
        y: i32,
        out_w: i32,
        out_h: i32,
    ) -> zbus::Result<()>;

    // ─── Workspaces / motor (áreas de trabalho) ──────────────────────

    /// Estado do motor de workspace: `(atual, quantas, [ [ids_da_área_0], … ])`.
    /// 0-based. O compositor mantém o slot atualizado por frame; o cliente
    /// (central/pager) faz poll + escuta `workspace_changed`.
    async fn workspace_state(&self) -> (u32, u32, Vec<Vec<u64>>) {
        match self.workspaces.lock() {
            Ok(g) => (g.current, g.count, g.windows.clone()),
            Err(_) => (0, 1, vec![vec![]]),
        }
    }

    /// Vai pra área de índice N (0-based). Auto-cria as intermediárias se N
    /// estiver além do fim. Fire-and-forget; `workspace_changed` reflete.
    async fn workspace_switch(&self, index: u32) {
        if self
            .cmd_tx
            .send(IpcCommand::WorkspaceSwitch(index as usize))
            .is_err()
        {
            error!("D-Bus: calloop fechado ao trocar de workspace");
        }
    }

    /// Cria uma área nova vazia e vai pra ela.
    async fn workspace_create(&self) {
        if self.cmd_tx.send(IpcCommand::WorkspaceCreate).is_err() {
            error!("D-Bus: calloop fechado ao criar workspace");
        }
    }

    /// Move a janela (pela id) pra área de índice `target` (0-based). Cria as
    /// intermediárias se preciso.
    async fn workspace_move_window(&self, window_id: u64, target: u32) {
        if self
            .cmd_tx
            .send(IpcCommand::WorkspaceMoveWindow {
                window_id,
                target: target as usize,
            })
            .is_err()
        {
            error!("D-Bus: calloop fechado ao mover janela de workspace");
        }
    }

    /// **Sinal de mudança** do motor de workspace — emitido quando a área atual
    /// ou o conjunto muda (troca/criar/mover/fechar janela). Carrega `(atual,
    /// quantas)`; o cliente relê `workspace_state` pros detalhes.
    #[zbus(signal)]
    async fn workspace_changed(
        emitter: &SignalEmitter<'_>,
        current: u32,
        count: u32,
    ) -> zbus::Result<()>;

    // ─── Alvos de toque (parte 2) ─────────────────────────────────────

    /// "Alvos de toque maiores": on usa `min_target_size`, off usa baseline.
    async fn larger_targets(&self) -> bool {
        match self.notifications.lock() {
            Ok(g) => g.larger_targets,
            Err(_) => false,
        }
    }

    /// Tamanho do alvo (px) quando `larger_targets` está on.
    async fn min_target_size(&self) -> u16 {
        match self.notifications.lock() {
            Ok(g) => g.min_target_size,
            Err(_) => 48,
        }
    }

    /// Alvo de toque **efetivo** (px) — o número final que os apps aplicam.
    /// `larger_targets ? min_target_size : 44`. Read-only (computado).
    async fn hit_target_min(&self) -> u16 {
        match self.notifications.lock() {
            Ok(g) => g.effective_hit_target(),
            Err(_) => crate::notifications::HIT_TARGET_BASELINE,
        }
    }

    /// Liga/desliga "alvos maiores" (parte 2). Memória + disco + emite
    /// `targets_changed(efetivo)` pros apps re-dimensionarem ao vivo.
    async fn set_larger_targets(
        &self,
        enabled: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        if let Ok(mut g) = self.notifications.lock() {
            g.larger_targets = enabled;
            info!("D-Bus: alvos maiores = {enabled}");
        } else {
            error!("D-Bus: lock envenenado ao setar alvos maiores");
        }
        persist_accessibility(&self.notifications);
        self.emit_targets_changed(&emitter).await;
    }

    /// Seta o tamanho do alvo (parte 2). Clampa em 44–72. Memória + disco +
    /// emite `targets_changed(efetivo)` (só muda o efetivo se larger on).
    async fn set_min_target_size(
        &self,
        size: u16,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let clamped = size.clamp(crate::notifications::HIT_TARGET_BASELINE, 72);
        if let Ok(mut g) = self.notifications.lock() {
            g.min_target_size = clamped;
            info!("D-Bus: tamanho do alvo = {clamped}px");
        } else {
            error!("D-Bus: lock envenenado ao setar tamanho do alvo");
        }
        persist_accessibility(&self.notifications);
        self.emit_targets_changed(&emitter).await;
    }

    /// **Sinal** de mudança dos alvos de toque (parte 2). Carrega o alvo
    /// EFETIVO (px) — os apps fazem `set_hit_target_min` e re-layoutam.
    #[zbus(signal)]
    async fn targets_changed(
        emitter: &SignalEmitter<'_>,
        effective: u16,
    ) -> zbus::Result<()>;

    /// Gatilho de teste do sino visual (3a): injeta uma notificação fake,
    /// que passa pelo caminho normal de alerta. Se o sino visual estiver
    /// ligado, a tela pisca. Existe porque, rodando aninhado sob um DE dono
    /// das notificações (ex.: Plasma), `notify-send` não chega no Luma —
    /// sem isso não dá pra ver a piscada ao vivo. Dispara por busctl.
    async fn emit_test_bell(&self) {
        crate::notifications::inject_test_notification(&self.notifications);
        info!("D-Bus: EmitTestBell — notificação de teste injetada");
    }
}

impl CompositorService {
    /// Emite `targets_changed` com o alvo de toque EFETIVO atual. Helper
    /// interno (fora da interface D-Bus), chamado pelos setters de alvos.
    /// Alvos parte 2.
    async fn emit_targets_changed(&self, emitter: &SignalEmitter<'_>) {
        let effective = self
            .notifications
            .lock()
            .map(|g| g.effective_hit_target())
            .unwrap_or(crate::notifications::HIT_TARGET_BASELINE);
        if let Err(err) = emitter.targets_changed(effective).await {
            error!("D-Bus: falha ao emitir targets_changed: {err}");
        }
    }
}

/// Persiste a seção `[accessibility]` a partir da MEMÓRIA (o slot é a fonte
/// de verdade). Escrita atômica via `settings::save`. Falha de I/O loga e
/// segue — a memória já mudou. NÃO segura o lock ao salvar (lock liberado
/// antes do I/O), pra não travar quem lê o slot no render. 3b/3c.
fn persist_accessibility(notifications: &NotificationsSlot) {
    let (accessibility, appearance) = match notifications.lock() {
        Ok(g) => (
            crate::settings::Accessibility {
                visual_bell_enabled: g.flash_enabled,
                photosafe_enabled: g.flash_photosafe,
                flash_intensity: g.flash_intensity,
                reduce_motion: g.reduce_motion,
                high_contrast: g.high_contrast,
                larger_targets: g.larger_targets,
                min_target_size: g.min_target_size,
                focus_highlight: g.focus_highlight,
            },
            crate::settings::Appearance {
                accent: g.accent.clone(),
            },
        ),
        Err(_) => {
            error!("D-Bus: lock envenenado ao persistir settings");
            return;
        }
    };
    let settings = crate::settings::Settings {
        accessibility,
        appearance,
    };
    if let Err(err) = crate::settings::save(&settings) {
        error!("D-Bus: falha ao persistir settings de acessibilidade: {err}");
    }
}

/// Sobe o servidor numa thread dedicada. Não-bloqueante. `available`
/// é uma cópia estática do catálogo do registry (3c.1) — clonada do
/// loop principal antes de chamar. `notifications` é o slot
/// compartilhado (clone do `Arc`) pro sino visual (3a).
pub fn spawn(
    cmd_tx: Sender<IpcCommand>,
    available: Vec<WidgetInfo>,
    notifications: NotificationsSlot,
    windows: WindowsSlot,
    sni: crate::sni::SniList,
    workspaces: WorkspaceSlot,
    signals: SignalSources,
) {
    let builder = thread::Builder::new().name("luma-dbus".into());
    if let Err(err) = builder.spawn(move || {
        run(
            cmd_tx,
            available,
            notifications,
            windows,
            sni,
            workspaces,
            signals,
        )
    }) {
        error!("D-Bus: não foi possível criar thread do servidor: {err}");
    }
}

fn run(
    cmd_tx: Sender<IpcCommand>,
    available: Vec<WidgetInfo>,
    notifications: NotificationsSlot,
    windows: WindowsSlot,
    sni: crate::sni::SniList,
    workspaces: WorkspaceSlot,
    signals: SignalSources,
) {
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
        if let Err(err) = serve(
            cmd_tx,
            available,
            notifications,
            windows,
            sni,
            workspaces,
            signals,
        )
        .await
        {
            error!("D-Bus: servidor encerrou com erro: {err}");
        }
    });
}

/// Dispara uma ação de sessão via `systemctl` (poweroff/reboot/suspend) —
/// trata polkit/logind. Num thread que reapeia (suspend volta; os outros
/// derrubam a sessão de qualquer jeito).
fn session_action(verb: &str) {
    let verb = verb.to_string();
    std::thread::spawn(move || {
        let _ = std::process::Command::new("systemctl").arg(&verb).status();
    });
}

/// Registra o nosso Agent1 no system bus como default agent do BlueZ. Devolve
/// a conexão (precisa viver enquanto o agent estiver registrado).
async fn register_bt_agent(
    pairing: crate::bt_agent::PairingSlot,
) -> zbus::Result<zbus::Connection> {
    use zbus::zvariant::ObjectPath;
    const AGENT_PATH: &str = "/dev/luma/btagent";
    let agent = crate::bt_agent::Agent { slot: pairing };
    let conn = zbus::connection::Builder::system()?
        .serve_at(AGENT_PATH, agent)?
        .build()
        .await?;
    let mgr = zbus::Proxy::new(&conn, "org.bluez", "/org/bluez", "org.bluez.AgentManager1").await?;
    let path = ObjectPath::try_from(AGENT_PATH)?;
    // DisplayYesNo: cobre RequestConfirmation (código numérico) + display.
    mgr.call::<_, _, ()>("RegisterAgent", &(&path, "DisplayYesNo")).await?;
    mgr.call::<_, _, ()>("RequestDefaultAgent", &(&path,)).await?;
    info!("D-Bus: agent BlueZ registrado em {AGENT_PATH}");
    Ok(conn)
}

async fn serve(
    cmd_tx: Sender<IpcCommand>,
    available: Vec<WidgetInfo>,
    notifications: NotificationsSlot,
    windows: WindowsSlot,
    sni: crate::sni::SniList,
    workspaces: WorkspaceSlot,
    signals: SignalSources,
) -> zbus::Result<()> {
    let SignalSources {
        battery,
        volume,
        network,
        bluetooth,
        menu_toggle,
        desktop_menu,
        workspace_changed,
        windows_changed,
        desktop_visible,
    } = signals;
    // Estado de pareamento compartilhado: o agent (system bus) escreve a fase
    // e guarda o oneshot da confirmação; o ConfirmPairing (session bus) lê.
    let pairing: crate::bt_agent::PairingSlot = Default::default();

    // Registra o agent BlueZ no SYSTEM bus (pro código de pareamento) ANTES de
    // montar o service — a conexão persistente entra no service pro scan usar a
    // MESMA conexão (BlueZ amarra discovery ao sender). Drop = desregistra.
    let bt_system = match register_bt_agent(pairing.clone()).await {
        Ok(c) => Some(c),
        Err(err) => {
            warn!("D-Bus: agent BlueZ não registrado: {err}");
            None
        }
    };

    // Clone do slot pro loop de sinais (o service consome o original).
    let workspaces_signal = workspaces.clone();
    let service = CompositorService {
        available_widgets: available,
        cmd_tx,
        notifications,
        windows,
        battery: battery.clone(),
        volume: volume.clone(),
        network: network.clone(),
        bluetooth: bluetooth.clone(),
        pairing: pairing.clone(),
        bt_system: bt_system.clone(),
        sni,
        workspaces,
        desktop_visible: desktop_visible.clone(),
    };
    let conn = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, service)?
        .build()
        .await?;
    info!("D-Bus: {BUS_NAME} registrado no session bus em {OBJECT_PATH}");
    // Segura a conexão do system bus viva enquanto o serve roda.
    let _bt_agent_conn = bt_system;

    // Loop dos sinais de sensores (bateria passo 3 + volume passo 4): emite
    // o sinal certo quando o `watch` respectivo muda. Mantém a conexão viva
    // (os `changed()` pendem enquanto o compositor — dono dos senders — vive).
    let emitter = SignalEmitter::new(&conn, OBJECT_PATH)?;
    let mut battery = battery;
    let mut volume = volume;
    let mut network = network;
    let mut bluetooth = bluetooth;
    let mut menu_toggle = menu_toggle;
    let mut desktop_menu = desktop_menu;
    let mut workspace_changed = workspace_changed;
    let mut windows_changed = windows_changed;
    let mut desktop_visible = desktop_visible;
    loop {
        tokio::select! {
            r = battery.changed() => {
                if r.is_err() { break; }
                let (present, capacity, charging) = *battery.borrow();
                if let Err(err) = emitter.battery_changed(present, capacity, charging).await {
                    error!("D-Bus: falha ao emitir sinal de bateria: {err}");
                }
            }
            r = volume.changed() => {
                if r.is_err() { break; }
                let (level, muted) = *volume.borrow();
                if let Err(err) = emitter.volume_changed(level, muted).await {
                    error!("D-Bus: falha ao emitir sinal de volume: {err}");
                }
            }
            r = network.changed() => {
                if r.is_err() { break; }
                let icon = network.borrow().clone();
                if let Err(err) = emitter.network_changed(&icon).await {
                    error!("D-Bus: falha ao emitir sinal de rede: {err}");
                }
            }
            r = bluetooth.changed() => {
                if r.is_err() { break; }
                let icon = bluetooth.borrow().clone();
                if let Err(err) = emitter.bluetooth_changed(&icon).await {
                    error!("D-Bus: falha ao emitir sinal de bluetooth: {err}");
                }
            }
            r = menu_toggle.changed() => {
                if r.is_err() { break; }
                // Super pediu o menu → emite o mesmo sinal do ShowMenu.
                if let Err(err) = emitter.menu_toggle().await {
                    error!("D-Bus: falha ao emitir menu_toggle (Super): {err}");
                }
            }
            r = desktop_menu.changed() => {
                if r.is_err() { break; }
                let (_seq, x, y, w, h) = *desktop_menu.borrow();
                if let Err(err) = emitter.desktop_menu(x, y, w, h).await {
                    error!("D-Bus: falha ao emitir desktop_menu: {err}");
                }
            }
            r = workspace_changed.changed() => {
                if r.is_err() { break; }
                // Motor de workspace mudou (troca/criar/mover/fechar). Lê o
                // snapshot fresco e emite (atual, quantas).
                let (current, count) = match workspaces_signal.lock() {
                    Ok(g) => (g.current, g.count),
                    Err(_) => (0, 1),
                };
                if let Err(err) = emitter.workspace_changed(current, count).await {
                    error!("D-Bus: falha ao emitir workspace_changed: {err}");
                }
            }
            r = windows_changed.changed() => {
                if r.is_err() { break; }
                // Lista de janelas mudou (abrir/fechar/focar/minimizar/título).
                // Sem args — a barra relê `list_windows` do slot.
                if let Err(err) = emitter.windows_changed().await {
                    error!("D-Bus: falha ao emitir windows_changed: {err}");
                }
            }
            r = desktop_visible.changed() => {
                if r.is_err() { break; }
                let visible = *desktop_visible.borrow();
                if let Err(err) = emitter.desktop_visible_changed(visible).await {
                    error!("D-Bus: falha ao emitir desktop_visible_changed: {err}");
                }
            }
        }
    }
    // Sender dropado (compositor saindo) — segura a conexão pra não derrubar
    // o nome/clientes durante o shutdown.
    std::future::pending::<()>().await;
    Ok(())
}
