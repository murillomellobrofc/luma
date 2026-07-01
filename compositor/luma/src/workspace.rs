//! Workspaces dinâmicos do Luma. H36.
//!
//! Modelo simples: `WorkspaceManager` mantém um Vec<Workspace> e um
//! `current` index. Cada `Workspace` lista as janelas que pertencem a
//! ele via `WlSurface` (identificador estável da surface toplevel).
//!
//! Janela NÃO sabe em qual workspace está — workspace é dono da lista
//! (single source of truth). Quando uma janela fecha, o WorkspaceManager
//! remove de qualquer workspace que contiver e roda cleanup_empty.
//!
//! H36.1: apenas tipos e construção. Métodos vêm em H36.2.

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

/// Identificador estável de uma janela toplevel via sua WlSurface.
/// Sobrevive enquanto o cliente não destruir a surface.
pub type WindowKey = WlSurface;

#[derive(Debug)]
pub struct Workspace {
    /// ID único permanente — não muda quando workspaces são removidos
    /// e re-indexados. Use pra referências persistentes.
    pub id: u32,
    /// Surfaces das janelas que pertencem a esse workspace, em ordem
    /// de criação. Quando o workspace é o current, todas elas estão
    /// mapeadas no Space; quando não é, ficam com posição off-screen.
    pub windows: Vec<WindowKey>,
    /// Última surface focada nesse workspace. Restaurada no switch_to.
    pub focused_window: Option<WindowKey>,
}

impl Workspace {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            windows: Vec::new(),
            focused_window: None,
        }
    }
}

#[derive(Debug)]
pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub current: usize,
    next_id: u32,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            workspaces: vec![Workspace::new(0)],
            current: 0,
            next_id: 1,
        }
    }

    pub fn current(&self) -> &Workspace {
        &self.workspaces[self.current]
    }

    pub fn current_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.current]
    }

    pub fn count(&self) -> usize {
        self.workspaces.len()
    }

    /// Vai pro workspace de índice `idx`. No-op se inválido ou já current.
    /// H36.2.
    pub fn switch_to(&mut self, idx: usize) -> bool {
        if idx >= self.workspaces.len() || idx == self.current {
            return false;
        }
        self.current = idx;
        true
    }

    /// Cria novo workspace ao final E foca nele. Retorna o índice.
    /// H36.2.
    pub fn create(&mut self) -> usize {
        let new_idx = self.workspaces.len();
        self.workspaces.push(Workspace::new(self.next_id));
        self.next_id += 1;
        self.current = new_idx;
        new_idx
    }

    /// Remove workspaces vazios. SEMPRE mantém pelo menos 1.
    /// Re-localiza `current` pelo id pra permanecer no mesmo workspace
    /// quando vizinhos somem. H36.2.
    pub fn cleanup_empty(&mut self) {
        if self.workspaces.len() <= 1 {
            return;
        }
        let current_id = self.workspaces[self.current].id;
        // Mantém current sempre, mesmo vazio (pra não pular pra outro).
        self.workspaces
            .retain(|w| !w.windows.is_empty() || w.id == current_id);
        if self.workspaces.is_empty() {
            self.workspaces.push(Workspace::new(self.next_id));
            self.next_id += 1;
        }
        self.current = self
            .workspaces
            .iter()
            .position(|w| w.id == current_id)
            .unwrap_or(self.workspaces.len() - 1);
    }

    pub fn cycle_next(&mut self) {
        if self.workspaces.is_empty() {
            return;
        }
        self.current = (self.current + 1) % self.workspaces.len();
    }

    /// Espelho de `cycle_next` — ligado em Ctrl+Alt+←.
    pub fn cycle_prev(&mut self) {
        if self.workspaces.is_empty() {
            return;
        }
        self.current = if self.current == 0 {
            self.workspaces.len() - 1
        } else {
            self.current - 1
        };
    }

    /// Move uma janela do workspace atual (ou de qualquer um) pro
    /// workspace de índice `target`. Cria workspaces intermediários se
    /// necessário pra que `target` exista. H36.2.
    pub fn move_window_to_workspace(&mut self, key: &WindowKey, target: usize) {
        while self.workspaces.len() <= target {
            self.workspaces.push(Workspace::new(self.next_id));
            self.next_id += 1;
        }
        for ws in &mut self.workspaces {
            ws.windows.retain(|w| w != key);
            if ws.focused_window.as_ref() == Some(key) {
                ws.focused_window = ws.windows.last().cloned();
            }
        }
        self.workspaces[target].windows.push(key.clone());
        self.workspaces[target].focused_window = Some(key.clone());
    }

    /// Adiciona uma janela ao workspace atual. Chamado quando uma
    /// janela nasce. H36.2.
    pub fn add_to_current(&mut self, key: WindowKey) {
        let cur = self.current_mut();
        if !cur.windows.contains(&key) {
            cur.windows.push(key.clone());
        }
        cur.focused_window = Some(key);
    }

    /// Remove uma janela de QUALQUER workspace (chamado quando fecha).
    /// Roda cleanup_empty automaticamente. H36.2.
    pub fn remove_window(&mut self, key: &WindowKey) {
        for ws in &mut self.workspaces {
            ws.windows.retain(|w| w != key);
            if ws.focused_window.as_ref() == Some(key) {
                ws.focused_window = ws.windows.last().cloned();
            }
        }
        self.cleanup_empty();
    }
}

