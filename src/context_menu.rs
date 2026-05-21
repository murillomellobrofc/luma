//! Menu de contexto do desktop. H48.
//!
//! Abre no right-click sobre área vazia do desktop (sem janela/tasbar/
//! popup sob o cursor). Posiciona no cursor, clamped pra ficar dentro
//! do output. Esc / clique fora fecha.
//!
//! Lista vertical de items 40px com ícone 16 + label + (opcional)
//! atalho ou setinha de submenu. Items podem ser:
//! - Action: dispatcha algo (terminal, wallpaper, settings)
//! - Submenu: abre lista filha à direita (sem ações ainda, só visual)
//! - Separator: linha 1px com padding
//!
//! Items `enabled = false` renderizam em `item_disabled` e não
//! reagem a hover/click.

use smithay::utils::{Logical, Point, Rectangle, Size};

/// Ações disparadas por items do menu. H48.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    ChangeWallpaper,
    ConfigureDisplay,
    OpenTerminal,
    OpenLumaSettings,
    /// Placeholder pros 4 items que dependem de ícones no desktop.
    /// Nunca dispatcha (item fica enabled=false).
    Disabled,
}

/// Tipo de entrada no menu.
#[derive(Debug, Clone)]
pub enum ItemKind {
    Action(ContextAction),
    /// Separator (linha horizontal entre grupos).
    Separator,
    /// Item que abre submenu à direita ao passar o mouse.
    /// Vec<ContextItem> = entradas do submenu (1 nível só na spec).
    Submenu(Vec<ContextItem>),
}

/// Item individual do menu.
#[derive(Debug, Clone)]
pub struct ContextItem {
    pub label: &'static str,
    /// Nome do SVG no `icons_path`, sem prefixo `icon-` nem extensão.
    /// `None` = sem ícone (espaço reservado mantido pra alinhamento).
    pub icon: Option<&'static str>,
    pub kind: ItemKind,
    pub enabled: bool,
    /// Atalho de teclado a renderizar à direita (cinza). H48 não
    /// vincula esses atalhos — meramente visual por enquanto.
    pub shortcut: Option<&'static str>,
}

impl ContextItem {
    /// `true` quando o item é interativo (não-separator + enabled).
    pub fn interactive(&self) -> bool {
        !matches!(self.kind, ItemKind::Separator) && self.enabled
    }
}

/// Estado do menu de contexto ativo. H48.
#[derive(Debug, Clone)]
pub struct ContextMenu {
    /// Top-left do menu em coords lógicas do output.
    pub origin: Point<i32, Logical>,
    pub items: Vec<ContextItem>,
    /// Index do item sob hover (None = nenhum). H48.ctx.2.
    pub hovered: Option<usize>,
    /// Index do item-Submenu cujo submenu está aberto. H48.ctx.4.
    pub open_submenu: Option<usize>,
    /// Index do item do submenu sob hover (None = nenhum).
    pub submenu_hovered: Option<usize>,
}

impl ContextMenu {
    /// Cria o menu default (8 items + 2 separators) ancorado em
    /// `cursor`. Clamp depois via `clamped_origin`. H48.ctx.1-3.
    pub fn new_at(cursor: Point<i32, Logical>) -> Self {
        Self {
            origin: cursor,
            items: default_items(),
            hovered: None,
            open_submenu: None,
            submenu_hovered: None,
        }
    }

    /// Altura total do menu (soma de items + separators).
    pub fn height(&self, tokens: &crate::theme::ContextMenuTokens) -> i32 {
        let mut h = 0;
        for item in &self.items {
            h += slot_height(item, tokens);
        }
        // Pad vertical pequeno top/bottom.
        h + 8
    }

    /// Recalcula `origin` pra ficar inteiro dentro do output. H48.ctx.1.
    pub fn clamp_into(&mut self, output_size: Size<i32, Logical>, tokens: &crate::theme::ContextMenuTokens) {
        let w = tokens.width;
        let h = self.height(tokens);
        let max_x = output_size.w - 8 - w;
        let max_y = output_size.h - 8 - h;
        if self.origin.x > max_x {
            self.origin.x = max_x.max(8);
        }
        if self.origin.y > max_y {
            self.origin.y = max_y.max(8);
        }
        if self.origin.x < 8 {
            self.origin.x = 8;
        }
        if self.origin.y < 8 {
            self.origin.y = 8;
        }
    }

    /// Bounding box do menu principal. H48.ctx.1.
    pub fn rect(&self, tokens: &crate::theme::ContextMenuTokens) -> Rectangle<i32, Logical> {
        Rectangle::new(
            self.origin,
            Size::from((tokens.width, self.height(tokens))),
        )
    }

    /// Hit-test contra items do menu principal. Retorna index do
    /// item sob `point`, ou `None`. Pula separators.
    pub fn hit_test(
        &self,
        tokens: &crate::theme::ContextMenuTokens,
        point: Point<f64, Logical>,
    ) -> Option<usize> {
        let main = self.rect(tokens);
        if !main.to_f64().contains(point) {
            return None;
        }
        let mut y = self.origin.y + 4;
        for (idx, item) in self.items.iter().enumerate() {
            let h = slot_height(item, tokens);
            if point.y >= y as f64 && point.y < (y + h) as f64 {
                if matches!(item.kind, ItemKind::Separator) {
                    return None;
                }
                return Some(idx);
            }
            y += h;
        }
        None
    }

    /// Bounding box do submenu (se aberto). À direita do item-pai;
    /// se não couber, à esquerda. H48.ctx.4.
    pub fn submenu_rect(
        &self,
        tokens: &crate::theme::ContextMenuTokens,
        output_size: Size<i32, Logical>,
    ) -> Option<Rectangle<i32, Logical>> {
        let idx = self.open_submenu?;
        let item = self.items.get(idx)?;
        let entries = match &item.kind {
            ItemKind::Submenu(v) => v,
            _ => return None,
        };
        let main = self.rect(tokens);
        let mut y = self.origin.y + 4;
        for it in self.items.iter().take(idx) {
            y += slot_height(it, tokens);
        }
        // Altura do submenu.
        let sub_h: i32 = entries.iter().map(|i| slot_height(i, tokens)).sum::<i32>() + 8;
        // X à direita; cai pra esquerda se não couber.
        let pref_x = main.loc.x + main.size.w + 2;
        let x = if pref_x + tokens.width <= output_size.w - 8 {
            pref_x
        } else {
            main.loc.x - tokens.width - 2
        };
        // Y clamped pra caber.
        let max_y = output_size.h - 8 - sub_h;
        let y_clamped = y.min(max_y).max(8);
        Some(Rectangle::new(
            Point::from((x, y_clamped)),
            Size::from((tokens.width, sub_h)),
        ))
    }

    /// Hit-test no submenu (se aberto). Retorna index dentro do
    /// vec do submenu.
    pub fn submenu_hit_test(
        &self,
        tokens: &crate::theme::ContextMenuTokens,
        output_size: Size<i32, Logical>,
        point: Point<f64, Logical>,
    ) -> Option<usize> {
        let rect = self.submenu_rect(tokens, output_size)?;
        if !rect.to_f64().contains(point) {
            return None;
        }
        let item = self.items.get(self.open_submenu?)?;
        let entries = match &item.kind {
            ItemKind::Submenu(v) => v,
            _ => return None,
        };
        let mut y = rect.loc.y + 4;
        for (i, e) in entries.iter().enumerate() {
            let h = slot_height(e, tokens);
            if point.y >= y as f64 && point.y < (y + h) as f64 {
                if matches!(e.kind, ItemKind::Separator) {
                    return None;
                }
                return Some(i);
            }
            y += h;
        }
        None
    }
}

/// Altura do "slot" alocado por item — items normais usam `item_height`,
/// separators ocupam só `separator_height + 2*separator_padding`.
pub fn slot_height(item: &ContextItem, tokens: &crate::theme::ContextMenuTokens) -> i32 {
    match item.kind {
        ItemKind::Separator => tokens.separator_height + 2 * tokens.separator_padding,
        _ => tokens.item_height,
    }
}

/// Os 8 items do menu (+ 2 separators) da spec H48. ATIVOS:
/// Mudar papel de parede, Abrir terminal, Configurações do Luma.
/// DIM: Configurar tela, Criar novo, Colar, Organizar ícones, Modo
/// de edição.
fn default_items() -> Vec<ContextItem> {
    vec![
        ContextItem {
            label: "Criar novo",
            icon: Some("ctx-new"),
            kind: ItemKind::Submenu(vec![
                ContextItem {
                    label: "Pasta",
                    icon: Some("ctx-new"),
                    kind: ItemKind::Action(ContextAction::Disabled),
                    enabled: false,
                    shortcut: None,
                },
                ContextItem {
                    label: "Arquivo de texto",
                    icon: Some("ctx-new"),
                    kind: ItemKind::Action(ContextAction::Disabled),
                    enabled: false,
                    shortcut: None,
                },
            ]),
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Colar",
            icon: Some("ctx-paste"),
            kind: ItemKind::Action(ContextAction::Disabled),
            enabled: false,
            shortcut: Some("Ctrl+V"),
        },
        ContextItem {
            label: "Organizar ícones",
            icon: Some("ctx-arrange"),
            kind: ItemKind::Submenu(vec![
                ContextItem {
                    label: "Por nome",
                    icon: None,
                    kind: ItemKind::Action(ContextAction::Disabled),
                    enabled: false,
                    shortcut: None,
                },
                ContextItem {
                    label: "Por tipo",
                    icon: None,
                    kind: ItemKind::Action(ContextAction::Disabled),
                    enabled: false,
                    shortcut: None,
                },
                ContextItem {
                    label: "Por data",
                    icon: None,
                    kind: ItemKind::Action(ContextAction::Disabled),
                    enabled: false,
                    shortcut: None,
                },
            ]),
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Modo de edição",
            icon: Some("ctx-edit"),
            kind: ItemKind::Action(ContextAction::Disabled),
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "",
            icon: None,
            kind: ItemKind::Separator,
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Abrir terminal aqui",
            icon: Some("ctx-terminal"),
            kind: ItemKind::Action(ContextAction::OpenTerminal),
            enabled: true,
            shortcut: None,
        },
        ContextItem {
            label: "Mudar papel de parede…",
            icon: Some("ctx-wallpaper"),
            kind: ItemKind::Action(ContextAction::ChangeWallpaper),
            enabled: true,
            shortcut: None,
        },
        ContextItem {
            label: "",
            icon: None,
            kind: ItemKind::Separator,
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Configurar tela",
            icon: Some("ctx-display"),
            kind: ItemKind::Action(ContextAction::ConfigureDisplay),
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Configurações do Luma",
            icon: Some("ctx-settings"),
            kind: ItemKind::Action(ContextAction::OpenLumaSettings),
            enabled: true,
            shortcut: None,
        },
    ]
}
