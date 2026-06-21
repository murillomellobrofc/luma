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
use tracing::{error, info};
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
    let accessibility = match notifications.lock() {
        Ok(g) => crate::settings::Accessibility {
            visual_bell_enabled: g.flash_enabled,
            photosafe_enabled: g.flash_photosafe,
            flash_intensity: g.flash_intensity,
            reduce_motion: g.reduce_motion,
            high_contrast: g.high_contrast,
            larger_targets: g.larger_targets,
            min_target_size: g.min_target_size,
        },
        Err(_) => {
            error!("D-Bus: lock envenenado ao persistir acessibilidade");
            return;
        }
    };
    let settings = crate::settings::Settings { accessibility };
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
) {
    let builder = thread::Builder::new().name("luma-dbus".into());
    if let Err(err) = builder.spawn(move || run(cmd_tx, available, notifications)) {
        error!("D-Bus: não foi possível criar thread do servidor: {err}");
    }
}

fn run(cmd_tx: Sender<IpcCommand>, available: Vec<WidgetInfo>, notifications: NotificationsSlot) {
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
        if let Err(err) = serve(cmd_tx, available, notifications).await {
            error!("D-Bus: servidor encerrou com erro: {err}");
        }
    });
}

async fn serve(
    cmd_tx: Sender<IpcCommand>,
    available: Vec<WidgetInfo>,
    notifications: NotificationsSlot,
) -> zbus::Result<()> {
    let service = CompositorService {
        available_widgets: available,
        cmd_tx,
        notifications,
    };
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
