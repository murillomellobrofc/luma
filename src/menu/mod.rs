//! Menu Luma — fullscreen híbrido (Launchpad + Spotlight + PCD). H32.10.
//!
//! Substitui o antigo menu Win7 2-colunas (FAVORITOS/LUGARES) por um
//! menu fullscreen com wallpaper escurecido, search GIGANTE no centro,
//! grid 5×2 de favoritos curados e session bar embaixo com 5 botões.
//!
//! Acessibilidade PCD é first-class: hit targets ≥ 44px (WCAG AAA),
//! auto-focus na search, ícones 80px, focus ring sempre visível.
//!
//! ## Arquitetura
//!
//! - **Bloco A — Estrutura fullscreen** (H32.10.1-5)
//!   - `state`            MenuState + MenuMode + FocusedZone
//!   - `render`           layout das 5 zonas + backdrop
//!   - `keyboard`         Tab toggla modos, Esc volta/limpa/fecha
//!   - `animation`        fade-in 200ms / fade-out 150ms
//!
//! - **Bloco B — Conteúdo (Favoritos + Search + Greeting)** (H32.10.6-9)
//!   - `greeting_corner`  avatar + nome + saudação dinâmica
//!   - `search_bar`       pill 64×500 com auto-focus
//!   - `favorites_grid`   5×2 com slots curados do TOML
//!
//! - **Bloco C — Todos os Apps** (H32.10.10-12)
//!   - `all_apps_grid`    8×4 paginado com .desktop reais
//!
//! - **Bloco D — Session bar** (H32.10.13-15)
//!   - `session_bar`      5 botões + hint + 4 mini-places
//!   - `icon_loader`      SVG → MemoryRenderBuffer via resvg
//!
//! - **Bloco E — Polish + a11y** (H32.10.16-17)
//!   - search results lista vertical
//!   - validação final de a11y

// Bloco A — Estrutura fullscreen.
pub mod animation;
pub mod render;
pub mod state;

// Bloco B — Conteúdo (greeting + search + favoritos).
pub mod favorites_grid;
pub mod greeting_corner;
pub mod search_bar;

// Bloco C — Todos os Apps.
pub mod all_apps_grid;
pub mod all_apps_link;

// Bloco D — Session bar.
pub mod icon_loader;
pub mod session_bar;

// Bloco E — Polish (search results + modal de confirmação).
pub mod confirm_modal;
pub mod search_results;
