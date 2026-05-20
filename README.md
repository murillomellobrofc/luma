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

## Deps de sistema (Fedora 44)

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
