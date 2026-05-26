# 🗺️ Luma — Roadmap

> **EN:** Wayland desktop environment in Rust + Smithay. Identity: Golden Hour
> (coffee / coral / honey / sage). Target: v1.0 as portfolio · ~2 year timeline
> · surprise drop.
>
> **PT:** Desktop Environment em Rust + Smithay (Wayland). Identidade: Golden Hour
> (coffee / coral / honey / sage). Meta: v1.0 como portfólio · timeline ~2 anos
> · surprise drop.

---

## ✅ Done / Já feito

### Compositor base / Base compositor

- **Smithay** (Wayland) — vendored fork em `vendor/smithay` com patch pro
  winit backend multi-window (H35). Features: `backend_winit`, `backend_drm`,
  `backend_gbm`, `backend_libinput`, `backend_udev`, `backend_session_libseat`,
  `renderer_gl`, `wayland_frontend`, `desktop`, `xwayland`.
- **Multi-monitor** via `outputs: Vec<Output>` em `LumaState`. `focused_output()`
  resolve por nome; `output_under_cursor(pos)` faz hit-test geométrico.
- **Multi-workspace** stateful: `WorkspaceManager` com janelas movidas pra
  off-screen (`-1_000_000, 0`) ao trocar, restauradas via `saved_positions:
  Vec<(WlSurface, Point)>` (Hash de WlSurface não é estável entre clones).
- **GLES2 renderer** com `custom_elements: Vec<LumaRenderElement<GlesRenderer>>`
  pra UI nativa. Smithay renderiza fim→começo do vec (índice 0 = front).
- **Rust 1.95 + edição 2024**. QA técnico: 0 bugs críticos.

### Interface

- **Menu fullscreen híbrido** (H32.10): backdrop Coffee 0.94 + linha âmbar 2px,
  search bar (`fontdue` rasterize + `fuzzy-matcher`), Favoritos 5×2,
  AllApps 8×4, session bar bottom (Logout/Restart/Power + 4 places).
  `MenuAnimation` fade-in/out 200ms ease_out.
- **Taskbar KDE-style** (H33) — 6 zonas: launcher · pager · janelas ·
  system tray (battery + wifi + bluetooth + volume + bell) · clock 2 linhas
  · show desktop. Gradient Coffee→Black.
- **Decoração SSD** (H_deco): titlebar com gradient + 2 cantos arred topo,
  botões 32×32 Golden Hour com SVG icons, hover glow âmbar.
- **Resolvedor de ícones**: cascata GTK Adwaita + KDE Breeze, 7557 SVGs
  cacheados (`HashMap<String, MemoryRenderBuffer>` por theme+size).
- **Pager** (H36): workspace switcher visual.

### Applets / OSD

- **Volume applet** (H44): popup com lista sinks/sources/streams via `wpctl`
  (PipeWire CLI, spawn ~5ms — sem dep da crate `pipewire`). Slider per-stream.
- **OSD volume + brilho** (H45): hooks de XF86 keys → overlay 1.5s.
- **OSD mídia** (H46): MPRIS now playing, polling 500ms enquanto visível.
- **System tray + SNI** (H47): D-Bus client em thread dedicada, `SniList`
  compartilhada via `Arc<Mutex>`. **DBusMenu nativo** (H47c) — flat menu v1
  com chamadas async em thread separada pra não bloquear o main loop.

### Notificações / Notifications (H54.P1 → P7)

Sistema completo `org.freedesktop.Notifications` D-Bus server.

| Phase | What | Como |
|-------|------|------|
| **P1** | Toast estilo macOS | D-Bus via `zbus 5` blocking, render como `MemoryRenderBufferRenderElement` no canto sup-direito, click dismiss |
| **P3** | Stack + timer + linhagem | Stack até `MAX_VISIBLE_TOASTS=4` (`Vec<NotificationEntry>`), fila VecDeque, hover pausa global (`paused_total: Duration` + `pause_started_at: Option<Instant>`), reflow ease_out_cubic 180ms, `replaces_id` atualiza in-place, critical + `expire_timeout=0` nunca expiram |
| **P3 fix** | Truncation + contraste + sombra | `wrap_to_lines_pixels` mede via `fontdue` (não corta no meio de caractere), scrim 2-layer (bg #2D2219 0.55 + scrim #1F1611 0.88) garante AA mesmo sobre wallpaper claro, sombra 2-layer (contact blur=8 + ambient blur=28) cacheada por card_h em `OnceLock<Mutex<HashMap>>` |
| **P4** | Action buttons + D-Bus signals | Hint parsing chunks_exact(2), separa `"default"` key, signals `ActionInvoked(id, key)` + `NotificationClosed(id, reason)` via `Arc<Mutex<Option<zbus::blocking::Connection>>>` compartilhado main↔server thread. Hint `resident` mantém toast aberto após action |
| **P4 polish** | Coluna + altura confortável | Buttons em `content_left_x` (alinhados com título/body), altura via token `action_btn_height=40` (PCD Max 48 reservado) — hit target visível = real, sem padding invisível |
| **P5** | Central + DND + histórico | Painel slide-in 220ms ease_out borda direita, histórico session-only `VecDeque` cap 150 com upsert por id, badge não-vistas (zera ao abrir), Do Not Disturb (critical fura), hint `transient` nunca entra no histórico, `replaces_id` que veio do histórico atualiza no lugar + re-popa toast como evento novo, `allocate_id` pula ids vivos (linhagem confiável) |
| **P5 fix** | Taskbar boundary + body wrap + shadow | `panel_h = output_h - taskbar_height()` (não cobre sino), scrim opaco full-height (combinado 0.982), body wrap até `panel_item_max_body_lines=2` com "…" só na última linha, sombra esquerda via `make_left_edge_shadow` (decay gaussiano horizontal σ=blur/2, 2 layers — mesma linguagem dos toasts) |
| **P6** | Som via libcanberra | `src/sound.rs` carrega `libcanberra.so.0` via `libloading` (dlopen runtime — build não exige libcanberra-dev). FFI direto: `ca_context_create/destroy`, `ca_proplist_*`, `ca_context_play_full`. Debounce 200ms via `Mutex<Option<Instant>>`. 2 OGGs Golden Hour em `assets/sounds/`. Silent fail se `.so` ausente ou arquivo falta. Toggle "Som"/"Som off" no header |
| **P7** | Visual bell + alert gate refactor | Overlay full-screen com decay gaussiano (fade-in 80ms + hold 80ms + fade-out 140ms = ~300ms total), peaks 0.18 normal / 0.28 critical (limites duros pra WCAG seizure safety), debounce 600ms anti-strobe (<1Hz no pior caso), cores honey #E89B3D / coral #D97757. Default OFF, override via `[notifications] flash_enabled_default = true` no luma.toml. **Refactor:** `sound_pending` → `alert_pending` — som + piscada compartilham o gate. `suppress_sound` é filtro só-do-som (não mata piscada — acessibilidade pra surdos com app mutado) |

🚧 **Gate aberto:** bateria comportamental P3-P5 (script `luma_test_notificacoes.sh`)
precisa de TTY com luma como servidor único de notif (plasma segura o bus em
sessão gráfica nested). Não bloqueia design da próxima fase.

### Desktop

- **Menu de contexto** (H48): right-click em área vazia, submenus com
  hover-gap +12px (evita fechar diagonal), 4 items dim aguardando features.
- **Wallpapers** (H49 + H51): 6 wallpapers Golden Hour built-in, galeria
  no ctx menu, hot-reload via thread + reload Arc<Wallpaper>.
- **Ícones no desktop** (H50): scan `~/Desktop` polling 2s, `FileKind`
  detect, grid column-major persistido em `~/.cache/luma/desktop-positions.toml`,
  `DesktopIconsLayer` mapeado na Space z=5 (entre wallpaper e janelas).
- **Interações** (H50.2): hit-test, single/multi select, drag com snap pra
  grid, rubber-band, kb arrow nav + Enter, double-click abre, Esc desseleciona.
- **Menu per-ícone** (H52): Abrir, Copiar, Renomear, Lixeira.
- **Rename inline** (H53): campo Golden Hour no label do ícone (sem KDialog),
  cursor blink, Enter commita, Esc cancela, click-outside commita.

### Misc / Técnico

- `calloop` main loop event-driven com sources Wayland + signals.
- `chrono::Local` pra `relative_time(Instant)` no histórico.
- `fontdue` rasterize_colored com premultiplied alpha BGRA Argb8888.
- `resvg`/`usvg` SVG decode em memória sem syscall.
- `image` crate (PNG + JPEG only, sem features desnecessárias).
- `tracing` + `tracing-subscriber` pra logs estruturados.
- `walkdir` pra index lazy de `~`.
- `xcursor` pra tema de cursor XCursor.

---

## 🔜 Next / Próximas etapas

### Curto prazo / Short term

- **H55 — Calendário ao clicar no relógio** (próximo, ainda não iniciado).
- **Acender items dim do ctx menu**: Criar novo, Colar, Organizar ícones
  (agora que tem ícones no desktop), Modo de edição (depende de widgets).
- **Applets restantes**: wifi, bluetooth, bateria (popups completos como
  o de volume).

### Médio prazo / Mid term

- **Notificações Phase 8+**: agrupamento por app no histórico (cognitive
  load reduction), markup Pango parser no body, persistência em disco
  (esperando decisão de privacidade), ducking de mídia, sound theme XDG
  completo via `CA_PROP_EVENT_ID` (libcanberra resolve sozinho), DND
  agendado, ação clicável no histórico, toggle de piscada na tela de settings.
- **Sistema de widgets configuráveis** (estilo KDE Plasma):
  - Widgets = Luma-native em Rust, renderizados pela própria luma-bar.
    NÃO são GTK nem Qt. Igual plasmoids do KDE — só renderizando a gente
    mesmo é que se garante o Golden Hour + hit target ≥ 40px.
  - GTK/Qt no Luma entra por outro caminho: apps de terceiro como janela
    (Wayland), bandeja via SNI, geração de tema GTK/Qt.
  - FUNDAÇÃO arquitetural grande — uma das maiores etapas. Só quando a
    base do DE estiver madura.
  - Desbloqueia: menu da taskbar + modo de edição geral.
- **Modo de edição geral** (estilo KDE, unificado):
  - Entra por item do ctx menu + atalho kb; sai com botão "Pronto" ou Esc.
  - Tela ganha overlay sutil, painéis/widgets com borda de realce, barra
    no topo com "Adicionar widget" e "Pronto".
  - Reposiciona ícones desktop, adiciona/move/remove/configura widgets
    (desktop e painéis), mexe nos painéis (altura, alinhamento, fixar/
    flutuar, criar novo), troca wallpaper e preset.
  - **Alça do widget — diferencial PCD**: cada widget focado mostra alça
    com botões (configurar, mover, redimensionar, remover). No KDE essas
    alças são minúsculas e irritantes — no Luma, hit target ≥ 40px, botões
    grandes, cor Golden Hour. Cortar o "rotacionar" (ninguém usa). Modo
    de edição usável por quem tem tremor / baixa visão = história PCD Max
    na prática.
  - Fatias incrementais: (1) só ícones do desktop, (2) + widgets, (3) +
    edição de painel.
- **Menu de contexto da taskbar** (depende de widgets).
- **Tela de configurações nativa do Luma**.
- **Configuração de tela** (resolução, monitores).

### Apps nativos do Luma / Native apps

> Programas próprios com identidade Golden Hour. Cada um é praticamente
> um projeto dentro do projeto — fazer um de cada vez, e só depois da
> base do DE estar madura.

- **Gerenciador de arquivos** (o mais importante — abre as pastas do
  desktop, navega, copia/move). Provavelmente `gio`/`udisks2` D-Bus pra
  detectar drives, `notify-rs` pra fsnotify, render próprio em GLES2.
- **Ferramenta de captura de tela** (recorte de tela, janela ou área,
  com anotação. PRIORIDADE ALTA: todo DE sério tem isso). Usa
  `wlr-screencopy-unstable-v1` ou compositor-native via Smithay.
- **Editor de texto**.
- **Visualizador de imagem** (já tem `image` crate como dep).
- **Calculadora** (já tem `meval` como dep).
- **Terminal próprio** (opcional — hoje usa o do sistema).
- **Gerenciador de tarefas** (monitor completo: processos, matar app,
  abas CPU / memória / disco / rede via `/proc`. Compartilha leitura com
  widget de monitor — widget é olhada rápida, app é completo. Baixa
  prioridade, fim da fila).
- **Centro de configurações** (já listado em médio prazo).

### Infra / técnico

- **Backend DRM** (rodar fora de outro compositor) → desbloqueia: cantos
  arredondados reais de janela, multi-window nativo, sessão TTY pra rodar
  bateria de testes de notif.
- **HiDPI / escalonamento** — `output.set_scale()` + render passes
  escalados.
- **Geração de tema GTK/Qt** — apps de terceiros adotam Golden Hour via
  `gsettings` + `kdeglobals`.

### Rumo ao v1.0

- Polish geral, QA completo.
- **4 presets visuais**: Classic (default), Mint (verde sage), Dock (apps
  estilo macOS), **PCD Max** (alvos 48px+, AAA contrast, flash ON, AT
  hooks). Cada preset = override de tokens no luma.toml.
- Documentação final (arquitetura, tokens, API D-Bus suportada).
- Surprise drop: Diolinux, r/linux, Phoronix, Hacker News.

---

## 📌 Technical debt / Dívidas técnicas (`docs/DEBT.md`)

- **Cantos arredondados de janela** → espera backend DRM.
- **calloop one-shot timer pra notif** → main loop chama `request_redraw`
  contínuo no nested winit, então timer wake desnecessário. Vira issue
  real no DRM backend.
- **Bateria comportamental P3-P5** → precisa de TTY com luma como servidor
  único de notif. Sem isso, gdbus calls batem no plasmashell.
- **Items dim do ctx menu**:
  - Criar novo, Colar, Organizar ícones → ícones no desktop (quase lá).
  - Modo de edição → sistema de widgets + modo de edição geral.
- **Menu da taskbar** → espera sistema de widgets.
- **Sound theme XDG** (CA_PROP_EVENT_ID) → trocar 1 linha em
  `play_file` quando alguém precisar de respeito a `sound-name` standard.

---

## 💭 Future ideas / Ideias futuras

> Recursos inspirados em mac/Windows que cabem num DE. NÃO são prioridade
> — entram só depois da base e dos apps nativos.

- **Busca global** (estilo Spotlight) — expande a busca do menu pra
  arquivos e configurações.
- **Snap de janelas** — arrastar pra borda encaixa em meia tela.
- **Modo escuro/claro automático por horário** — tema acompanha pôr do
  sol. Combina com Golden Hour, diferencial temático.
- **Atalhos de teclado customizáveis** pelo usuário.
- **Preview de arquivos** — prévia ao selecionar.
- **Trocador de janelas com miniatura** (Alt+Tab com preview, não só ícone).
- **Visão geral de janelas** (Exposé / Mission Control estilo Mac).
- **Histórico de área de transferência** (Win+V).
- **Cantos ativos** — mouse no canto dispara ação.
- **Night light** — tela esquenta à noite. Combina com Golden Hour.

### Acessibilidade / Accessibility (alimentam PCD Max preset)

> NÃO são "ideia futura" de verdade — são o que dá peso ao ângulo "DE
> desenhado por pessoa com deficiência" no surprise drop. Sobem na fila
> e entram antes do v1.0.

- **Lupa de tela** — ampliar pedaço.
- **Cursor grande e com realce** — achar o ponteiro fácil.
- **Teclas de aderência** — atalho uma tecla por vez.

---

## 🚫 Out of scope / Fora do escopo

> Features de empresa gigante — fogem de um DE de portfólio.

- Histórico de versões de arquivo (tipo Time Machine).
- Continuidade entre dispositivos.
- Loja de apps própria.
- Assistente de voz.

---

## 🎯 Principles / Princípios do projeto

- **One step at a time** — uma etapa terminada e testada antes da próxima.
- **Feature depends on missing system** = item dim + nota no `DEBT.md`.
- **Zero hex hardcoded** — tudo no `theme.toml`, defaults em `theme.rs`.
- **Commits atômicos** — cada um compila e roda.
- **Backup tag antes de cada etapa**.
- **Acessibilidade PCD** — hit targets ≥ 40px, WCAG AA mínimo, AAA quando
  cabe sem custo (P5 scrim, P7 contraste através do overlay).
- **Privacy-first** — session-only por default (histórico de notif), opt-in
  pra persistência.
- **Falha em silêncio** quando o lib opcional ausente (libcanberra `.so`,
  arquivos de som ausentes, etc). Build nunca quebra por dep externa
  opcional.
