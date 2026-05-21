<div align="center">
  <img src="assets/branding/luma-logo.svg" alt="Luma" width="640">
</div>

# Luma

A desktop environment for Linux, written in Rust.

Luma is a complete desktop environment built on Wayland, with its own
compositor, taskbar, application menu and theming system. It aims to
bring together three things that rarely go together: careful visual
design, real configurability, and accessibility considered from day one.

> Status: in active development, heading toward version 1.0.

---

## What it is

Luma is not a theme or an extension of another desktop. It is a full
graphical environment, built from scratch:

- A custom **Wayland compositor**, built with Smithay
- A KDE-style **taskbar** with six zones (launcher, workspaces, apps,
  system tray, clock and show desktop)
- A fullscreen **application menu** with search, favorites and shortcuts
- **Window decorations** with soft shadows and custom controls
- A complete **theming system**, driven by a single configuration file

All of it carries a visual identity called Golden Hour: shades of
coffee, coral, honey and sage, the palette of a late afternoon.

---

## Why it exists

Most desktop environments treat accessibility as a settings menu tucked
away at the end. Luma takes the opposite path.

Luma is designed by a person with a physical disability, and
accessibility is an architectural decision, not an optional feature.
Generous click targets, full keyboard navigation, proper contrast, and
a planned extreme-accessibility preset are part of the project from the
very start.

The goal is simple: a desktop that is beautiful for everyone and
genuinely usable by everyone.

---

## Design identity

Luma mixes its references on purpose:

- The menu echoes the clarity of the classic Windows 7 menu
- The configurability follows the philosophy of Linux Mint and KDE
- The Rust technical foundation is inspired by COSMIC's architecture
- Accessibility guides every interface decision
- The Golden Hour look gives it a soul: coffee, coral, honey and sage

Traditional layout, familiar ergonomics, modern technology.

---

## Current state

Already working:

- Wayland compositor running GTK and KDE/Qt applications
- Full taskbar with a system tray reading real data
  (battery, network, bluetooth, volume)
- Fullscreen application menu
- Workspaces
- Window decorations
- Theming system via a configuration file

Under construction: notifications, clickable applets, native
applications, and the alternative visual presets.

---

## Documentation

- [Technical documentation](docs/TECHNICAL.en.md) — architecture,
  build, contributing
- [Vision and roadmap](docs/ROADMAP.en.md) — where Luma is heading
- [Versão em português](README.pt.md)

---

## License

GPL-3.0-or-later.
