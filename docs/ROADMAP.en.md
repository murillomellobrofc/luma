# 🗺️ Luma — Roadmap

> Desktop Environment in Rust + Smithay (Wayland)
> Identity: Golden Hour (coffee / coral / honey / sage)
> Target: v1.0 as portfolio · ~2-year timeline · surprise drop
>
> SCOPE: Luma = the Desktop Environment, runs on any distro.
> Lumina OS (its own distro, with its own OS installer) is a
> SEPARATE project, with its own roadmap. It does NOT belong in this file.

---

## ✅ DONE

### Base / Compositor
- Working Smithay compositor (Wayland)
- Multi-monitor, multi-workspace
- Runs real Wayland apps
- GPU rendering
- Rust 1.95 + 2024 edition
- Technical QA done — 0 critical bugs

### Interface
- Hybrid fullscreen menu (favorites, search, session)
- KDE-style taskbar (6 zones)
- Window decoration (titlebar, Golden Hour buttons)
- Icon resolver (GTK + KDE, 7557 icons)

### Applets / OSD
- Volume applet
- Volume + brightness OSD
- Media OSD (now playing)
- Expandable system tray + SNI (D-Bus)
- H54 — Notification system phase 1 (macOS-style toast)
- H57 — Wi-Fi, Bluetooth and Battery applets (popups)
  · hides Wi-Fi when there is no wireless card (absent hardware handled)
  · wired network in the popup, unified hover across all applets,
    scan via direct D-Bus

### Desktop
- Desktop context menu
- Default Golden Hour wallpapers + gallery
- File icons on the desktop (read, draw, grid)
- Selection, drag, rubber-band, arrows
- Per-icon menu (open, copy, rename, trash)
- Inline rename (no KDialog)
- H55 — Calendar on clicking the clock
- H56 — Lit up 3 dim items in the context menu
  (Create new, Paste, Arrange icons — the 4th, Modo de edição,
  stays dim until the widget system exists)
- H58 — Lock screen
  · Wayland protocol ext-session-lock · Golden Hour screen with
    giant clock, date and password field · closes the "Lock"
    button in the menu
  · cards (Calendar, Email, etc.) strictly passive: read-only,
    not clickable, don't leak sensitive content
  · serves as the visual base that the SDDM theme and the splash
    will reuse
  · slice 6 (hardening): hotkey Ctrl+Alt+L · XF86 key whitelist
    (brightness, volume, media pass through even when locked) ·
    client surface and pointer suppression during the lock · OSD
    renders on top of lock_bg
- H58.1 — Notifications on the lock screen
  · discreet stack on the lock cover (icon + app + count only,
    full mode is opt-in) · per-app aggregation from
    visible+history · seed LUMA_FAKE_NOTIFICATIONS=N to test in
    nested mode without tearing down the KDE popup
- H59 — Widget system, FOUNDATION (code ready, DISABLED until
  Modo de edição exists)
  · trait Widget (render + tick_interval + tick) in the
    luma-widget crate · name→builder registry in the luma crate ·
    RenderCtx host-aware (Panel { height } / Desktop) ·
    layout.toml as config ([[widget]] with type, placement,
    config)
  · POCs: LabelWidget (static text) and CounterWidget (1/s)
  · inline tick loop in the redraw handler, same pattern as the
    elapsed-checks in the OSD and the toasts
  · 2 hosts: panel row + free position on the desktop
    (z=5, behind windows)
  · MIGRATION DONE: monorepo Cargo workspace with 3 crates
    (luma, luma-theme, luma-widget). The old note about "migrate
    after widgets" became history
  · DISABLED BY DECISION: system stays OFF until Modo de edição
    exists. Without Modo de edição the user has no way to
    add/move/remove a widget — only by hand-editing layout.toml,
    and that is a dev experience, not a user experience.
    Reactivate with 1 uncomment in state.rs when Modo de edição
    lands.

### In progress
- 🔨 Contextual customization (pure Mint style) — PHASE in design
  Renamed from "General edit mode". DECISION: going 100% Mint/Cinnamon
  style, NO mode. Configuration happens via right-click directly where
  the problem is, just like Cinnamon does. Turns on the dormant widget
  system (H59). H number assigned per slice.

  Why pure Mint (not a hybrid with KDE):
  · Mint is 1 of the 4 pillars of Luma's identity (Windows 7 menu
    + Linux Mint customization + COSMIC tech + PCD design)
  · PCD: no mode means no friction. Right-click solves it without
    the user needing to remember to enter/exit a mode
  · The KDE-style "widget handle" (halo in edit mode) goes away.
    The PCD intent migrates: right-click menus have large items,
    strong contrast and hit target ≥ 40px

  How it works (Cinnamon model):
  · right-click on the panel → menu with add widget,
    configure panel, etc.
  · right-click on the desktop → add widget, customize
    desktop, change wallpaper/preset
  · right-click on a widget → configure, move, remove that
    specific widget
  · there is NO "edit mode" to enter/exit

  Consequences (written down so we don't forget):
  · the dim "Modo de edição" item in the desktop context menu is
    DELETED (not lit up) — part of the tasks in this phase
  · the "Taskbar context menu" that was in medium term is
    absorbed by this phase (right-click on the panel IS the
    configuration entry point now)
  · the old "mode that only repositions desktop icons" slice
    goes away — in the Mint style, the user just drags the icon
    and it works

  Proposed slices (design chat can refine):
  1. Right-click menus (panel, desktop, widget) + turn on
     H59. User can already add/move/remove widgets
  2. Customization dialog (manage installed widgets,
     change wallpaper/preset). MAY become a tab in the
     native Settings screen instead of a standalone dialog
     — decide in the design chat
  3. Configure panel (height, alignment, create/remove panel)

  Unlocked when phase closes: widget system leaves dormancy +
  dim menu item is deleted + taskbar context menu comes to life.

---

## 🔜 NEXT STEPS (path to v1.0)

### Short term
- (empty — items promoted to "In progress" or already done.
  Next batch comes from "Medium term")

### Medium term
- Notifications phase 2: stacking, group by app, center/history
- Native Luma settings screen (includes the theme pages for the
  System Screens: splash, login/SDDM, boot — one panel, just like
  KDE's Colors and Themes)
- Display configuration (resolution, monitors)
- Brightness slider inside the battery applet (not its own
  applet — goes with the battery, like KDE does. Backend already
  exists, the brightness OSD proves it; just the clickable shell
  is missing. Appears only when there is backlight, same logic as
  the battery)

### Native Luma apps
> Luma's own programs, with Golden Hour identity.
> Each is basically a project inside the project — do them one at
> a time, and only after the DE core is mature.
- File manager (the most important — opens desktop folders,
  navigates, copies/moves)
- Screenshot tool (screen, window or free-area capture, with
  annotation. Like Cmd+Shift+5 on Mac / Snipping Tool on Windows.
  HIGH PRIORITY: every serious DE has this, do it right after
  the file manager)
- Text editor
- Image viewer
- Calculator
- Own terminal (optional — currently uses the system's)
- Task manager (full system monitor: process list, kill stuck
  apps, tabs for CPU / memory / disk / network. Shares /proc
  reading with the "system monitor" widget from future ideas —
  widget is the quick glance on the panel, app is the full
  thing. Low priority, end of the native-app queue, after the
  file manager)
- Software center (COSMIC Store model, NOT a standalone store)
  → graphical showcase with Golden Hour skin to find, install,
    update and remove apps
  → underneath it uses what already exists: libflatpak for Flathub +
    packagekit for system packages. Luma does NOT host anything,
    does NOT moderate, has NO own infra
  → direct reference: cosmic-store is Rust and open source,
    you can study how System76 did it
  → substantial app, low priority, doesn't block v1.0 (early on
    the user uses any software center, same as the terminal today)
- Settings center (already listed in "medium term")

### Infra / technical
- DRM backend (run outside another compositor)
  → unblocks: real rounded corners, native multi-window
  → ALSO unblocks: VRR and multi-monitor with mixed refresh
    (Gaming Tier 2) — and those are bread and butter for the gamer.
    In other words, DRM deserves more priority than it looks: it's
    not just "rounded corners", it unlocks half of what the gaming
    community demands. Bump it up the medium-term queue.
- HiDPI / scaling — fractional scaling WITHOUT blurring the screen.
  PRIORITY: serves the gamer (high-res monitor) AND the PCD side
  (legible UI on high density) at the same time. Same work, two
  audiences. Cannot ship broken.
- GTK/Qt theme generation (third-party apps adopt Golden Hour)
- Monorepo migration (Cargo workspace) — ✅ DONE in H59.
  Today Luma is a monorepo with 3 crates: luma, luma-theme,
  luma-widget. Note kept only to document the decision and the
  timing of the migration.

### Gaming (single phase — "get out of the game's way")
> Gaming support. Fits in a single phase, not a monster. At its
> core it's the compositor learning to step aside. Smithay already
> delivers the building blocks for most of it (direct scanout, VRR
> on the DRM backend, tearing) — it's more wiring than inventing.
> Position: after the taskbar and the DE core are solid. Does NOT
> need to be in the initial v1.0. But Tier 1 can land early,
> since it's little code and big payoff.
>
> PRIORITY — what people LOUDLY ask for is NOT what they punish
> you for missing. HDR is the loudest demand, but it's the hardest
> and nobody slams a new DE for missing HDR in v1. VRR, fractional
> scaling and multi-monitor, on the other hand, are bread and
> butter: if they're missing or broken, the gamer tests for 10 min
> and leaves. Those CANNOT ship broken. HDR stays as the v2 star,
> headline material for the surprise drop.

**Tier 1 — essential (without these Luma is bad for gaming):**
- Direct scanout in fullscreen — game buffer goes straight to
  hardware, Luma copies nothing in between
- Idle inhibit — Wayland protocol that prevents the screen from
  blanking while a game runs (e.g., playing RedM with a controller
  without touching the keyboard). Classic bug, cheap fix
- Disable own effects — fullscreen game detected, cut animation,
  blur and shadow. Stop wasting GPU for nothing

**Tier 2 — important, can come later:**
- VRR (adaptive sync) — per-monitor toggle for FreeSync/G-Sync.
  Kills stutter. Depends on the DRM backend
- Multi-monitor with mixed refresh — a 144 Hz and a 60 Hz together
  without the system clamping everything to the slower one. Depends
  on the DRM backend
- Tearing control — wp_tearing_control protocol, game asks for
  async presentation for minimum latency (competitive gaming)

**Tier 3 — luxury, much later:**
- GameMode status in the bar (decoration, optional)
- HDR — too complex and too new for v1. Becomes the v2 STAR:
  it's the loudest demand from the community and the #1 complaint
  from newcomers to Linux. Saved on purpose for headline value

### Toward v1.0

**System screens (the "chrome" around the session):**
> Like KDE, Luma treats these as themeable components, all
> configurable in a single panel of the native Settings screen.
> We do NOT build a display manager — we theme SDDM.
- Session file (.desktop) — makes Luma appear in the SDDM /
  GDM / any display manager list. Trivial, but WITHOUT this
  nobody can log into Luma. MANDATORY for v1.0.
- SDDM theme (Golden Hour) — the login screen. A QML theme for
  SDDM, exactly like KDE does. DECISION: no custom greeter
  written from scratch; theme SDDM, which already exists. Polish.
- Splash screen — the quick screen after login while the DE
  loads (KDE's KSplash). Luma-rendered, lightweight, brand moment.
  Polish, low priority.
- Boot theme (Plymouth) Golden Hour — just graphics + small
  config, builds nothing. Cheap polish, high return.

- Luma packaging and installation — this is how the DE reaches
  people's machines. Covers: build instructions from source +
  distro packages (AUR for Arch, COPR for Fedora, .deb for
  Ubuntu). MANDATORY for the surprise drop — without this, the
  folks on r/linux, Phoronix and HN open the repo and can't test.
  Pairs with the session file. (Luma is a DE: it does NOT become
  a Flatpak, Flatpak is for apps, a DE installs on the system.)
- General polish, full QA
- The 4 visual presets: Classic, Mint, Dock, PCD Max
- Final documentation
- Surprise drop: Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 TECHNICAL DEBT NOTED (docs/DEBT.md)
- Rounded window corners (waiting for the DRM backend)
- Context menu: 1 dim item remaining (rest resolved in H56):
  · Modo de edição → this item will be DELETED (not lit up) in
    the Contextual customization phase, pure Mint decision: no
    mode, configuration via right-click directly where the problem is
- Taskbar menu → absorbed by the Contextual customization phase
  (right-click on the panel IS the configuration entry point)

---

## 💭 FUTURE IDEAS (post-foundation, noted so we don't forget)
> Features inspired by Mac/Windows that fit a DE. NOT a priority —
> they come in only after the foundation and the native apps.
- Global search (Spotlight-style) — search apps, files and
  settings in one place. Expands the existing menu search.
- Window snapping — drag to the edge snaps the window to half
  screen. Productivity feature.
- Automatic dark/light mode by time of day — the theme follows
  the sunset. Matches the Golden Hour identity, would be a thematic
  differentiator for Luma.
- User-customizable keyboard shortcuts.
- File previews — preview on selecting a file on the desktop.
- Window switcher with thumbnails — Alt+Tab showing a preview
  of each window instead of just an icon.
- Window overview — Expose/Mission Control style from Mac,
  spreads all windows on screen to pick from.
- Clipboard history — Win+V style, keeps the last copied items.
- Active corners — throwing the mouse into a corner triggers an
  action (show desktop, open menu, etc.).
- Night light — screen warms up in color at night. Matches the
  Golden Hour identity, like the automatic dark mode.

### Accessibility (feeds the PCD Max preset)
> These are NOT really "future ideas" — they're what gives weight
> to the "DE designed by a person with a disability" angle in the
> surprise drop. They deserve to move up the queue and land
> before v1.0.
- Screen magnifier — zoom into a piece of the screen.
- Large, highlighted cursor — find the pointer easily.
- Sticky keys — press shortcuts one key at a time.

## 🚫 OUT OF SCOPE (not pursuing)
> Mega-corp features — they don't belong in a portfolio DE.
- File version history (Time Machine style)
- Continuity between devices
- Voice assistant

## 🎯 PROJECT PRINCIPLES
- One step at a time, finished and tested before the next
- Feature that depends on a system not yet ready = dim item + note in DEBT
- Zero hardcoded hex — everything in theme.toml
- Atomic commits, each one compiles and runs
- Backup tag before each step
- PCD accessibility: hit targets ≥ 40px, WCAG AA
