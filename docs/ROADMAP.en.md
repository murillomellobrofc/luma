# 🗺️ Luma — Roadmap

> Wayland desktop environment in Rust + Smithay.
> Identity: Golden Hour (coffee / coral / honey / sage).
> Target: v1.0 as portfolio · ~2 year timeline · surprise drop.

---

## ✅ Done

### Compositor base

- Working Smithay compositor (Wayland)
- Multi-monitor, multi-workspace
- Runs real Wayland apps
- GPU rendering
- Rust 1.95 + 2024 edition
- Technical QA: 0 critical bugs

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
- **H57 — Wi-Fi, Bluetooth and Battery applets** (popups)
  - Hide Wi-Fi when no wireless card (absent hardware handled)
  - Wired connection in the popup, unified hover across all applets,
    scan via direct D-Bus

### Desktop

- Desktop context menu
- Default Golden Hour wallpapers + gallery
- File icons on the desktop (read, draw, grid)
- Selection, drag, rubber-band, arrows
- Per-icon menu (open, copy, rename, trash)
- Inline rename (no KDialog)
- **H55 — Calendar on clicking the clock**
- **H56 — Lit up the 3 dim items in the context menu**
  (Create new, Paste, Arrange icons — the 4th, Edit mode, stays dim
  until the widget system exists)

### In progress

(nothing right now — short-term queue empty, next batch comes from
"Mid term")

---

## 🔜 Next steps (path to v1.0)

### Short term

(empty — items promoted to "In progress" or already done. Next batch
comes from "Mid term")

### Mid term

- **Notifications phase 2**: stacking, group by app, center/history

- **Configurable widget system** (KDE Plasma-style)
  - Widgets the user adds/removes/configures on the panel and on
    the desktop (clock, weather, notes, system monitor, etc).
  - **Decision**: widgets = Luma-native in Rust, rendered by luma-bar
    itself. NOT GTK, NOT Qt. Like KDE plasmoids — those aren't pluggable
    Qt apps. Only by rendering ourselves can we guarantee Golden Hour
    + hit target ≥ 40px.
  - GTK/Qt in Luma enters through other paths: third-party apps as
    windows (Wayland), tray via SNI, and GTK/Qt theme generation.
  - It's a major architectural foundation — one of the biggest steps
    on the roadmap. Only do it once the DE core is mature.
  - Unblocks: taskbar context menu + unified edit mode.

- **Unified edit mode** (KDE-style, unified)
  - A mode where the user organizes everything in one place:
    repositions desktop icons AND tweaks widgets/panel layout.
  - Depends on desktop icons (✅ done) + widget system (❌ not yet) —
    that's why it comes with/after widgets.
  - It's what lights up the dim "Edit mode" item in the context menu.
  - Combines 3 roadmap items into one: dim menu item + widget system
    + this unified edit mode.

  **How it works:**
  - Enter via the "Edit mode" item in the context menu + keyboard
    shortcut; exit with a "Done" button or Esc (everything locks again).
  - Screen gets a subtle overlay, panels/widgets get highlight borders,
    a top bar with "Add widget" and "Done".
  - You can: reposition desktop icons · add/move/remove/configure
    widgets (desktop and panels) · tweak panels (height, alignment,
    pin/float, create new) · change wallpaper and preset.

  **Widget handle — PCD differentiator:**
  - Each focused widget shows a handle with buttons (configure, move,
    resize, remove).
  - In KDE these handles are tiny and annoying — in Luma, hit target
    ≥ 40px, big buttons, Golden Hour color.
  - Cut the "rotate" from KDE (nobody uses it).
  - Edit mode usable by people with tremor / low vision = the PCD Max
    story made real.

  **Slices** (each compiles and runs on its own):
  1. Mode that only repositions desktop icons (base already exists)
  2. + add/move/remove/configure widgets
  3. + panel edit (height, alignment, create panel)

- **Taskbar context menu** (depends on widgets)
- **Native Luma settings screen**
- **Display configuration** (resolution, monitors)
- **Brightness slider inside the battery applet** — not its own applet,
  goes with the battery, like KDE does. The backend already exists
  (the brightness OSD proves it); just the clickable shell is missing.
  Appears only when there's backlight, same rule as battery.

### Native Luma apps

> Luma's own programs, with Golden Hour identity. Each is basically a
> project inside the project — do them one at a time, and only after
> the DE core is mature.

- **File manager** (the most important — opens desktop folders,
  navigates, copies/moves)
- **Screenshot tool** (rect, window or free area capture, with
  annotation. Like Cmd+Shift+5 on Mac / Snipping Tool on Windows. HIGH
  PRIORITY: every serious DE has this, do it right after the file
  manager)
- **Text editor**
- **Image viewer**
- **Calculator**
- **Own terminal** (optional — currently uses the system's)
- **Task manager** (full system monitor: process list, kill stuck
  apps, tabs for CPU / memory / disk / network. Shares /proc reading
  with the "system monitor" widget from future ideas — widget is the
  quick glance on the panel, app is the full thing. Low priority,
  end of native app queue, after the file manager)
- **Software center** (COSMIC Store model, NOT a standalone store)
  - Graphical showcase with Golden Hour skin to find, install, update,
    remove apps.
  - Uses what already exists underneath: libflatpak for Flathub +
    packagekit for system packages. Luma does NOT host anything, does
    NOT moderate, has NO own infra.
  - Direct reference: cosmic-store is Rust and open source, you can
    study how System76 did it.
  - Substantial app, low priority, doesn't block v1.0 (early on the
    user uses any software center, same as the terminal today).
- **Settings center** (already listed in "mid term")

### Infra / technical

- **DRM backend** (run outside another compositor)
  - Unblocks: real rounded window corners, native multi-window.
  - ALSO unblocks: VRR and multi-monitor with mixed refresh
    (Gaming Tier 2) — and these are bread and butter for the gamer.
  - In other words, DRM deserves more priority than it looks: it's
    not just "rounded corners", it unlocks half of what the gaming
    community demands. Bump it up the mid-term queue.

- **HiDPI / scaling** — fractional scaling WITHOUT blurring the screen.
  PRIORITY: serves gamers (high-resolution monitors) AND the PCD side
  (legible UI on high density) at the same time. Same work, two
  audiences. Cannot ship broken.

- **GTK/Qt theme generation** (third-party apps adopt Golden Hour)

### Gaming (single phase — "get out of the game's way")

> Gaming support. Fits in a single phase, not a monster. At its core
> it's the compositor learning to step out of the way. Smithay already
> delivers the building blocks for most of it (direct scanout, VRR on
> the DRM backend, tearing) — it's more about wiring than inventing.
>
> **Position**: after the taskbar and the DE core are solid. Doesn't
> need to be in v1.0 initially. But Tier 1 can land early, since it's
> little code and big payoff.
>
> **Priority** — what people LOUDLY ask for is NOT what they punish you
> for missing. HDR is the loudest demand, but it's the hardest one and
> nobody slams a new DE for missing HDR in v1. VRR, fractional scaling
> and multi-monitor, on the other hand, are bread and butter: if they're
> missing or broken, the gamer tests for 10 min and leaves. Those CANNOT
> ship broken. HDR stays as the v2 headline, surprise-drop material.

**Tier 1 — essential** (without these Luma is bad for gaming):

- **Direct scanout in fullscreen** — game buffer goes straight to
  hardware, no Luma copy in the middle.
- **Idle inhibit** — Wayland protocol that prevents the screen from
  blanking while a game runs (e.g., playing RedM with a controller
  without touching the keyboard). Classic bug, cheap fix.
- **Disable our own effects** — detected fullscreen game, cut
  animation, blur and shadow. Stop wasting GPU.

**Tier 2 — important, can come later:**

- **VRR (adaptive sync)** — per-monitor toggle for FreeSync/G-Sync.
  Kills stutter. Depends on DRM backend.
- **Multi-monitor with mixed refresh** — a 144Hz and a 60Hz together
  without the system clamping everything to the slower. Depends on
  DRM backend.
- **Tearing control** — `wp_tearing_control` protocol, game asks for
  async presentation for minimum latency (competitive gaming).

**Tier 3 — luxury, much later:**

- **GameMode status in the bar** (decoration, optional).
- **HDR** — too complex and too new for v1. Becomes the v2 STAR:
  it's the loudest demand from the community and the #1 complaint
  from newcomers to Linux. Saved on purpose for headline value.

### Toward v1.0

- General polish, full QA
- The 4 visual presets: Classic, Mint, Dock, PCD Max
- Final documentation
- Surprise drop: Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 Technical debt (`docs/DEBT.md`)

- Rounded window corners (waiting for DRM backend)
- Notifications: stacking / center / actions / sound
- Context menu: 1 dim item remaining (rest resolved in H56):
  - Edit mode → waits for widget system + unified edit mode
- Taskbar context menu (waits for widget system)

---

## 💭 Future ideas (post-foundation, noted so we don't forget)

> Features inspired by mac/Windows that fit a DE. NOT a priority —
> they come in only after the foundation and the native apps.

- **Global search** (Spotlight-style) — search apps, files and
  settings in one place. Expands the existing menu search.
- **Window snapping** — drag to the edge snaps the window to half
  screen. Productivity feature.
- **Automatic dark/light mode by time** — the theme follows the
  sunset. Matches the Golden Hour identity, would be Luma's thematic
  differentiator.
- **User-customizable keyboard shortcuts**.
- **File previews** — preview on selecting a desktop file.
- **Window switcher with thumbnails** — Alt+Tab showing a preview of
  each window instead of just an icon.
- **Window overview** — Exposé/Mission Control style from Mac, spreads
  all windows on screen to pick.
- **Clipboard history** — Win+V style, keeps the last copied items.
- **Active corners** — throwing the mouse into a corner triggers an
  action (show desktop, open menu, etc).
- **Night light** — screen warms up at night. Matches Golden Hour,
  like the automatic dark mode.

### Accessibility (feeds the PCD Max preset)

> These are NOT really "future ideas" — they're what gives weight to
> the "DE designed by a person with a disability" angle in the
> surprise drop. They deserve to move up the queue and land before
> v1.0.

- **Screen magnifier** — zoom into a piece of the screen.
- **Large, highlighted cursor** — find the pointer easily.
- **Sticky keys** — press shortcuts one key at a time.

---

## 🚫 Out of scope (not pursuing)

> Mega-corp features — they don't belong in a portfolio DE.

- File version history (Time Machine style)
- Continuity between devices
- Voice assistant

---

## 🎯 Project principles

- **One step at a time**, finished and tested before the next
- Feature that depends on a missing system = dim item + note in DEBT
- **Zero hardcoded hex** — everything in `theme.toml`
- **Atomic commits**, each one compiles and runs
- **Backup tag** before each step
- **PCD accessibility**: hit targets ≥ 40px, WCAG AA
