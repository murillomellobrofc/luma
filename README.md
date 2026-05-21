# Luma

Wayland compositor e ambiente gráfico do Lumina OS, todo em Rust.

## Status

v0.0.1, etapa A (bootstrap). Abre socket Wayland, não desenha nada ainda.

## Stack

- [Smithay](https://github.com/Smithay/smithay) (master) compositor
- [Calloop](https://crates.io/crates/calloop) event loop
- [Slint](https://slint.dev/) widgets do panel/launcher (próximas fases)
- [Tracing](https://crates.io/crates/tracing) logging

## Visão de design

Layout tradicional tipo KDE/Cinnamon: taskbar inferior, menu/launcher à
esquerda, system tray à direita, janelas com decorações clássicas.
Configurabilidade tipo KDE, ergonomia tipo Cinnamon/Windows.

Cosmic Desktop (System76) é referência técnica de arquitetura Rust, não
dependência.

## Roadmap

1. Compositor mínimo + janelas (2 a 4 meses)
2. Panel + launcher Slint (mais 3 a 6 meses)
3. Notificações, lock, power (mais 3 a 6 meses)
4. File manager, settings, polimento (mais 6 a 12 meses)

## Deps de sistema

### Fedora

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

### Ubuntu / Debian

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

Runtime do system tray usa `nmcli` (rede), `bluetoothctl` (bluetooth)
e `wpctl` (volume PipeWire):

```
sudo apt install network-manager bluez pipewire wireplumber
```

### Arch Linux

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

Runtime do system tray:

```
sudo pacman -S --needed networkmanager bluez bluez-utils pipewire wireplumber
```

## Build

```
cargo build
```

## Rodar aninhado (dentro de outra sessão Wayland)

```
cargo run
```

## Licença

GPL-3.0-or-later
