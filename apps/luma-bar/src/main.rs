//! `luma-bar` — a barra do Luma como **cliente layer-shell** próprio (igual o
//! `luma-config`), no modelo limpo: libluma (Golden Hour) + o canal D-Bus, daí
//! acento/HC de graça. Passo 1 (mínimo): ancora na borda com zona exclusiva,
//! renderiza a moldura + relógio, e segue o acento/HC ao vivo. Sem taskbar/
//! applets ainda; coexiste com a barra velha do compositor pra comparar.

mod ipc;

use std::time::Duration;

use iced::widget::{
    column, container, image, mouse_area, row, scrollable, stack, svg, text, tooltip, Space,
};
use iced::{Background, Border, Color, Element, Length, Padding, Task};
use iced_layershell::reexport::{
    Anchor, IcedId, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

/// Altura do conteúdo da barra (px).
const BAR_HEIGHT: u32 = 36;
/// Espessura da linha de acento na base da barra.
const ACCENT_LINE: u32 = 2;
/// Máximo de chars do título no botão (trunca com reticências).
const TITLE_MAX: usize = 22;

/// Uma janela na taskbar (passo 2): id pra ativar, label resolvido, foco.
#[derive(Debug, Clone)]
struct WinEntry {
    id: u64,
    label: String,
    focused: bool,
    minimized: bool,
}

/// Qual popup está aberto (um de cada vez).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopupKind {
    Volume,
    Bluetooth,
    Network,
    Notifications,
    TrayOverflow,
}

/// Um item da bandeja (SNI).
#[derive(Debug, Clone)]
struct TrayItem {
    service: String,
    path: String,
    tooltip: String,
    /// Caminho do ícone já resolvido pelo compositor (svg/png), ou "".
    icon_path: String,
}

/// Uma notificação na central.
#[derive(Debug, Clone)]
struct Notif {
    #[allow(dead_code)]
    id: u32,
    app: String,
    summary: String,
    body: String,
}

/// Uma rede wi-fi no popup de rede.
#[derive(Debug, Clone)]
struct WifiNet {
    ssid: String,
    /// Sinal 0–100.
    signal: u8,
    protected: bool,
    in_use: bool,
}

/// Um dispositivo bluetooth no popup.
#[derive(Debug, Clone)]
struct BtDev {
    mac: String,
    name: String,
    #[allow(dead_code)]
    paired: bool,
    connected: bool,
}

/// Um dispositivo de áudio no mixer do popup de volume.
#[derive(Debug, Clone)]
struct AudioDev {
    id: u32,
    name: String,
    /// Volume 0–150 (%). Ajustado ao vivo pelo slider; commita no soltar.
    volume: u32,
    muted: bool,
    is_source: bool,
    is_default: bool,
}

struct Bar {
    /// Hora atual "HH:MM".
    clock: String,
    /// Data curta pt-BR ("seg · 22 jun"). Atualiza junto no tick.
    date: String,
    /// Janelas abertas (poll de `list_windows` a cada 500ms).
    windows: Vec<WinEntry>,
    /// Bateria: `Some((capacity, charging))`, ou `None` se não há bateria.
    /// Lida no boot + atualizada pelo sinal (sem poll — passo 3).
    battery: Option<(u8, bool)>,
    /// Volume: `(level 0–100, muted)`. Lido no boot + sinal; mexido por
    /// scroll/clique com update otimista (passo 4).
    volume: (u8, bool),
    /// Rede: nome do ícone do estado ("ethernet-on", "wifi-full"…). Vazio
    /// até o boot ler. Lido no boot + sinal (passo 5).
    network: String,
    /// Bluetooth: nome do ícone ("bluetooth-on"/"bluetooth-off"). Vazio =
    /// sem controller → esconde o applet. Boot + sinal.
    bluetooth: String,
    /// Popup aberto `(id, tipo)`, ou `None` (fechado). Um de cada vez.
    popup: Option<(IcedId, PopupKind)>,
    /// Dispositivos do mixer (carregados ao abrir o popup de volume).
    mixer: Vec<AudioDev>,
    /// Bluetooth ligado (toggle do popup).
    bt_enabled: bool,
    /// Há controller de bluetooth.
    bt_has_hw: bool,
    /// Dispositivos bluetooth (carregados ao abrir o popup).
    bt_devices: Vec<BtDev>,
    /// Modo "adicionar dispositivo" (varredura ativa).
    bt_adding: bool,
    /// Descobertos na varredura (não-pareados).
    bt_scan: Vec<BtDev>,
    /// Fase do pareamento `(fase, passkey, mensagem)`. Fase: 0 ocioso,
    /// 1 pareando, 2 confere o código, 3 deu certo, 4 falhou.
    bt_pairing: (u8, u32, String),
    /// Rede: wi-fi ligado, tem placa, device cabeado (ou ""), redes wi-fi.
    net_wifi_on: bool,
    net_has_wifi: bool,
    net_eth: String,
    net_wifi: Vec<WifiNet>,
    /// Sino: ícone do estado ("bell-normal"/"bell-alert"/"bell-dnd").
    notif_icon: String,
    /// DND e som (pros botões da central).
    notif_dnd: bool,
    notif_sound: bool,
    /// Lista da central (histórico).
    notif_list: Vec<Notif>,
    /// Itens da bandeja (SNI), poll ao vivo.
    tray: Vec<TrayItem>,
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    /// Tick de 1s — atualiza o relógio.
    Tick,
    /// Acento lido no boot / mudado pelo sinal — re-resolve o token ao vivo.
    AccentLoaded(Result<String, String>),
    AccentChanged(String),
    /// Alto contraste lido no boot / mudado pelo sinal.
    HighContrastLoaded(Result<bool, String>),
    HighContrastChanged(bool),
    /// Hora de re-ler a lista de janelas (tick de 500ms).
    PollWindows,
    /// Lista de janelas lida do compositor (id, título, app_id, focada, min).
    WindowsLoaded(Vec<(u64, String, String, bool, bool)>),
    /// Clique num botão de janela → foca/levanta (ou restaura) no compositor.
    ActivateWindow(u64),
    /// Clique na janela focada → minimiza (passo 7).
    MinimizeWindow(u64),
    /// Botão direito num botão de janela → fecha (passo 7).
    CloseWindow(u64),
    /// Botão "mostrar área de trabalho" (passo 7).
    ShowDesktop,
    /// Botão de menu (canto esquerdo) → abre/fecha o luma-menu.
    OpenMenu,
    /// Bateria lida no boot / mudada pelo sinal: `(present, capacity, charging)`.
    BatteryLoaded((bool, u8, bool)),
    BatteryChanged((bool, u8, bool)),
    /// Volume lido no boot / mudado pelo sinal: `(level, muted)`.
    VolumeLoaded((u8, bool)),
    VolumeChanged((u8, bool)),
    /// Scroll no applet de volume → muda o nível (±5).
    VolumeScroll(iced::mouse::ScrollDelta),
    /// Clique no applet de volume → alterna mudo.
    ToggleMute,
    /// No-op (descarta o resultado de um set fire-and-forget).
    Noop,
    /// Rede lida no boot / mudada pelo sinal: nome do ícone.
    NetworkLoaded(String),
    NetworkChanged(String),
    /// Bluetooth lido no boot / mudado pelo sinal: nome do ícone.
    BluetoothLoaded(String),
    BluetoothChanged(String),
    /// Clique no applet de volume → abre/fecha o popup do mixer.
    OpenVolumePopup,
    /// Clique no applet de bluetooth → abre/fecha o popup do bluetooth.
    OpenBluetoothPopup,
    /// Fecha o popup aberto (clique fora).
    ClosePopup,
    /// Lista de dispositivos do mixer carregada/recarregada.
    MixerLoaded(Vec<(u32, String, u32, bool, bool, bool)>),
    /// Slider de um dispositivo mexendo (ao vivo, só a UI): `(id, vol%)`.
    DeviceVolume(u32, u32),
    /// Slider solto → commita o volume daquele dispositivo no D-Bus.
    DeviceVolumeRelease(u32),
    /// Botão de mudo de um dispositivo → alterna no D-Bus.
    DeviceMute(u32),
    /// Estado do bluetooth carregado/recarregado.
    BluetoothStateLoaded(ipc::BtState),
    /// Toggle de energia do bluetooth.
    SetBtPower(bool),
    /// Conecta/desconecta um dispositivo bluetooth pela MAC.
    ConnectDev(String),
    DisconnectDev(String),
    /// Entra/sai do modo "adicionar dispositivo" (varredura).
    BtAddMode,
    BtExitAdd,
    /// Tick de poll enquanto procura (relê descobertos + fase).
    BtPoll,
    /// Lista do scan carregada.
    BtScanLoaded(Vec<(String, String, bool, bool)>),
    /// Fase do pareamento lida.
    BtPairingLoaded((u8, u32, String)),
    /// Inicia o pareamento de um device.
    PairDev(String),
    /// Responde a confirmação de código (confirma/cancela).
    BtConfirm(bool),
    /// Clique no applet de rede → abre/fecha o popup de rede.
    OpenNetworkPopup,
    /// Estado de rede carregado/recarregado.
    NetworkDevicesLoaded(ipc::NetState),
    /// Poll do popup de rede (relê a lista enquanto aberto).
    NetPoll,
    /// Conecta numa rede wi-fi pelo SSID.
    ConnectWifi(String),
    /// Liga/desliga o rádio wi-fi.
    SetWifiRadio(bool),
    /// Clique no sino → abre/fecha a central.
    OpenNotifPopup,
    /// Poll do sino (status sempre; lista se a central está aberta).
    NotifPoll,
    /// Status do sino carregado `(ícone, contagem, dnd, som)`.
    NotifStatusLoaded((String, u32, bool, bool)),
    /// Lista da central carregada.
    NotifListLoaded(Vec<(u32, String, String, String)>),
    /// Botões da central: DND, som, limpar.
    ToggleDnd,
    ToggleNotifSound,
    ClearNotifs,
    /// Lista da bandeja carregada (poll).
    TrayLoaded(Vec<(String, String, String, String, String)>),
    /// Clique num ícone da bandeja → ativa o app.
    TrayActivate(String, String),
    /// Clique na setinha → abre/fecha o popup de overflow.
    OpenTrayOverflow,
}

fn now_hm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Data curta pt-BR no estilo da barra velha: "seg · 22 jun".
fn now_date() -> String {
    use chrono::Datelike;
    let now = chrono::Local::now();
    format!(
        "{} · {:02} {}",
        weekday_pt_br(now.weekday().number_from_monday()),
        now.day(),
        month_pt_br(now.month())
    )
}

/// Abrev pt-BR do dia da semana (1=seg..7=dom). Espelha a barra velha.
fn weekday_pt_br(d: u32) -> &'static str {
    match d {
        1 => "seg",
        2 => "ter",
        3 => "qua",
        4 => "qui",
        5 => "sex",
        6 => "sáb",
        7 => "dom",
        _ => "?",
    }
}

/// Abrev pt-BR do mês (1=jan..12=dez). Espelha a barra velha.
fn month_pt_br(m: u32) -> &'static str {
    match m {
        1 => "jan",
        2 => "fev",
        3 => "mar",
        4 => "abr",
        5 => "mai",
        6 => "jun",
        7 => "jul",
        8 => "ago",
        9 => "set",
        10 => "out",
        11 => "nov",
        12 => "dez",
        _ => "?",
    }
}

fn boot() -> (Bar, Task<Message>) {
    // Daemon: abre a surface principal da barra no boot (StartMode::Background
    // não cria nenhuma; a barra abre a sua, e o popup abre sob demanda).
    let (_bar_id, open_bar) = Message::layershell_open(bar_settings());
    // Lê acento + HC + janelas no boot pra a barra abrir já no estado certo.
    let read = Task::batch([
        open_bar,
        Task::perform(ipc::read_accent(), Message::AccentLoaded),
        Task::perform(ipc::read_high_contrast(), Message::HighContrastLoaded),
        Task::perform(ipc::read_windows(), Message::WindowsLoaded),
        Task::perform(ipc::read_battery(), Message::BatteryLoaded),
        Task::perform(ipc::read_volume(), Message::VolumeLoaded),
        Task::perform(ipc::read_network(), Message::NetworkLoaded),
        Task::perform(ipc::read_bluetooth(), Message::BluetoothLoaded),
        Task::perform(ipc::read_tray_items(), Message::TrayLoaded),
    ]);
    (
        Bar {
            clock: now_hm(),
            date: now_date(),
            windows: Vec::new(),
            battery: None,
            volume: (0, false),
            network: String::new(),
            bluetooth: String::new(),
            popup: None,
            mixer: Vec::new(),
            bt_enabled: false,
            bt_has_hw: false,
            bt_devices: Vec::new(),
            bt_adding: false,
            bt_scan: Vec::new(),
            bt_pairing: (0, 0, String::new()),
            net_wifi_on: false,
            net_has_wifi: false,
            net_eth: String::new(),
            net_wifi: Vec::new(),
            notif_icon: "bell-normal".to_string(),
            notif_dnd: false,
            notif_sound: true,
            notif_list: Vec::new(),
            tray: Vec::new(),
        },
        read,
    )
}

/// Settings da surface principal da barra: topo, full-width, zona exclusiva.
fn bar_settings() -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((0, BAR_HEIGHT + ACCENT_LINE)),
        layer: Layer::Top,
        anchor: Anchor::Top | Anchor::Left | Anchor::Right,
        exclusive_zone: Some((BAR_HEIGHT + ACCENT_LINE) as i32),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::LastOutput,
        events_transparent: false,
        namespace: Some("luma-bar".to_string()),
    }
}

/// Sai do modo "adicionar dispositivo" se estava ativo: para a varredura,
/// zera a fase e limpa os descobertos.
fn stop_adding(state: &mut Bar) -> Task<Message> {
    if state.bt_adding {
        state.bt_adding = false;
        state.bt_scan.clear();
        state.bt_pairing = (0, 0, String::new());
        return Task::perform(ipc::stop_scan(), |_| Message::Noop);
    }
    Task::none()
}

/// Abre um popup (um de cada vez). Clicar no mesmo applet fecha (toggle);
/// clicar noutro troca. Carrega o conteúdo do tipo. Sair do BT para a procura.
fn open_popup(state: &mut Bar, kind: PopupKind) -> Task<Message> {
    let cleanup = stop_adding(state);
    if let Some((id, k)) = state.popup.take() {
        let close = Task::done(Message::RemoveWindow(id));
        if k == kind {
            return Task::batch([cleanup, close]); // toggle: fecha o mesmo
        }
        let (nid, open) = Message::layershell_open(popup_settings());
        state.popup = Some((nid, kind));
        return Task::batch([cleanup, close, open, load_popup(kind)]);
    }
    let (nid, open) = Message::layershell_open(popup_settings());
    state.popup = Some((nid, kind));
    Task::batch([cleanup, open, load_popup(kind)])
}

/// Carrega o conteúdo do popup conforme o tipo.
fn load_popup(kind: PopupKind) -> Task<Message> {
    match kind {
        PopupKind::Volume => Task::perform(ipc::read_audio_devices(), Message::MixerLoaded),
        PopupKind::Bluetooth => {
            Task::perform(ipc::read_bluetooth_state(), Message::BluetoothStateLoaded)
        }
        PopupKind::Network => {
            Task::perform(ipc::read_network_devices(), Message::NetworkDevicesLoaded)
        }
        PopupKind::Notifications => Task::batch([
            Task::perform(ipc::read_notification_status(), Message::NotifStatusLoaded),
            Task::perform(ipc::read_notifications(), Message::NotifListLoaded),
        ]),
        // Overflow lê de state.tray (já no poll); nada a carregar.
        PopupKind::TrayOverflow => Task::none(),
    }
}

/// Settings do popup: surface full-screen (pra o backdrop fechar no clique
/// fora), camada Overlay. O card é posicionado perto do applet via alinhamento
/// no view (canto superior-direito, sob o cluster de applets).
fn popup_settings() -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: None,
        layer: Layer::Overlay,
        anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
        exclusive_zone: Some(0),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::LastOutput,
        events_transparent: false,
        namespace: Some("luma-volume-popup".to_string()),
    }
}

fn update(state: &mut Bar, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            state.clock = now_hm();
            state.date = now_date();
        }
        // Acento/HC: giram os flags globais do libluma; o próximo render
        // re-resolve `accent()`/cores. Fonte única = boot + sinal.
        Message::AccentLoaded(Ok(id)) => libluma::tokens::set_accent_id(&id),
        Message::AccentLoaded(Err(e)) => eprintln!("luma-bar: ler acento falhou: {e}"),
        Message::AccentChanged(id) => libluma::tokens::set_accent_id(&id),
        Message::HighContrastLoaded(Ok(v)) => libluma::tokens::set_high_contrast(v),
        Message::HighContrastLoaded(Err(e)) => {
            eprintln!("luma-bar: ler alto contraste falhou: {e}")
        }
        Message::HighContrastChanged(v) => libluma::tokens::set_high_contrast(v),
        // Taskbar: poll → fetch; fetch → guarda; clique → ativa e re-poll na
        // hora (o destaque do foco atualiza rápido).
        Message::PollWindows => {
            return Task::perform(ipc::read_windows(), Message::WindowsLoaded);
        }
        Message::WindowsLoaded(list) => {
            state.windows = list
                .into_iter()
                .map(|(id, title, app_id, focused, minimized)| WinEntry {
                    id,
                    label: resolve_label(&title, &app_id),
                    focused,
                    minimized,
                })
                .collect();
        }
        Message::ActivateWindow(id) => {
            return Task::perform(ipc::activate_window(id), |_| Message::PollWindows);
        }
        Message::MinimizeWindow(id) => {
            return Task::perform(ipc::minimize_window(id), |_| Message::PollWindows);
        }
        Message::CloseWindow(id) => {
            return Task::perform(ipc::close_window(id), |_| Message::PollWindows);
        }
        Message::ShowDesktop => {
            return Task::perform(ipc::show_desktop(), |_| Message::PollWindows);
        }
        Message::OpenMenu => {
            return Task::perform(ipc::show_menu(), |_| Message::Noop);
        }
        // Bateria (passo 3): boot e sinal aplicam igual. `present=false` =
        // sem bateria (esconde o applet).
        Message::BatteryLoaded((present, cap, chg)) | Message::BatteryChanged((present, cap, chg)) => {
            state.battery = present.then_some((cap, chg));
        }
        // Volume (passo 4): boot/sinal aplicam; scroll/clique mexem otimista +
        // mandam o set/toggle (o sinal confirma).
        Message::VolumeLoaded((level, muted)) | Message::VolumeChanged((level, muted)) => {
            state.volume = (level, muted);
        }
        Message::VolumeScroll(delta) => {
            let y = match delta {
                iced::mouse::ScrollDelta::Lines { y, .. } => y,
                iced::mouse::ScrollDelta::Pixels { y, .. } => y,
            };
            let step: i32 = if y > 0.0 {
                5
            } else if y < 0.0 {
                -5
            } else {
                0
            };
            if step != 0 {
                let new = (state.volume.0 as i32 + step).clamp(0, 100) as u8;
                state.volume.0 = new; // otimista
                return Task::perform(ipc::set_volume(new), |_| Message::Noop);
            }
        }
        Message::ToggleMute => {
            state.volume.1 = !state.volume.1; // otimista
            return Task::perform(ipc::toggle_mute(), |_| Message::Noop);
        }
        // Popups: abre/fecha (um de cada vez) + carrega o conteúdo.
        Message::OpenVolumePopup => return open_popup(state, PopupKind::Volume),
        Message::OpenBluetoothPopup => return open_popup(state, PopupKind::Bluetooth),
        Message::ClosePopup => {
            let cleanup = stop_adding(state);
            if let Some((id, _)) = state.popup.take() {
                return Task::batch([cleanup, Task::done(Message::RemoveWindow(id))]);
            }
            return cleanup;
        }
        Message::MixerLoaded(list) => {
            state.mixer = list
                .into_iter()
                .map(|(id, name, volume, muted, is_source, is_default)| AudioDev {
                    id,
                    name,
                    volume,
                    muted,
                    is_source,
                    is_default,
                })
                .collect();
        }
        Message::DeviceVolume(id, pct) => {
            // Ao vivo: só a UI (o commit vem no soltar).
            if let Some(d) = state.mixer.iter_mut().find(|d| d.id == id) {
                d.volume = pct;
            }
        }
        Message::DeviceVolumeRelease(id) => {
            if let Some(d) = state.mixer.iter().find(|d| d.id == id) {
                let pct = d.volume;
                return Task::perform(
                    ipc::set_device_volume(id, pct),
                    Message::MixerLoaded,
                );
            }
        }
        Message::DeviceMute(id) => {
            return Task::perform(ipc::set_device_mute(id), Message::MixerLoaded);
        }
        Message::BluetoothStateLoaded((enabled, has_hw, devices)) => {
            state.bt_enabled = enabled;
            state.bt_has_hw = has_hw;
            state.bt_devices = devices
                .into_iter()
                .map(|(mac, name, paired, connected)| BtDev {
                    mac,
                    name,
                    paired,
                    connected,
                })
                .collect();
        }
        Message::SetBtPower(on) => {
            state.bt_enabled = on; // otimista
            return Task::perform(ipc::set_bluetooth_power(on), Message::BluetoothStateLoaded);
        }
        Message::ConnectDev(mac) => {
            return Task::perform(ipc::connect_device(mac), Message::BluetoothStateLoaded);
        }
        Message::DisconnectDev(mac) => {
            return Task::perform(ipc::disconnect_device(mac), Message::BluetoothStateLoaded);
        }
        Message::BtAddMode => {
            state.bt_adding = true;
            state.bt_scan.clear();
            state.bt_pairing = (0, 0, String::new());
            return Task::perform(ipc::start_scan(), |_| Message::BtPoll);
        }
        Message::BtExitAdd => return stop_adding(state),
        Message::BtPoll => {
            if state.bt_adding {
                return Task::batch([
                    Task::perform(ipc::read_scan_devices(), Message::BtScanLoaded),
                    Task::perform(ipc::read_pairing_state(), Message::BtPairingLoaded),
                ]);
            }
        }
        Message::BtScanLoaded(list) => {
            // Só os não-pareados (os pareados já estão na lista principal).
            state.bt_scan = list
                .into_iter()
                .filter(|(_, _, paired, _)| !paired)
                .map(|(mac, name, paired, connected)| BtDev {
                    mac,
                    name,
                    paired,
                    connected,
                })
                .collect();
        }
        Message::BtPairingLoaded((phase, passkey, msg)) => {
            state.bt_pairing = (phase, passkey, msg);
            // Pareou: sai do modo e recarrega a lista de pareados.
            if phase == 3 {
                let leave = stop_adding(state);
                return Task::batch([
                    leave,
                    Task::perform(ipc::read_bluetooth_state(), Message::BluetoothStateLoaded),
                ]);
            }
        }
        Message::PairDev(mac) => {
            return Task::perform(ipc::pair_device(mac), |_| Message::BtPoll);
        }
        Message::BtConfirm(ok) => {
            return Task::perform(ipc::confirm_pairing(ok), |_| Message::BtPoll);
        }
        Message::OpenNetworkPopup => return open_popup(state, PopupKind::Network),
        Message::NetworkDevicesLoaded((wifi_on, has_wifi, eth, nets)) => {
            state.net_wifi_on = wifi_on;
            state.net_has_wifi = has_wifi;
            state.net_eth = eth;
            state.net_wifi = nets
                .into_iter()
                .map(|(ssid, signal, protected, in_use)| WifiNet {
                    ssid,
                    signal,
                    protected,
                    in_use,
                })
                .collect();
        }
        Message::NetPoll => {
            if matches!(state.popup, Some((_, PopupKind::Network))) {
                return Task::perform(ipc::read_network_devices(), Message::NetworkDevicesLoaded);
            }
        }
        Message::ConnectWifi(ssid) => {
            return Task::perform(ipc::connect_wifi(ssid), Message::NetworkDevicesLoaded);
        }
        Message::SetWifiRadio(on) => {
            state.net_wifi_on = on; // otimista
            return Task::perform(ipc::set_wifi_radio(on), Message::NetworkDevicesLoaded);
        }
        Message::OpenNotifPopup => return open_popup(state, PopupKind::Notifications),
        Message::NotifPoll => {
            // Aproveita o tick de 2s pra o sino E a bandeja (aparece/some ao vivo).
            let mut tasks = vec![
                Task::perform(ipc::read_notification_status(), Message::NotifStatusLoaded),
                Task::perform(ipc::read_tray_items(), Message::TrayLoaded),
            ];
            if matches!(state.popup, Some((_, PopupKind::Notifications))) {
                tasks.push(Task::perform(ipc::read_notifications(), Message::NotifListLoaded));
            }
            return Task::batch(tasks);
        }
        Message::NotifStatusLoaded((icon, _count, dnd, sound)) => {
            state.notif_icon = icon;
            state.notif_dnd = dnd;
            state.notif_sound = sound;
        }
        Message::NotifListLoaded(list) => {
            state.notif_list = list
                .into_iter()
                .map(|(id, app, summary, body)| Notif {
                    id,
                    app,
                    summary,
                    body,
                })
                .collect();
        }
        Message::ToggleDnd => return Task::perform(ipc::toggle_dnd(), |_| Message::NotifPoll),
        Message::ToggleNotifSound => {
            return Task::perform(ipc::toggle_notification_sound(), |_| Message::NotifPoll);
        }
        Message::ClearNotifs => {
            return Task::perform(ipc::clear_notifications(), |_| Message::NotifPoll);
        }
        Message::TrayLoaded(list) => {
            state.tray = list
                .into_iter()
                .map(|(service, path, _title, tooltip, icon_path)| TrayItem {
                    service,
                    path,
                    tooltip,
                    icon_path,
                })
                .collect();
            // Bandeja esvaziou com o popup aberto → recolhe (some a setinha).
            if state.tray.is_empty()
                && matches!(state.popup, Some((_, PopupKind::TrayOverflow)))
            {
                if let Some((id, _)) = state.popup.take() {
                    return Task::done(Message::RemoveWindow(id));
                }
            }
        }
        Message::TrayActivate(service, path) => {
            return Task::perform(ipc::tray_activate(service, path), |_| Message::Noop);
        }
        Message::OpenTrayOverflow => return open_popup(state, PopupKind::TrayOverflow),
        Message::Noop => {}
        // Rede (passo 5): boot/sinal aplicam o nome do ícone.
        Message::BluetoothLoaded(icon) | Message::BluetoothChanged(icon) => {
            state.bluetooth = icon;
        }
        Message::NetworkLoaded(icon) | Message::NetworkChanged(icon) => {
            state.network = icon;
        }
        _ => {}
    }
    Task::none()
}

/// Resolve o rótulo do botão: título, senão app_id, senão genérico. Trunca.
fn resolve_label(title: &str, app_id: &str) -> String {
    let raw = if !title.trim().is_empty() {
        title.trim()
    } else if !app_id.trim().is_empty() {
        app_id.trim()
    } else {
        "Janela"
    };
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() > TITLE_MAX {
        format!("{}…", chars[..TITLE_MAX - 1].iter().collect::<String>())
    } else {
        raw.to_string()
    }
}

/// Handle do SVG de um applet por nome de estado. **Todo applet puxa daqui**
/// — os SVGs vivem em `assets/branding/icons/`, bundlados via `include_bytes!`.
/// Ninguém desenha ícone na mão. Fallback: wifi-off.
fn applet_svg(name: &str) -> svg::Handle {
    let bytes: &'static [u8] = match name {
        // rede
        "ethernet-on" => include_bytes!("../../../assets/branding/icons/icon-ethernet-on.svg"),
        "ethernet-off" => include_bytes!("../../../assets/branding/icons/icon-ethernet-off.svg"),
        "wifi-full" => include_bytes!("../../../assets/branding/icons/icon-wifi-full.svg"),
        "wifi-low" => include_bytes!("../../../assets/branding/icons/icon-wifi-low.svg"),
        "wifi-none" => include_bytes!("../../../assets/branding/icons/icon-wifi-none.svg"),
        "wifi-off" => include_bytes!("../../../assets/branding/icons/icon-wifi-off.svg"),
        // volume
        "volume-high" => include_bytes!("../../../assets/branding/icons/icon-volume-high.svg"),
        "volume-medium" => include_bytes!("../../../assets/branding/icons/icon-volume-medium.svg"),
        "volume-low" => include_bytes!("../../../assets/branding/icons/icon-volume-low.svg"),
        "volume-muted" => include_bytes!("../../../assets/branding/icons/icon-volume-muted.svg"),
        // bateria
        "battery-charging" => {
            include_bytes!("../../../assets/branding/icons/icon-battery-charging.svg")
        }
        "battery-high" => include_bytes!("../../../assets/branding/icons/icon-battery-high.svg"),
        "battery-medium" => {
            include_bytes!("../../../assets/branding/icons/icon-battery-medium.svg")
        }
        "battery-low" => include_bytes!("../../../assets/branding/icons/icon-battery-low.svg"),
        // bluetooth
        "bluetooth-on" => include_bytes!("../../../assets/branding/icons/icon-bluetooth-on.svg"),
        "bluetooth-off" => include_bytes!("../../../assets/branding/icons/icon-bluetooth-off.svg"),
        // sino
        "bell-normal" => include_bytes!("../../../assets/branding/icons/icon-bell-normal.svg"),
        "bell-alert" => include_bytes!("../../../assets/branding/icons/icon-bell-alert.svg"),
        "bell-dnd" => include_bytes!("../../../assets/branding/icons/icon-bell-dnd.svg"),
        // bandeja: setinha de overflow
        "tray-overflow-down" => {
            include_bytes!("../../../assets/branding/icons/icon-tray-overflow-down.svg")
        }
        "tray-overflow-up" => {
            include_bytes!("../../../assets/branding/icons/icon-tray-overflow-up.svg")
        }
        _ => include_bytes!("../../../assets/branding/icons/icon-wifi-off.svg"),
    };
    svg::Handle::from_memory(bytes)
}

/// Ícone SVG de applet, tamanho fixo, tingido pelo tema.
fn applet_icon<'a>(name: &str, size: f32, color: Color) -> Element<'a, Message> {
    svg(applet_svg(name))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

/// Applet de bateria: o SVG do estado + "%". Carregando = sage; baixa (<20% e
/// descarregando) = coral (alerta próprio, não o acento); normal = creme. O
/// nível mapeia pro arquivo certo (charging/high/medium/low).
fn battery_applet<'a>(cap: u8, charging: bool) -> Element<'a, Message> {
    let pal = libluma::tokens::Palette::for_contrast(libluma::tokens::high_contrast());
    let cream = libluma::tokens::to_iced(pal.cream);
    let coral = libluma::tokens::to_iced(pal.coral);
    let sage = libluma::tokens::to_iced(pal.sage);
    let fill_color = if charging {
        sage
    } else if cap < 20 {
        coral
    } else {
        cream
    };
    let name = if charging {
        "battery-charging"
    } else if cap >= 66 {
        "battery-high"
    } else if cap >= 33 {
        "battery-medium"
    } else {
        "battery-low"
    };

    row![
        applet_icon(name, 18.0, fill_color),
        text(format!("{cap}%")).size(13.0).color(fill_color)
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .into()
}

/// Applet de rede: o SVG do estado (tipo + força), tingido pelo tema. Clique
/// abre o popup de rede.
fn network_applet<'a>(icon: &str) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    mouse_area(applet_icon(icon, 18.0, cream))
        .on_press(Message::OpenNetworkPopup)
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

/// Ícone de um item da bandeja (do app, best-effort): SVG ou raster do caminho
/// resolvido; fallback num pontinho de acento se vazio.
fn tray_icon<'a>(path: &str) -> Element<'a, Message> {
    if path.ends_with(".svg") {
        svg(svg::Handle::from_path(path.to_string()))
            .width(Length::Fixed(18.0))
            .height(Length::Fixed(18.0))
            .into()
    } else if !path.is_empty() {
        image(image::Handle::from_path(path.to_string()))
            .width(Length::Fixed(18.0))
            .height(Length::Fixed(18.0))
            .into()
    } else {
        let accent = libluma::tokens::accent();
        container(Space::new().width(Length::Fixed(8.0)).height(Length::Fixed(8.0)))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(accent)),
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }
}

/// Um ícone de bandeja com tooltip + clique-ativa (usado na barra e no popup).
fn tray_icon_button<'a>(it: &TrayItem) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    let surface = libluma::tokens::to_iced(libluma::tokens::semantic_default().background);
    let btn = mouse_area(tray_icon(&it.icon_path))
        .on_press(Message::TrayActivate(it.service.clone(), it.path.clone()))
        .interaction(iced::mouse::Interaction::Pointer);
    let tip = if it.tooltip.trim().is_empty() {
        it.service.clone()
    } else {
        it.tooltip.clone()
    };
    tooltip(
        btn,
        container(text(tip).size(12.0).color(cream))
            .padding(Padding::from([4.0, 8.0]))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(surface)),
                border: Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        tooltip::Position::Bottom,
    )
    .into()
}

/// A bandeja na barra é uma **gaveta**: só a setinha (quando há itens); todos
/// os ícones moram no popup. Setinha pra cima quando aberto, pra baixo fechado.
/// Vazia = nada (sem setinha).
fn tray_view(items: &[TrayItem], open: bool) -> Element<'_, Message> {
    if items.is_empty() {
        return Space::new().width(Length::Fixed(0.0)).into();
    }
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    let arrow = if open {
        "tray-overflow-up"
    } else {
        "tray-overflow-down"
    };
    mouse_area(applet_icon(arrow, 16.0, cream))
        .on_press(Message::OpenTrayOverflow)
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

/// Corpo do popup da bandeja: TODOS os ícones (com tooltip + clique-ativa).
fn tray_overflow_body(state: &Bar) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    let cream = libluma::tokens::to_iced(sem.text);
    let mut items: Vec<Element<Message>> = Vec::new();
    items.push(popup_title("Bandeja", cream));
    if state.tray.is_empty() {
        items.push(
            text("Sem itens")
                .size(13.0)
                .color(libluma::tokens::to_iced(sem.text_secondary))
                .into(),
        );
    } else {
        let icons: Vec<Element<Message>> =
            state.tray.iter().map(|it| tray_icon_button(it)).collect();
        items.push(row(icons).spacing(14).align_y(iced::Alignment::Center).into());
    }
    column(items).spacing(12.0).into()
}

/// Applet do sino: o SVG do estado (normal/alerta/dnd), tingido pelo tema.
/// Clique abre a central de notificações.
fn notification_applet<'a>(icon: &str) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    let accent = libluma::tokens::accent();
    // Alerta no acento (chama atenção); normal/dnd no creme.
    let color = if icon == "bell-alert" { accent } else { cream };
    mouse_area(applet_icon(icon, 18.0, color))
        .on_press(Message::OpenNotifPopup)
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

/// Applet de bluetooth: o SVG do estado (on/off), tingido pelo tema. (Só há
/// on/off nos assets — "conectado" usa on.) Escondido se não há controller.
/// Clique abre o popup do bluetooth.
fn bluetooth_applet<'a>(icon: &str) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    mouse_area(applet_icon(icon, 18.0, cream))
        .on_press(Message::OpenBluetoothPopup)
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

/// Applet de volume: o SVG do estado (alto/médio/baixo/mudo) + "%". **scroll**
/// muda o nível e **clique** alterna mudo. Mudo = ícone mudo + "mudo".
fn volume_applet<'a>(level: u8, muted: bool) -> Element<'a, Message> {
    let pal = libluma::tokens::Palette::for_contrast(libluma::tokens::high_contrast());
    let cream = libluma::tokens::to_iced(pal.cream);
    let name = if muted || level == 0 {
        "volume-muted"
    } else if level >= 66 {
        "volume-high"
    } else if level >= 33 {
        "volume-medium"
    } else {
        "volume-low"
    };
    let icon_color = if muted {
        Color { a: 0.45, ..cream }
    } else {
        cream
    };

    let label = if muted {
        "mudo".to_string()
    } else {
        format!("{level}%")
    };
    let label_color = if muted {
        Color { a: 0.55, ..cream }
    } else {
        cream
    };

    let content = row![
        applet_icon(name, 18.0, icon_color),
        text(label).size(12.0).color(label_color)
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    mouse_area(content)
        .on_scroll(Message::VolumeScroll)
        .on_press(Message::OpenVolumePopup)
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

/// Dispatch do daemon: a surface da barra renderiza a barra; a do popup
/// renderiza o conteúdo do tipo aberto.
fn view(state: &Bar, id: iced::window::Id) -> Element<'_, Message> {
    if let Some((pid, kind)) = &state.popup {
        if *pid == id {
            return match kind {
                PopupKind::Volume => popup_frame(volume_popup_body(state), 60.0),
                PopupKind::Bluetooth => popup_frame(bluetooth_popup_body(state), 200.0),
                PopupKind::Network => popup_frame(network_popup_body(state), 130.0),
                PopupKind::Notifications => popup_frame(notif_popup_body(state), 30.0),
                PopupKind::TrayOverflow => popup_frame(tray_overflow_body(state), 100.0),
            };
        }
    }
    bar_view(state)
}

fn bar_view(state: &Bar) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    let bar_bg = libluma::tokens::to_iced(sem.surface);
    let accent = libluma::tokens::accent();
    let secondary = libluma::tokens::to_iced(sem.text_secondary);

    // Conteúdo: dot de acento + relógio à direita. O dot e a linha de base
    // seguem o acento — a personalização aparece na hora.
    let dot = container(Space::new().width(Length::Fixed(7.0)).height(Length::Fixed(7.0))).style(move |_| {
        container::Style {
            background: Some(Background::Color(accent)),
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    });
    // Botões de janela (esquerda): um por janela, título; a focada destacada
    // no acento (texto na cor do fundo). Clique → ativa/foca.
    let cream = libluma::tokens::to_iced(sem.text);
    let bg_color = libluma::tokens::to_iced(sem.background);
    let win_buttons: Vec<Element<Message>> = state
        .windows
        .iter()
        .map(|w| {
            // focada = acento (clique minimiza); minimizada = dim (clique
            // restaura); normal = sutil (clique foca). Direito = fecha.
            let (bg, label_color) = if w.focused {
                (accent, bg_color)
            } else if w.minimized {
                (Color { a: 0.05, ..cream }, Color { a: 0.45, ..cream })
            } else {
                (Color { a: 0.10, ..cream }, cream)
            };
            let left = if w.focused {
                Message::MinimizeWindow(w.id)
            } else {
                Message::ActivateWindow(w.id)
            };
            let inner = container(text(w.label.clone()).size(13.0).color(label_color))
                .padding(iced::Padding::from([3.0, 10.0]))
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(bg)),
                    border: Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            mouse_area(inner)
                .on_press(left)
                .on_right_press(Message::CloseWindow(w.id))
                .interaction(iced::mouse::Interaction::Pointer)
                .into()
        })
        .collect();
    let windows_row = row(win_buttons)
        .spacing(6)
        .align_y(iced::Alignment::Center);

    // Botão de menu (canto esquerdo): a marca do Luma, tingida no acento.
    // Clique chama o ShowMenu do compositor (mesmo caminho do Super).
    let menu_btn = mouse_area(
        svg(svg::Handle::from_memory(
            include_bytes!("../../../assets/branding/icons/icon-luma-menu.svg").as_slice(),
        ))
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .style(move |_theme, _status| svg::Style { color: Some(accent) }),
    )
    .on_press(Message::OpenMenu)
    .interaction(iced::mouse::Interaction::Pointer);

    // Botão "mostrar área de trabalho" (canto direito, como a barra velha).
    let show_desktop_btn = mouse_area(
        svg(svg::Handle::from_memory(
            include_bytes!("../../../assets/branding/icons/icon-show-desktop.svg").as_slice(),
        ))
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .style(move |_theme, _status| svg::Style { color: Some(cream) }),
    )
    .on_press(Message::ShowDesktop)
    .interaction(iced::mouse::Interaction::Pointer);

    // Applet de bateria (direita), só se há bateria. Segue o tema; baixa tem
    // cor de alerta própria (coral), carregando fica sage — não o acento.
    let battery_el: Element<Message> = match state.battery {
        Some((cap, charging)) => battery_applet(cap, charging),
        None => Space::new().width(Length::Fixed(0.0)).into(),
    };
    // Rede: o ícone do estado (vazio até o boot ler).
    let network_el: Element<Message> = if state.network.is_empty() {
        Space::new().width(Length::Fixed(0.0)).into()
    } else {
        network_applet(&state.network)
    };
    // Bluetooth: o ícone do estado; vazio = sem controller → esconde.
    let bluetooth_el: Element<Message> = if state.bluetooth.is_empty() {
        Space::new().width(Length::Fixed(0.0)).into()
    } else {
        bluetooth_applet(&state.bluetooth)
    };

    let content = container(
        row![
            Space::new().width(Length::Fixed(12.0)),
            menu_btn,
            Space::new().width(Length::Fixed(10.0)),
            windows_row,
            Space::new().width(Length::Fill),
            bluetooth_el,
            Space::new().width(Length::Fixed(14.0)),
            network_el,
            Space::new().width(Length::Fixed(14.0)),
            volume_applet(state.volume.0, state.volume.1),
            Space::new().width(Length::Fixed(14.0)),
            battery_el,
            Space::new().width(Length::Fixed(14.0)),
            dot,
            // Data (secundária) + hora (acento), lado a lado.
            row![
                text(state.date.clone()).size(12.0).color(secondary),
                text(state.clock.clone()).size(15.0).color(accent),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            Space::new().width(Length::Fixed(14.0)),
            tray_view(
                &state.tray,
                matches!(state.popup, Some((_, PopupKind::TrayOverflow))),
            ),
            Space::new().width(Length::Fixed(14.0)),
            notification_applet(&state.notif_icon),
            Space::new().width(Length::Fixed(14.0)),
            show_desktop_btn,
            Space::new().width(Length::Fixed(12.0)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(iced::alignment::Vertical::Center)
    .style(move |_| container::Style {
        background: Some(Background::Color(bar_bg)),
        ..Default::default()
    });

    // Linha de acento na base (a moldura Golden Hour da barra).
    let accent_line = container(Space::new().width(Length::Fill).height(Length::Fixed(ACCENT_LINE as f32)))
        .style(move |_| container::Style {
            background: Some(Background::Color(accent)),
            ..Default::default()
        });

    column![content, accent_line]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Uma linha do mixer: nome (+ default) + botão de mudo + slider de volume.
fn device_row<'a>(d: &AudioDev) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    let secondary =
        libluma::tokens::to_iced(libluma::tokens::semantic_default().text_secondary);
    let id = d.id;

    // Nome + marca de default.
    let title = if d.is_default {
        format!("{}  ·  padrão", d.name)
    } else {
        d.name.clone()
    };
    let name_color = if d.is_default { cream } else { secondary };

    // Botão de mudo: ícone do estado, clique alterna.
    let mute_name = if d.muted { "volume-muted" } else { "volume-high" };
    let mute_color = if d.muted {
        Color { a: 0.5, ..cream }
    } else {
        cream
    };
    let mute_btn = mouse_area(applet_icon(mute_name, 18.0, mute_color))
        .on_press(Message::DeviceMute(id))
        .interaction(iced::mouse::Interaction::Pointer);

    // Slider 0–150% (libluma): ao vivo na UI, commita no soltar.
    let slider = libluma::slider_row(
        "",
        d.volume as f32,
        0.0..=150.0,
        1.0,
        "%",
        move |v| Message::DeviceVolume(id, v.round() as u32),
        Some(Message::DeviceVolumeRelease(id)),
    );

    column![
        row![text(title).size(13.0).color(name_color), Space::new().width(Length::Fill), mute_btn]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        slider,
    ]
    .spacing(4)
    .into()
}

/// Título de popup (Fraunces, semibold).
fn popup_title<'a>(s: &'a str, color: Color) -> Element<'a, Message> {
    text(s)
        .size(16.0)
        .color(color)
        .font(iced::Font {
            family: iced::font::Family::Name(libluma::tokens::FONT_DISPLAY),
            weight: iced::font::Weight::Semibold,
            ..iced::Font::DEFAULT
        })
        .into()
}

/// Moldura comum dos popups: backdrop transparente (clique-fora fecha) + card
/// no canto superior-direito, abaixo da barra, com margem direita por tipo
/// (perto do applet que abriu). `body` é o conteúdo (já rolável).
fn popup_frame<'a>(body: Element<'a, Message>, right_margin: f32) -> Element<'a, Message> {
    let sem = libluma::tokens::semantic_default();
    let surface = libluma::tokens::to_iced(sem.surface);
    let cream = libluma::tokens::to_iced(sem.text);

    // Backdrop TRANSPARENTE: cobre a tela só pra capturar o clique-fora (fecha),
    // sem escurecer nada.
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill)).style(|_theme| {
            container::Style {
                background: None,
                ..Default::default()
            }
        }),
    )
    .on_press(Message::ClosePopup);

    let card = mouse_area(
        container(body)
            .width(Length::Fixed(340.0))
            .max_height(520.0)
            .padding(18)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(surface)),
                border: Border {
                    radius: 16.0.into(),
                    width: 1.0,
                    color: Color { a: 0.22, ..cream },
                },
                ..Default::default()
            }),
    )
    .on_press(Message::Noop);

    stack![
        backdrop,
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Top)
            .padding(Padding {
                top: (BAR_HEIGHT + ACCENT_LINE) as f32 + 6.0,
                right: right_margin,
                bottom: 0.0,
                left: 0.0,
            }),
    ]
    .into()
}

/// Corpo do popup do volume: saídas e entradas com sliders.
fn volume_popup_body(state: &Bar) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    let cream = libluma::tokens::to_iced(sem.text);
    let secondary = libluma::tokens::to_iced(sem.text_secondary);

    let mut items: Vec<Element<Message>> = Vec::new();
    items.push(popup_title("Volume", cream));

    let sinks: Vec<&AudioDev> = state.mixer.iter().filter(|d| !d.is_source).collect();
    let sources: Vec<&AudioDev> = state.mixer.iter().filter(|d| d.is_source).collect();

    if !sinks.is_empty() {
        items.push(text("Saídas").size(11.0).color(secondary).into());
        for d in sinks {
            items.push(device_row(d));
        }
    }
    if !sources.is_empty() {
        items.push(text("Entradas").size(11.0).color(secondary).into());
        for d in sources {
            items.push(device_row(d));
        }
    }
    if state.mixer.is_empty() {
        items.push(text("Sem dispositivos de áudio").size(13.0).color(secondary).into());
    }

    scrollable(column(items).spacing(12.0)).into()
}

/// Botão-pílula simples (Confirmar/Cancelar/Adicionar/Voltar).
fn pill_button<'a>(label: &'a str, msg: Message, filled: bool) -> Element<'a, Message> {
    let sem = libluma::tokens::semantic_default();
    let cream = libluma::tokens::to_iced(sem.text);
    let bg = libluma::tokens::to_iced(sem.background);
    let accent = libluma::tokens::accent();
    let (fill, fg) = if filled {
        (accent, bg)
    } else {
        (Color { a: 0.10, ..cream }, cream)
    };
    mouse_area(
        container(text(label).size(13.0).color(fg))
            .padding(Padding::from([7.0, 14.0]))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(fill)),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_press(msg)
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

/// Banner do estado do pareamento (procurando/pareando/confere/deu certo/falhou).
fn pairing_banner<'a>(state: &Bar) -> Option<Element<'a, Message>> {
    let sem = libluma::tokens::semantic_default();
    let cream = libluma::tokens::to_iced(sem.text);
    let secondary = libluma::tokens::to_iced(sem.text_secondary);
    let accent = libluma::tokens::accent();
    let (phase, passkey, msg) = &state.bt_pairing;
    match phase {
        1 => Some(text("Pareando…").size(13.0).color(secondary).into()),
        2 => Some(
            column![
                text("Confere o código no aparelho:").size(13.0).color(cream),
                text(format!("{:06}", passkey))
                    .size(26.0)
                    .color(accent)
                    .font(iced::Font {
                        family: iced::font::Family::Name(libluma::tokens::FONT_DISPLAY),
                        weight: iced::font::Weight::Semibold,
                        ..iced::Font::DEFAULT
                    }),
                row![
                    pill_button("Confirmar", Message::BtConfirm(true), true),
                    pill_button("Cancelar", Message::BtConfirm(false), false),
                ]
                .spacing(8),
            ]
            .spacing(8)
            .into(),
        ),
        3 => Some(text("Pareado!").size(13.0).color(accent).into()),
        4 => {
            let why = if msg.is_empty() {
                "Falhou".to_string()
            } else {
                format!("Falhou: {msg}")
            };
            Some(text(why).size(13.0).color(secondary).into())
        }
        _ => None,
    }
}

/// Corpo do popup do bluetooth. Dois modos: lista de pareados (com toggle e
/// "Adicionar dispositivo") ou modo adicionar (varredura + estados do pair).
fn bluetooth_popup_body(state: &Bar) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    let cream = libluma::tokens::to_iced(sem.text);
    let secondary = libluma::tokens::to_iced(sem.text_secondary);

    let mut items: Vec<Element<Message>> = Vec::new();

    if state.bt_adding {
        // Modo adicionar: voltar + título + banner do estado + descobertos.
        items.push(
            row![
                pill_button("‹ Voltar", Message::BtExitAdd, false),
                popup_title("Adicionar", cream),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center)
            .into(),
        );
        if let Some(b) = pairing_banner(state) {
            items.push(b);
        }
        if state.bt_scan.is_empty() {
            items.push(text("Procurando aparelhos…").size(13.0).color(secondary).into());
        } else {
            items.push(text("Encontrados").size(11.0).color(secondary).into());
            for d in &state.bt_scan {
                items.push(bt_scan_row(d));
            }
        }
        return scrollable(column(items).spacing(12.0)).into();
    }

    // Modo normal.
    items.push(popup_title("Bluetooth", cream));
    if !state.bt_has_hw {
        items.push(text("Sem controller de bluetooth").size(13.0).color(secondary).into());
        return scrollable(column(items).spacing(12.0)).into();
    }
    items.push(libluma::toggle_row(
        "Bluetooth",
        state.bt_enabled,
        Message::SetBtPower,
    ));
    if !state.bt_enabled {
        items.push(text("Ligue pra ver os dispositivos").size(13.0).color(secondary).into());
    } else {
        if state.bt_devices.is_empty() {
            items.push(text("Nenhum dispositivo pareado").size(13.0).color(secondary).into());
        } else {
            items.push(text("Dispositivos").size(11.0).color(secondary).into());
            for d in &state.bt_devices {
                items.push(bt_device_row(d));
            }
        }
        items.push(pill_button("+ Adicionar dispositivo", Message::BtAddMode, false));
    }

    scrollable(column(items).spacing(12.0)).into()
}

/// Linha de um descoberto na varredura: nome + "tocar pra parear".
fn bt_scan_row<'a>(d: &BtDev) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    let secondary =
        libluma::tokens::to_iced(libluma::tokens::semantic_default().text_secondary);
    let name = if d.name.trim().is_empty() {
        d.mac.clone()
    } else {
        d.name.clone()
    };
    let content = column![
        text(name).size(14.0).color(cream),
        text("tocar pra parear").size(11.0).color(secondary),
    ]
    .spacing(2);
    mouse_area(
        container(content)
            .width(Length::Fill)
            .padding(Padding::from([8.0, 10.0]))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(Color { a: 0.06, ..cream })),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_press(Message::PairDev(d.mac.clone()))
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

/// Uma linha de dispositivo bluetooth: nome + estado; clique conecta/desconecta.
fn bt_device_row<'a>(d: &BtDev) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    let secondary =
        libluma::tokens::to_iced(libluma::tokens::semantic_default().text_secondary);
    let accent = libluma::tokens::accent();

    let mac = d.mac.clone();
    let msg = if d.connected {
        Message::DisconnectDev(mac)
    } else {
        Message::ConnectDev(mac)
    };
    let state_label = if d.connected { "conectado" } else { "tocar pra conectar" };
    let state_color = if d.connected { accent } else { secondary };

    let content = column![
        text(d.name.clone()).size(14.0).color(cream),
        text(state_label).size(11.0).color(state_color),
    ]
    .spacing(2);

    mouse_area(
        container(content)
            .width(Length::Fill)
            .padding(Padding::from([8.0, 10.0]))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(Color {
                    a: 0.06,
                    ..cream
                })),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_press(msg)
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

/// Corpo do popup de rede: cabeada (se houver) + wi-fi (toggle + lista).
fn network_popup_body(state: &Bar) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    let cream = libluma::tokens::to_iced(sem.text);
    let secondary = libluma::tokens::to_iced(sem.text_secondary);
    let accent = libluma::tokens::accent();

    let mut items: Vec<Element<Message>> = Vec::new();
    items.push(popup_title("Rede", cream));

    // Com fio (informativo, conectada no acento).
    if !state.net_eth.is_empty() {
        items.push(text("Com fio").size(11.0).color(secondary).into());
        let eth = state.net_eth.clone();
        items.push(
            container(
                column![
                    text("Conectado").size(14.0).color(accent),
                    text(eth).size(11.0).color(secondary),
                ]
                .spacing(2),
            )
            .width(Length::Fill)
            .padding(Padding::from([8.0, 10.0]))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(Color { a: 0.06, ..cream })),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into(),
        );
    }

    // Wi-Fi.
    if state.net_has_wifi {
        items.push(libluma::toggle_row(
            "Wi-Fi",
            state.net_wifi_on,
            Message::SetWifiRadio,
        ));
        if !state.net_wifi_on {
            items.push(text("Wi-Fi desligado").size(13.0).color(secondary).into());
        } else if state.net_wifi.is_empty() {
            items.push(text("Procurando redes…").size(13.0).color(secondary).into());
        } else {
            items.push(text("Redes").size(11.0).color(secondary).into());
            for n in &state.net_wifi {
                items.push(wifi_row(n));
            }
        }
    } else if state.net_eth.is_empty() {
        items.push(text("Sem conexão").size(13.0).color(secondary).into());
    }

    scrollable(column(items).spacing(12.0)).into()
}

/// Linha de uma rede wi-fi: ícone do sinal + nome + estado; clique conecta.
/// A conectada fica destacada no acento.
fn wifi_row<'a>(n: &WifiNet) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    let secondary =
        libluma::tokens::to_iced(libluma::tokens::semantic_default().text_secondary);
    let accent = libluma::tokens::accent();

    let icon = if n.signal >= 66 {
        "wifi-full"
    } else if n.signal >= 33 {
        "wifi-low"
    } else {
        "wifi-none"
    };
    let color = if n.in_use { accent } else { cream };
    let sub = if n.in_use {
        "conectada".to_string()
    } else {
        format!(
            "{}% · {}",
            n.signal,
            if n.protected { "protegida" } else { "aberta" }
        )
    };
    let sub_color = if n.in_use { accent } else { secondary };

    let content = row![
        applet_icon(icon, 18.0, color),
        column![
            text(n.ssid.clone()).size(14.0).color(color),
            text(sub).size(11.0).color(sub_color),
        ]
        .spacing(2),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    mouse_area(
        container(content)
            .width(Length::Fill)
            .padding(Padding::from([8.0, 10.0]))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(Color { a: 0.06, ..cream })),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_press(Message::ConnectWifi(n.ssid.clone()))
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

/// Corpo da central: botões (DND/Som/Limpar) + lista de notificações.
fn notif_popup_body(state: &Bar) -> Element<'_, Message> {
    let sem = libluma::tokens::semantic_default();
    let cream = libluma::tokens::to_iced(sem.text);
    let secondary = libluma::tokens::to_iced(sem.text_secondary);

    let mut items: Vec<Element<Message>> = Vec::new();
    items.push(popup_title("Notificações", cream));

    // Botões: DND (aceso quando ligado), Som (aceso quando ligado), Limpar.
    items.push(
        row![
            pill_button("Não perturbe", Message::ToggleDnd, state.notif_dnd),
            pill_button("Som", Message::ToggleNotifSound, state.notif_sound),
            pill_button("Limpar", Message::ClearNotifs, false),
        ]
        .spacing(8)
        .into(),
    );

    if state.notif_list.is_empty() {
        items.push(text("Sem notificações").size(13.0).color(secondary).into());
    } else {
        for n in &state.notif_list {
            items.push(notif_card(n));
        }
    }

    scrollable(column(items).spacing(12.0)).into()
}

/// Card de uma notificação: app + resumo + corpo.
fn notif_card<'a>(n: &Notif) -> Element<'a, Message> {
    let cream = libluma::tokens::to_iced(libluma::tokens::semantic_default().text);
    let secondary =
        libluma::tokens::to_iced(libluma::tokens::semantic_default().text_secondary);
    let accent = libluma::tokens::accent();

    let mut lines: Vec<Element<Message>> = Vec::new();
    if !n.app.trim().is_empty() {
        lines.push(text(n.app.clone()).size(11.0).color(accent).into());
    }
    if !n.summary.trim().is_empty() {
        lines.push(text(n.summary.clone()).size(14.0).color(cream).into());
    }
    if !n.body.trim().is_empty() {
        lines.push(text(n.body.clone()).size(12.0).color(secondary).into());
    }

    container(column(lines).spacing(3))
        .width(Length::Fill)
        .padding(Padding::from([8.0, 10.0]))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(Color { a: 0.06, ..cream })),
            border: Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn subscription(state: &Bar) -> iced::Subscription<Message> {
    let mut subs = vec![
        iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick),
        // Poll da lista de janelas (≤500ms de latência — "ao vivo" o bastante
        // pro passo 2; um push-signal dá pra adicionar depois).
        iced::time::every(Duration::from_millis(500)).map(|_| Message::PollWindows),
        iced::Subscription::run(accent_subscription),
        iced::Subscription::run(high_contrast_subscription),
        iced::Subscription::run(battery_subscription),
        iced::Subscription::run(volume_subscription),
        iced::Subscription::run(network_subscription),
        iced::Subscription::run(bluetooth_subscription),
    ];
    // Modo adicionar: poll dos descobertos + fase do pareamento a cada 2s.
    if state.bt_adding {
        subs.push(iced::time::every(Duration::from_millis(2000)).map(|_| Message::BtPoll));
    }
    // Popup de rede aberto: relê a lista a cada 3s (o scan do wifi demora).
    if matches!(state.popup, Some((_, PopupKind::Network))) {
        subs.push(iced::time::every(Duration::from_millis(3000)).map(|_| Message::NetPoll));
    }
    // Sino: poll do status a cada 2s (sempre — pra o ícone refletir notif nova
    // / DND ao vivo); a lista só relê com a central aberta (no NotifPoll).
    subs.push(iced::time::every(Duration::from_millis(2000)).map(|_| Message::NotifPoll));
    iced::Subscription::batch(subs)
}

fn bluetooth_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::bluetooth_signal_stream().await {
            Ok(s) => s.map(Message::BluetoothChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de bluetooth falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn network_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::network_signal_stream().await {
            Ok(s) => s.map(Message::NetworkChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de rede falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn volume_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::volume_signal_stream().await {
            Ok(s) => s.map(Message::VolumeChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de volume falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn battery_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::battery_signal_stream().await {
            Ok(s) => s.map(Message::BatteryChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de bateria falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn accent_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::accent_signal_stream().await {
            Ok(s) => s.map(Message::AccentChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de acento falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn high_contrast_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match ipc::high_contrast_signal_stream().await {
            Ok(s) => s.map(Message::HighContrastChanged).boxed(),
            Err(e) => {
                eprintln!("luma-bar: subscription de HC falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

fn main() -> Result<(), iced_layershell::Error> {
    // Daemon (multi-surface): começa sem surface (Background); o boot abre a
    // barra (bar_settings) e o popup do volume abre sob demanda. Precisa ser
    // daemon pra abrir o popup — application só tem uma surface.
    let settings = Settings {
        layer_settings: LayerShellSettings {
            start_mode: StartMode::Background,
            ..Default::default()
        },
        ..Default::default()
    };

    iced_layershell::daemon(boot, "luma-bar", update, view)
        .settings(settings)
        .theme(|_state: &Bar, _id| libluma::golden_hour_theme())
        // Surface transparente: o "clear color" padrão é o fundo do tema
        // (escuro), que pintava a tela toda no popup. A barra pinta o próprio
        // fundo (container), então fica opaca; o popup deixa o wallpaper passar.
        .style(|_state: &Bar, theme: &iced::Theme| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        })
        .subscription(subscription)
        .run()
}
