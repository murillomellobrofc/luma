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

use std::process::Command;

use smithay::utils::{Logical, Point, Rectangle, Size};

/// Ações disparadas por items do menu. H48.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    /// Abre file picker (kdialog/zenity) pra escolher qualquer imagem.
    ChangeWallpaperPick,
    /// Aplica um dos wallpapers shipados em `assets/wallpapers/`.
    /// O &'static str é o filename completo. H51.
    SetWallpaper(&'static str),
    ConfigureDisplay,
    OpenTerminal,
    OpenLumaSettings,
    /// Per-icon: abre o target via xdg-open. H52.
    OpenTarget,
    /// Per-icon: copia o path do target pro clipboard (wl-copy). H52.
    CopyPath,
    /// Per-icon: kdialog --inputbox pra novo nome → fs::rename. H52.
    Rename,
    /// Per-icon: gio trash <target> (fallback kioclient5). H52.
    MoveToTrash,
    /// Click num item de DBusMenu nativo — dispatch envia Event(id)
    /// pro app via `sni::send_menu_event`. H47c.
    DBusMenuClick(i32),
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
    pub label: String,
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
    /// Path do ícone-target quando o menu é per-icon (right-click num
    /// ícone do desktop). `None` quando é menu do desktop. H52.
    pub target: Option<std::path::PathBuf>,
    /// (service, menu_path) do DBusMenu quando o menu foi construído
    /// a partir de um item SNI. None pros outros menus. Items deste
    /// menu disparam `DBusMenuClick(id)` que chama
    /// `sni::send_menu_event(service, menu_path, id)`. H47c.
    pub dbus_menu: Option<(String, String)>,
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
            target: None,
            dbus_menu: None,
        }
    }

    /// Constrói o menu per-icon (right-click sobre um ícone do
    /// desktop). Items: Abrir / Copiar caminho / Renomear / Mover
    /// pra Lixeira. Caller deve `clamp_into` depois. H52.
    pub fn for_icon(cursor: Point<i32, Logical>, target: std::path::PathBuf) -> Self {
        Self {
            origin: cursor,
            items: icon_items(),
            hovered: None,
            open_submenu: None,
            submenu_hovered: None,
            target: Some(target),
            dbus_menu: None,
        }
    }

    /// Constrói o menu a partir de um DBusMenu já consultado (flat).
    /// `service` + `menu_path` ficam guardados pro dispatch da ação.
    /// H47c.
    pub fn for_dbusmenu(
        cursor: Point<i32, Logical>,
        service: String,
        menu_path: String,
        items: Vec<crate::sni::DBusMenuItem>,
    ) -> Self {
        let ctx_items: Vec<ContextItem> = items
            .into_iter()
            .map(|i| ContextItem {
                label: if i.separator {
                    String::new()
                } else {
                    i.label
                },
                icon: None,
                kind: if i.separator {
                    ItemKind::Separator
                } else {
                    ItemKind::Action(ContextAction::DBusMenuClick(i.id))
                },
                enabled: i.enabled,
                shortcut: None,
            })
            .collect();
        Self {
            origin: cursor,
            items: ctx_items,
            hovered: None,
            open_submenu: None,
            submenu_hovered: None,
            target: None,
            dbus_menu: Some((service, menu_path)),
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

/// Abre o terminal padrão em $HOME. Ordem: $TERMINAL → x-terminal-emulator
/// → konsole → gnome-terminal → foot → alacritty → kitty → xterm.
/// "Aqui" = $HOME enquanto não houver navegação de pastas no desktop
/// (debt em DEBT.md). H48.ctx.5.
pub fn open_terminal() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let candidates: Vec<String> = {
        let mut v = Vec::new();
        if let Ok(t) = std::env::var("TERMINAL") {
            if !t.is_empty() {
                v.push(t);
            }
        }
        for n in [
            "x-terminal-emulator",
            "konsole",
            "gnome-terminal",
            "foot",
            "alacritty",
            "kitty",
            "wezterm",
            "xterm",
        ] {
            v.push(n.to_string());
        }
        v
    };
    for term in candidates {
        if which(&term) {
            let mut cmd = Command::new(&term);
            cmd.current_dir(&home);
            match cmd.spawn() {
                Ok(_) => {
                    tracing::info!("ctx menu: abriu {} em {}", term, home);
                    return;
                }
                Err(e) => {
                    tracing::warn!("ctx menu: falha lançando {term}: {e}");
                }
            }
        }
    }
    tracing::warn!("ctx menu: nenhum terminal encontrado");
}

/// Items do menu per-icon. Abrir / Copiar caminho / Renomear / Mover
/// pra Lixeira. Reaproveita os ícones do pack ctx-* + alguns extras.
/// H52.
fn icon_items() -> Vec<ContextItem> {
    vec![
        ContextItem {
            label: "Abrir".into(),
            icon: Some("ctx-terminal"), // placeholder; sem SVG dedicado
            kind: ItemKind::Action(ContextAction::OpenTarget),
            enabled: true,
            shortcut: Some("Enter"),
        },
        ContextItem {
            label: "".into(),
            icon: None,
            kind: ItemKind::Separator,
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Copiar caminho".into(),
            icon: Some("ctx-paste"),
            kind: ItemKind::Action(ContextAction::CopyPath),
            enabled: true,
            shortcut: None,
        },
        ContextItem {
            label: "Renomear…".into(),
            icon: Some("ctx-edit"),
            kind: ItemKind::Action(ContextAction::Rename),
            enabled: true,
            shortcut: None,
        },
        ContextItem {
            label: "".into(),
            icon: None,
            kind: ItemKind::Separator,
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Mover pra Lixeira".into(),
            icon: Some("ctx-new"),
            kind: ItemKind::Action(ContextAction::MoveToTrash),
            enabled: true,
            shortcut: None,
        },
    ]
}

/// Executa `wl-copy <text>`. Sem dep nova — usa o binário do
/// wl-clipboard. Loga warn se ausente. H52.
pub fn copy_to_clipboard(text: &str) {
    let result = std::process::Command::new("wl-copy").arg(text).spawn();
    match result {
        Ok(_) => tracing::info!("ctx menu: clipboard <- {text}"),
        Err(e) => tracing::warn!(
            "ctx menu: wl-copy falhou ({e}) — instalar wl-clipboard"
        ),
    }
}

// H53: rename_target / rename_blocking / run_input_dialog (kdialog
// + thread) REMOVIDOS. Substituídos por campo inline editável no
// próprio ícone do desktop. Dispatch agora seta
// `state.desktop_renaming` em pointer.rs.

/// Move o target pra lixeira via `gio trash` (gvfs) ou
/// `kioclient5 move trash:/` (KDE 5/6). Falha silenciosa se nenhum
/// dos dois estiver instalado. Não usa `rm -rf` por segurança. H52.
pub fn move_to_trash(target: &std::path::Path) {
    if which("gio") {
        let out = std::process::Command::new("gio")
            .args(["trash", target.to_string_lossy().as_ref()])
            .status();
        match out {
            Ok(s) if s.success() => {
                tracing::info!("ctx menu: trash {target:?}");
                return;
            }
            Ok(s) => {
                tracing::warn!("ctx menu: gio trash exit {s:?}");
            }
            Err(e) => tracing::warn!("ctx menu: gio trash falhou: {e}"),
        }
    }
    for kioclient in &["kioclient5", "kioclient"] {
        if which(kioclient) {
            let out = std::process::Command::new(kioclient)
                .args(["move", target.to_string_lossy().as_ref(), "trash:/"])
                .status();
            match out {
                Ok(s) if s.success() => {
                    tracing::info!("ctx menu: {kioclient} trash {target:?}");
                    return;
                }
                Ok(s) => {
                    tracing::warn!("ctx menu: {kioclient} trash exit {s:?}");
                }
                Err(e) => tracing::warn!("ctx menu: {kioclient} falhou: {e}"),
            }
        }
    }
    tracing::warn!(
        "ctx menu: trash falhou — sem gio nem kioclient5 instalado"
    );
}

/// Items do submenu da galeria de wallpapers. 6 shipados +
/// "Escolher imagem..." que abre o file picker. H51.
pub fn wallpaper_gallery_items() -> Vec<ContextItem> {
    let entries: &[(&'static str, &'static str)] = &[
        ("Pôr do Sol", "luma-sunset-peaks-1920x1080.png"),
        ("Pôr do Sol Calmo", "luma-sunset-calm-1920x1080.png"),
        ("Gradiente Aurora", "luma-gradient-dawn-1920x1080.png"),
        ("Gradiente Crepúsculo", "luma-gradient-dusk-1920x1080.png"),
        ("Geometria Suave", "luma-geometric-soft-1920x1080.png"),
        ("Geometria Arcos", "luma-geometric-arcs-1920x1080.png"),
    ];
    let mut items: Vec<ContextItem> = entries
        .iter()
        .map(|(label, file)| ContextItem {
            label: label.to_string(),
            icon: Some("ctx-wallpaper"),
            kind: ItemKind::Action(ContextAction::SetWallpaper(file)),
            enabled: true,
            shortcut: None,
        })
        .collect();
    items.push(ContextItem {
        label: "".into(),
        icon: None,
        kind: ItemKind::Separator,
        enabled: false,
        shortcut: None,
    });
    items.push(ContextItem {
        label: "Escolher imagem…".into(),
        icon: Some("ctx-wallpaper"),
        kind: ItemKind::Action(ContextAction::ChangeWallpaperPick),
        enabled: true,
        shortcut: None,
    });
    items
}

/// Aplica um wallpaper shipado: copia o PNG pra
/// `~/.config/luma/wallpaper.png` (overwriting). Retorna o path
/// final pro caller fazer reload. H51.
pub fn apply_shipped_wallpaper(filename: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let src = std::path::PathBuf::from(format!("assets/wallpapers/{}", filename));
    if !src.exists() {
        tracing::warn!("ctx menu: wallpaper shipado ausente: {filename}");
        return None;
    }
    let cfg_dir = std::path::PathBuf::from(&home).join(".config/luma");
    let _ = std::fs::create_dir_all(&cfg_dir);
    // Remove .jpg/.jpeg pra não bagunçar; sempre escreve .png.
    for ext in ["jpg", "jpeg"] {
        let _ = std::fs::remove_file(cfg_dir.join(format!("wallpaper.{}", ext)));
    }
    let dst = cfg_dir.join("wallpaper.png");
    if let Err(e) = std::fs::copy(&src, &dst) {
        tracing::warn!("ctx menu: copy wallpaper falhou: {e}");
        return None;
    }
    tracing::info!("ctx menu: wallpaper aplicado de {filename}");
    Some(dst)
}

/// Abre o seletor de imagem (kdialog / zenity), copia o arquivo
/// escolhido pra `~/.config/luma/wallpaper.png` e dispara o reload.
/// Bloqueia até o user fechar o diálogo. H48.ctx.6.
pub fn change_wallpaper() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let pictures = format!("{}/Imagens", home);
    let pictures = if std::path::Path::new(&pictures).exists() {
        pictures
    } else {
        format!("{}/Pictures", home)
    };
    let path = if which("kdialog") {
        let out = Command::new("kdialog")
            .args([
                "--getopenfilename",
                &pictures,
                "Imagens (*.png *.jpg *.jpeg)",
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else if which("zenity") {
        let out = Command::new("zenity")
            .args([
                "--file-selection",
                &format!("--filename={}/", pictures),
                "--file-filter=Imagens | *.png *.jpg *.jpeg",
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        tracing::warn!("ctx menu: nenhum file picker (kdialog/zenity)");
        return None;
    };
    if path.is_empty() {
        return None;
    }
    let src = std::path::PathBuf::from(&path);
    if !src.exists() {
        return None;
    }
    // Copia pra ~/.config/luma/wallpaper.png (ou .jpg conforme ext).
    let cfg_dir = std::path::PathBuf::from(&home).join(".config/luma");
    let _ = std::fs::create_dir_all(&cfg_dir);
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let dst = cfg_dir.join(format!("wallpaper.{}", ext));
    // Remove os outros pra não ter ambiguidade na próxima leitura.
    for old_ext in ["png", "jpg", "jpeg"] {
        if old_ext != ext {
            let _ = std::fs::remove_file(cfg_dir.join(format!("wallpaper.{}", old_ext)));
        }
    }
    if let Err(e) = std::fs::copy(&src, &dst) {
        tracing::warn!("ctx menu: copy wallpaper falhou: {e}");
        return None;
    }
    tracing::info!("ctx menu: wallpaper copiado pra {dst:?}");
    Some(dst)
}

/// Abre o theme.toml do user em editor padrão via xdg-open. Cria o
/// arquivo vazio se não existir. H48.ctx.7.
pub fn open_luma_settings() {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return,
    };
    let cfg_dir = std::path::PathBuf::from(&home).join(".config/luma");
    let _ = std::fs::create_dir_all(&cfg_dir);
    let theme_path = cfg_dir.join("theme.toml");
    if !theme_path.exists() {
        let _ = std::fs::write(
            &theme_path,
            "# Theme overrides do Luma. Veja src/theme.rs pros tokens.\n",
        );
    }
    let _ = Command::new("xdg-open").arg(&theme_path).spawn();
    tracing::info!("ctx menu: xdg-open {:?}", theme_path);
}

fn which(bin: &str) -> bool {
    Command::new("which")
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Os 8 items do menu (+ 2 separators) da spec H48. ATIVOS:
/// Mudar papel de parede, Abrir terminal, Configurações do Luma.
/// DIM: Configurar tela, Criar novo, Colar, Organizar ícones, Modo
/// de edição.
fn default_items() -> Vec<ContextItem> {
    vec![
        ContextItem {
            label: "Criar novo".into(),
            icon: Some("ctx-new"),
            kind: ItemKind::Submenu(vec![
                ContextItem {
                    label: "Pasta".into(),
                    icon: Some("ctx-new"),
                    kind: ItemKind::Action(ContextAction::Disabled),
                    enabled: false,
                    shortcut: None,
                },
                ContextItem {
                    label: "Arquivo de texto".into(),
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
            label: "Colar".into(),
            icon: Some("ctx-paste"),
            kind: ItemKind::Action(ContextAction::Disabled),
            enabled: false,
            shortcut: Some("Ctrl+V"),
        },
        ContextItem {
            label: "Organizar ícones".into(),
            icon: Some("ctx-arrange"),
            kind: ItemKind::Submenu(vec![
                ContextItem {
                    label: "Por nome".into(),
                    icon: None,
                    kind: ItemKind::Action(ContextAction::Disabled),
                    enabled: false,
                    shortcut: None,
                },
                ContextItem {
                    label: "Por tipo".into(),
                    icon: None,
                    kind: ItemKind::Action(ContextAction::Disabled),
                    enabled: false,
                    shortcut: None,
                },
                ContextItem {
                    label: "Por data".into(),
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
            label: "Modo de edição".into(),
            icon: Some("ctx-edit"),
            kind: ItemKind::Action(ContextAction::Disabled),
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "".into(),
            icon: None,
            kind: ItemKind::Separator,
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Abrir terminal aqui".into(),
            icon: Some("ctx-terminal"),
            kind: ItemKind::Action(ContextAction::OpenTerminal),
            enabled: true,
            shortcut: None,
        },
        ContextItem {
            label: "Mudar papel de parede".into(),
            icon: Some("ctx-wallpaper"),
            kind: ItemKind::Submenu(wallpaper_gallery_items()),
            enabled: true,
            shortcut: None,
        },
        ContextItem {
            label: "".into(),
            icon: None,
            kind: ItemKind::Separator,
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Configurar tela".into(),
            icon: Some("ctx-display"),
            kind: ItemKind::Action(ContextAction::ConfigureDisplay),
            enabled: false,
            shortcut: None,
        },
        ContextItem {
            label: "Configurações do Luma".into(),
            icon: Some("ctx-settings"),
            kind: ItemKind::Action(ContextAction::OpenLumaSettings),
            enabled: true,
            shortcut: None,
        },
    ]
}
