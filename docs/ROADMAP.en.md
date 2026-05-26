# Vision and roadmap — Luma

---

## The vision

Luma wants to be a desktop environment that doesn't force anyone to
choose between beautiful, configurable and accessible.

Today, Linux users usually make a trade. Some desktops are gorgeous
but rigid. Others are configurable but cold. And accessibility is
almost always an afterthought, a settings menu someone remembered to
add at the end.

Luma rejects that trade. The proposal is a desktop with a traditional,
familiar layout, ergonomics in the spirit of Cinnamon and Windows,
configurability in the spirit of KDE, and accessibility treated as a
foundation. All wrapped in its own visual identity, Golden Hour.

The technical reference is System76's COSMIC, for its modern Rust
architecture. But Luma does not depend on it or copy it: it follows
its own path.

---

## What sets it apart

Luma is designed by a person with a physical disability. That changes
everything. Accessibility stops being a checklist item and becomes the
lens through which every decision is made.

It is not about adding a special mode. It is about the whole desktop
being comfortable: large click targets, keyboard navigation
everywhere, contrast that works, and the option of an extreme
accessibility preset for those who need it.

This is the angle that makes Luma different from any other existing
desktop environment.

---

## Roadmap

Development is split into steps. The realistic timeline is roughly two
years to version 1.0.

### Visual foundation — done

- Working Wayland compositor
- Fullscreen application menu
- Workspaces
- KDE-style taskbar with six zones
- System tray reading real data
- Icon resolver compatible with GTK and KDE applications
- Window decorations
- File icons on the desktop, with selection, drag, rubber-band and
  inline rename
- Desktop context menu and wallpaper gallery
- Volume applet and media OSD (now playing)

### Notification system — done (H54)

Complete D-Bus `org.freedesktop.Notifications` server, split into seven
phases:

- macOS-style toast in the upper right corner
- Stacking with hover-pause, overflow queue, animated reflow
- Action buttons (`ActionInvoked`) and `resident` hint (toast stays
  open after click)
- Notification center as a right-edge panel, with session-only history,
  unread badge and Do Not Disturb (critical urgency breaks through)
- Notification sound via libcanberra, with debounce and a sound toggle
  in the panel
- Visual bell (screen flash) with photosensitivity limits, default off
  (only the PCD Max preset turns it on)

### Living system — next steps

- Calendar on clicking the clock
- Remaining applets: network, bluetooth, battery (full popups)
- Light up the four dim items in the context menu (Create new, Paste,
  Arrange icons, Edit mode)

### Applications and polish

- Configurable widget system (KDE plasmoid-style, but Luma-native in
  Rust — a large architectural foundation)
- Unified edit mode (reposition desktop icons and widgets in one place)
- Native Luma applications: file manager, screenshot tool, text editor,
  image viewer, calculator
- Native settings panel
- HiDPI and scaling support
- DRM backend, to run directly without depending on another environment
- Multi-window support
- GTK/Qt theme generation so third-party apps wear Golden Hour

### Version 1.0

- General polish and bug fixing
- Release

### After 1.0 — the presets

Luma will have four visual presets, switchable by the user:

- **Classic** — the fullscreen menu, the default look
- **Mint** — a Cinnamon-style menu, more traditional
- **Dock** — a macOS-style bottom bar
- **PCD Max** — accessibility taken to the extreme

Each preset is a different way to live in the same desktop.

---

## Compatibility

Luma runs applications from both major Linux worlds: GTK and KDE/Qt.
Because it speaks Wayland, any modern application works. The icon
resolver finds icons from any installed theme.

Further ahead, Luma will also generate themes for GTK and Qt, so that
third-party applications wear the Golden Hour identity and the desktop
stays visually coherent from end to end.

---

## Philosophy

A few ideas guide the project:

- Finishing beats being perfect. Each step is closed before the next
  one begins.
- Consistency beats heroism. Luma grows one commit at a time.
- No work thrown away. Finished components are preserved, even when
  the focus shifts.
- The user decides. Real configurability means respecting choices.

Luma is a long-term project, built with patience. The destination is
clear, and the path is walked one step at a time.
