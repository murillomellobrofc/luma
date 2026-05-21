# Technical documentation — Luma

This document describes Luma's architecture, how to build the project
and how to contribute.

---

## Architecture overview

Luma is modular by design. Instead of a single huge binary, the
environment is split into independent parts that talk to each other:

- **Compositor** — the Wayland core, responsible for drawing windows,
  processing input and managing the display
- **luma-bar** — the taskbar
- **luma-menu** — the application menu
- **luma-applets** — the system tray components

This separation makes it possible to evolve each part without breaking
the others, and opens the door for components to be replaced in the
future.

---

## Technical stack

- **Language:** Rust (2021 edition)
- **Display protocol:** Wayland
- **Compositor framework:** Smithay
- **SVG rendering:** usvg, resvg and tiny-skia
- **Fonts:** fontdue
- **Search:** fuzzy-matcher
- **Images:** image
- **Configuration:** toml
- **Others:** chrono, meval, tracing

Smithay is used as a local fork (in `vendor/smithay`), with a custom
patch for multi-window support in the winit backend.

---

## Code design principles

### Zero hardcoded colors

No hexadecimal color value or visual measurement is written directly
in the Rust code. Everything comes from a theme file, `theme.toml`.
This means Luma's entire appearance can change without recompiling
anything.

### Accessibility as a rule

Click targets follow a minimum of 44 pixels. Every feature must be
reachable by keyboard. Contrast follows the WCAG AA guidelines. This
is non-negotiable across the whole interface.

### Small, working commits

Every commit in the project compiles and runs. Work is split into
named steps (for example, "etapa H33.taskbar.4"), and each step is a
verifiable increment.

---

## How to build

### System dependencies

#### Debian / Ubuntu

```
sudo apt install \
  build-essential pkg-config \
  libwayland-dev wayland-protocols \
  libxkbcommon-dev libinput-dev \
  libegl-dev libgbm-dev \
  libdrm-dev libseat-dev \
  libsystemd-dev libdbus-1-dev libpixman-1-dev \
  xwayland \
  fonts-dejavu-core fonts-liberation
```

System tray runtime tools (network, bluetooth, audio):

```
sudo apt install network-manager bluez pipewire wireplumber
```

#### Fedora

```
sudo dnf install \
  gcc pkgconf-pkg-config \
  wayland-devel wayland-protocols-devel \
  libxkbcommon-devel libinput-devel \
  mesa-libEGL-devel mesa-libgbm-devel \
  libdrm-devel libseat-devel \
  systemd-devel dbus-devel pixman-devel \
  xorg-x11-server-Xwayland
```

#### Arch Linux

```
sudo pacman -S --needed \
  base-devel pkgconf \
  wayland wayland-protocols \
  libxkbcommon libinput \
  mesa libdrm seatd \
  systemd dbus pixman \
  xorg-xwayland \
  ttf-dejavu ttf-liberation
```

Runtime:

```
sudo pacman -S --needed networkmanager bluez bluez-utils pipewire wireplumber
```

You also need Rust installed, via rustup.

### Build

```
git clone <repository>
cd luma
cargo build
```

The Smithay fork is already included in the repository, so the build
works with no extra steps. To run:

```
cargo run
```

---

## Folder structure

```
src/            Rust source code
  backends/     rendering and input backends
  menu/         the application menu
  handlers/     Wayland protocol handlers
  input/        keyboard, pointer and shortcuts
assets/         icons, visual identity, screenshots
vendor/         the local Smithay fork
docs/           this documentation
```

---

## Theming system

The entire appearance is controlled by `~/.config/luma/theme.toml`.
This file defines colors, sizes, spacing and behaviors for each
component: taskbar, menu, window decorations and applets.

The default palette is Golden Hour:

- Coffee — the dark background tones
- Coral — highlights and important actions
- Honey — secondary elements and glows
- Sage — confirmations and positive states

---

## How to contribute

For now the repository is private and development is led by a single
person. After version 1.0, the project will be opened and this
document will be expanded with a full contribution guide.

If you are reading this before then, you are probably part of a close
circle. Suggestions and reviews are welcome.
