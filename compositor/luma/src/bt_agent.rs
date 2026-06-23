//! Agent BlueZ (org.bluez.Agent1) pro pareamento com confirmação de código.
//! Registrado como default agent no system bus; o BlueZ chama nossos métodos
//! durante o `Device1.Pair()`. O caso moderno é `RequestConfirmation` (mostra
//! um código, o aparelho mostra o mesmo, o usuário confirma que batem).
//!
//! Coordenação com o popup (sem sinal cross-bus): tudo via `PairingSlot`
//! compartilhado. O agent põe a fase `Confirm(passkey)` + guarda um oneshot;
//! o popup mostra o código e chama `ConfirmPairing(bool)` (session bus), que
//! preenche o oneshot e o `RequestConfirmation` resolve.

use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;
use zbus::interface;
use zbus::zvariant::ObjectPath;

/// Em que pé está o pareamento (lido pelo popup via `PairingState`).
#[derive(Debug, Clone)]
pub enum Phase {
    /// Nada em andamento.
    Idle,
    /// `Device1.Pair()` em curso.
    Pairing,
    /// BlueZ pediu confirmação do código `passkey` — espera o usuário.
    Confirm(u32),
    /// Pareou (e tentou trust + connect).
    Done,
    /// Falhou, com o motivo.
    Failed(String),
}

impl Default for Phase {
    fn default() -> Self {
        Phase::Idle
    }
}

/// Estado compartilhado entre o agent (system bus), os comandos de pareamento
/// e o `ConfirmPairing` (session bus).
#[derive(Default)]
pub struct Pairing {
    pub phase: Phase,
    /// Canal pra responder o `RequestConfirmation` pendente (confirma/cancela).
    pub confirm_tx: Option<oneshot::Sender<bool>>,
}

pub type PairingSlot = Arc<Mutex<Pairing>>;

/// Código numérico da fase atual (pro `PairingState` D-Bus):
/// 0 Idle, 1 Pairing, 2 Confirm, 3 Done, 4 Failed.
pub fn phase_tuple(slot: &PairingSlot) -> (u8, u32, String) {
    let g = slot.lock().unwrap();
    match &g.phase {
        Phase::Idle => (0, 0, String::new()),
        Phase::Pairing => (1, 0, String::new()),
        Phase::Confirm(p) => (2, *p, String::new()),
        Phase::Done => (3, 0, String::new()),
        Phase::Failed(m) => (4, 0, m.clone()),
    }
}

/// Responde o `RequestConfirmation` pendente (chamado pelo `ConfirmPairing`).
pub fn answer(slot: &PairingSlot, ok: bool) {
    if let Some(tx) = slot.lock().unwrap().confirm_tx.take() {
        let _ = tx.send(ok);
    }
}

/// Volta a fase pra ociosa (ao sair do modo adicionar / após ver o resultado).
pub fn reset(slot: &PairingSlot) {
    let mut g = slot.lock().unwrap();
    g.confirm_tx = None;
    g.phase = Phase::Idle;
}

/// O objeto Agent1 servido no system bus.
pub struct Agent {
    pub slot: PairingSlot,
}

#[interface(name = "org.bluez.Agent1")]
impl Agent {
    /// BlueZ chama ao desregistrar o agent.
    async fn release(&self) {}

    /// O caso moderno (Just Works numérico): mostra o código e espera o
    /// usuário confirmar que bate com o do aparelho.
    async fn request_confirmation(
        &self,
        _device: ObjectPath<'_>,
        passkey: u32,
    ) -> zbus::fdo::Result<()> {
        let (tx, rx) = oneshot::channel();
        {
            let mut g = self.slot.lock().unwrap();
            g.phase = Phase::Confirm(passkey);
            g.confirm_tx = Some(tx);
        }
        match rx.await {
            Ok(true) => Ok(()),
            _ => Err(zbus::fdo::Error::Failed("pareamento recusado".into())),
        }
    }

    /// Autoriza o aparelho/serviço automaticamente (já confirmado no pair).
    async fn request_authorization(&self, _device: ObjectPath<'_>) -> zbus::fdo::Result<()> {
        Ok(())
    }

    async fn authorize_service(
        &self,
        _device: ObjectPath<'_>,
        _uuid: String,
    ) -> zbus::fdo::Result<()> {
        Ok(())
    }

    /// Aparelho mostra um passkey que a gente só exibe (sem confirmar). Trata
    /// como Confirm pra o popup mostrar o código.
    async fn display_passkey(&self, _device: ObjectPath<'_>, passkey: u32, _entered: u16) {
        self.slot.lock().unwrap().phase = Phase::Confirm(passkey);
    }

    async fn display_pin_code(
        &self,
        _device: ObjectPath<'_>,
        _pincode: String,
    ) -> zbus::fdo::Result<()> {
        Ok(())
    }

    /// PIN/passkey DIGITADO (aparelhos legados) — não coletamos entrada de
    /// texto nesse passo; recusa com motivo claro.
    async fn request_pin_code(&self, _device: ObjectPath<'_>) -> zbus::fdo::Result<String> {
        Err(zbus::fdo::Error::Failed(
            "aparelho pede PIN digitado (não suportado ainda)".into(),
        ))
    }

    async fn request_passkey(&self, _device: ObjectPath<'_>) -> zbus::fdo::Result<u32> {
        Err(zbus::fdo::Error::Failed(
            "aparelho pede passkey digitado (não suportado ainda)".into(),
        ))
    }

    /// BlueZ cancelou a operação (timeout, aparelho sumiu).
    async fn cancel(&self) {
        let mut g = self.slot.lock().unwrap();
        g.confirm_tx = None;
        g.phase = Phase::Failed("cancelado".into());
    }
}
