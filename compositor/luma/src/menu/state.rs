//! State do menu fullscreen híbrido. H32.10.3.
//!
//! Substitui (a partir de H32.10.17) o `legacy::MenuState` Win7 que
//! tinha tudo num struct gigante. Aqui o state é dividido por zona
//! (Search, FavGrid, AllGrid, Session, Places) e o modo (Favorites
//! default ou AllApps via Tab) controla qual grid é visível.

use smithay::backend::renderer::Color32F;

/// Modo do menu: grid de favoritos curados (default) ou grid completo
/// de apps instalados (via Tab). Search é independente — qualquer
/// query != "" substitui o grid por lista vertical de resultados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuMode {
    /// Grid 5×2 com 10 slots curados do TOML. Estado inicial ao abrir.
    Favorites,
    /// Grid 8×4 paginado com todos os `.desktop` instalados, ordem
    /// alfabética. Entra via Tab ou click no link "TODOS OS APPS →".
    AllApps,
}

impl Default for MenuMode {
    fn default() -> Self {
        MenuMode::Favorites
    }
}

/// Zona com foco de teclado. Setas/Enter/Esc agem conforme a zona.
/// PCD a11y: tudo é navegável só por teclado, foco sempre visível.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedZone {
    /// Search bar (default ao abrir — qualquer letra digitada filtra).
    Search,
    /// Grid de favoritos 5×2.
    FavGrid,
    /// Grid de todos os apps 8×4 paginado.
    AllGrid,
    /// Session bar embaixo (5 botões). Reservada — nav atual cobre só
    /// Search/FavGrid/AllGrid; expansão pra Session vem depois.
    #[allow(dead_code)]
    Session,
    /// Mini-places à direita da session bar (4 ícones). Reservada (idem).
    #[allow(dead_code)]
    Places,
}

impl Default for FocusedZone {
    fn default() -> Self {
        FocusedZone::Search
    }
}

/// 5 botões da session bar, na ordem esquerda→direita do spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionButton {
    Lock,
    Logout,
    Sleep,
    Restart,
    Power,
}

/// Slot curado do grid de favoritos. Cor do ícone vem do TOML, NÃO
/// do `.desktop`. Se nenhum dos `desktop_ids` candidatos estiver
/// instalado, slot vira "+" tracejado.
#[derive(Debug, Clone)]
pub struct CuratedFavorite {
    /// Posição no grid 5×2 (1..=10, esquerda→direita / topo→base).
    /// Reservado pra reordenação manual de favoritos (drag-and-drop futuro).
    #[allow(dead_code)]
    pub slot: usize,
    /// Label embaixo do ícone (max ~12 chars).
    pub label: String,
    /// Cor sólida do quadrado 80×80 (canto round 18).
    pub icon_color: Color32F,
    /// Cor opcional do quadrado interno menor (se Some, desenha um
    /// quadrado de tamanho 0.45× centralizado dentro do icon_color).
    pub icon_inner_color: Option<Color32F>,
    /// Lista de desktop IDs candidatos a resolver pra esse slot, em
    /// ordem de preferência. Ex: ["org.kde.dolphin", "nautilus", ...].
    /// O primeiro instalado ganha.
    pub desktop_ids: Vec<&'static str>,
}

impl CuratedFavorite {
    /// Defaults dos 10 slots conforme brief H32.10. Mantidos no
    /// código por enquanto. Quando o parser de `[favorites.curated]`
    /// do theme.toml entrar, esses viram fallback.
    pub fn defaults() -> Vec<Self> {
        use smithay::backend::renderer::Color32F as C;
        let rgb = |r, g, b| C::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            1.0,
        );
        vec![
            Self {
                slot: 1,
                label: "Arquivos".into(),
                icon_color: rgb(0xD9, 0x77, 0x57), // Coral
                icon_inner_color: None,
                desktop_ids: vec![
                    "org.kde.dolphin", "nautilus", "nemo", "thunar",
                ],
            },
            Self {
                slot: 2,
                label: "Navegador".into(),
                icon_color: rgb(0x6B, 0x8E, 0x7F), // Sage
                icon_inner_color: None,
                desktop_ids: vec!["brave-browser", "firefox", "chromium"],
            },
            Self {
                slot: 3,
                label: "Terminal".into(),
                icon_color: rgb(0x1F, 0x16, 0x11), // Coffee
                icon_inner_color: Some(rgb(0xE8, 0x9B, 0x3D)), // Amber
                desktop_ids: vec!["foot", "Alacritty", "org.kde.konsole", "kitty"],
            },
            Self {
                slot: 4,
                label: "Editor".into(),
                icon_color: rgb(0xFB, 0xF6, 0xEE), // Cream
                icon_inner_color: None,
                desktop_ids: vec!["code", "org.kde.kate", "org.gnome.gedit", "sublime_text"],
            },
            Self {
                slot: 5,
                label: "Calc".into(),
                icon_color: rgb(0x9F, 0x4A, 0x35), // Rust
                icon_inner_color: None,
                desktop_ids: vec!["org.kde.kcalc", "gnome-calculator"],
            },
            Self {
                slot: 6,
                label: "Player".into(),
                icon_color: rgb(0xE8, 0x9B, 0x3D), // Amber
                icon_inner_color: None,
                desktop_ids: vec!["vlc", "mpv", "spotify"],
            },
            Self {
                slot: 7,
                label: "Fotos".into(),
                icon_color: rgb(0xD9, 0x77, 0x57), // Coral
                icon_inner_color: None,
                desktop_ids: vec!["org.kde.gwenview", "org.gnome.Loupe", "eog"],
            },
            Self {
                slot: 8,
                label: "Ajustes".into(),
                icon_color: rgb(0x6B, 0x8E, 0x7F), // Sage
                icon_inner_color: None,
                desktop_ids: vec!["systemsettings", "gnome-control-center"],
            },
            Self {
                slot: 9,
                label: "Código".into(),
                icon_color: rgb(0x2A, 0x1F, 0x1A), // Coffee dark
                icon_inner_color: Some(rgb(0xE8, 0x9B, 0x3D)), // Amber
                desktop_ids: vec!["code", "code-oss"],
            },
            Self {
                slot: 10,
                label: "Discord".into(),
                icon_color: rgb(0x58, 0x65, 0xF2), // Discord blurple
                icon_inner_color: None,
                desktop_ids: vec!["discord", "com.discordapp.Discord"],
            },
        ]
    }
}

/// State do menu fullscreen híbrido. Coexiste com `legacy::MenuState`
/// até H32.10.17 quando o legacy é deletado. Nome distinto evita
/// conflito com o re-export em `mod.rs`.
#[derive(Debug)]
pub struct FullscreenMenuState {
    pub mode: MenuMode,
    pub focused_zone: FocusedZone,
    pub search_query: String,

    /// Grid de favoritos 5×2.
    pub fav_selected: usize,
    pub fav_items: Vec<CuratedFavorite>,

    /// Grid de todos os apps 8×4 paginado.
    pub all_apps_page: usize,
    /// (col, row) dentro do grid 8×4.
    pub all_apps_selected: (usize, usize),

    /// Index na lista vertical de resultados quando search_query != "".
    pub search_selected: usize,
    /// Índices em `state.apps` casados pela query atual (top 6 fuzzy).
    /// Vazio quando search_query == "".
    pub search_results: Vec<usize>,

    /// Botão da session bar com hover/foco visual.
    pub session_hovered: Option<SessionButton>,
    /// Mini-place à direita da session bar sob o cursor (0..4).
    pub place_hovered: Option<usize>,
    /// Mouse sobre o link "TODOS OS APPS" / "← FAVORITOS".
    pub link_hovered: bool,
    /// Slot do grid 5×2 sob o cursor (None = sem hover). Pinta glow
    /// âmbar igual ao keyboard selected pra dar feedback de mouse.
    pub fav_hovered: Option<usize>,
    /// Slot do grid 8×4 sob o cursor (idx no state.apps absoluto).
    pub all_hovered: Option<usize>,
    /// Idx do item da lista de search results sob o cursor.
    pub search_hovered: Option<usize>,
    /// Idx do favorito em "click pressed" (visual feedback de press).
    /// Liga em mouse down, desliga em mouse up + launch.
    pub fav_pressed: Option<usize>,
    /// Mouse sobre a pill da search bar. Ativa borda âmbar 2.5px.
    pub search_bar_hovered: bool,
    /// User clicou na pill da search bar ou já tem query. Liga o
    /// cursor "|" piscante. Reseta quando user clica em outra zone.
    pub search_bar_active: bool,
    /// Ação pendente de confirmação (Logout/Restart/Power). Quando
    /// `Some`, render mostra modal central. Lock/Sleep executam
    /// direto sem confirmação.
    pub pending_action: Option<SessionButton>,
}

impl FullscreenMenuState {
    pub fn new() -> Self {
        Self {
            mode: MenuMode::default(),
            focused_zone: FocusedZone::default(),
            search_query: String::new(),
            fav_selected: 0,
            fav_items: CuratedFavorite::defaults(),
            all_apps_page: 0,
            all_apps_selected: (0, 0),
            search_selected: 0,
            search_results: Vec::new(),
            session_hovered: None,
            place_hovered: None,
            link_hovered: false,
            fav_hovered: None,
            all_hovered: None,
            search_hovered: None,
            fav_pressed: None,
            search_bar_hovered: false,
            search_bar_active: false,
            pending_action: None,
        }
    }

    /// Reseta state ao fechar o menu (próxima abertura volta pro default).
    pub fn reset(&mut self) {
        self.mode = MenuMode::default();
        self.focused_zone = FocusedZone::default();
        self.search_query.clear();
        self.search_results.clear();
        self.fav_selected = 0;
        self.all_apps_page = 0;
        self.all_apps_selected = (0, 0);
        self.search_selected = 0;
        self.session_hovered = None;
        self.place_hovered = None;
        self.link_hovered = false;
        self.fav_hovered = None;
        self.all_hovered = None;
        self.search_hovered = None;
        self.fav_pressed = None;
        self.search_bar_hovered = false;
        self.search_bar_active = false;
        self.pending_action = None;
    }

    /// Recomputa search_results pra query atual usando fuzzy match.
    pub fn refresh_search_results(&mut self, apps: &[crate::apps::DesktopApp]) {
        self.search_results = crate::menu::search_results::match_query(apps, &self.search_query);
        if self.search_selected >= self.search_results.len() {
            self.search_selected = 0;
        }
    }
}

impl Default for FullscreenMenuState {
    fn default() -> Self {
        Self::new()
    }
}
