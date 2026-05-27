# Documentación técnica — Luma

Este documento describe la arquitectura de Luma, cómo compilar el
proyecto y cómo contribuir.

---

## Vista general de la arquitectura

Luma es modular por diseño. En lugar de un único binario gigante, el
entorno está dividido en partes independientes que conversan entre sí:

- **Compositor** — el núcleo Wayland, responsable de dibujar ventanas,
  procesar entrada y manejar la pantalla
- **luma-bar** — la taskbar
- **luma-menu** — el menú de aplicaciones
- **luma-applets** — los componentes de la bandeja del sistema

Esta separación permite evolucionar cada parte sin romper las otras, y
abre camino para que componentes puedan ser reemplazados en el futuro.

---

## Stack técnico

- **Lenguaje:** Rust (edición 2024, MSRV 1.85)
- **Protocolo gráfico:** Wayland
- **Framework de compositor:** Smithay
- **Renderizado de SVG:** usvg, resvg y tiny-skia
- **Fuentes:** fontdue (rasterización BGRA premultiplied)
- **Búsqueda:** fuzzy-matcher
- **Imágenes:** image (PNG y JPEG)
- **Configuración:** toml + serde
- **D-Bus:** zbus 5 (blocking, con thread dedicada por servicio)
- **Audio de notificación:** libcanberra cargada en runtime vía
  libloading (dlopen, no exige headers de desarrollo en el build)
- **Cliente PipeWire/PulseAudio:** spawn de `wpctl` (control) y
  `paplay` (playback simple) — sin dependencia del crate `pipewire`
- **Otras:** chrono (fechas/horario), meval (calculadora),
  tracing + tracing-subscriber (logs estructurados), walkdir

Smithay se usa como un fork local (en `vendor/smithay`), con un patch
propio para soporte de múltiples ventanas en el backend winit.

### Arquitectura del sistema de notificaciones

El sistema de notificaciones `org.freedesktop.Notifications` es un
servidor D-Bus corriendo en una thread dedicada (zbus blocking). El
estado del stack de toasts vive en `Arc<Mutex<NotificationsState>>`,
compartido entre la thread D-Bus (que recibe los `Notify`) y el main
loop (que renderiza y procesa input).

Las señales `ActionInvoked` y `NotificationClosed` se emiten desde el
main loop vía un `Arc<Mutex<Option<zbus::blocking::Connection>>>`
populado por la thread server después del `RequestName`.

La capa de sonido es separada (`src/sound.rs`) — carga
`libcanberra.so.0` en runtime, mantiene un único contexto `ca_context`
durante toda la sesión, y tiene debounce interno para impedir que
ráfagas se vuelvan strobes audibles. Si el `.so` no está instalado,
el engine cae en silencio sin romper el build ni crashear en runtime.

El flash de pantalla (visual bell) comparte con el sonido el mismo
gate de alerta (`alert_pending` por entrada) — sonido y flash son dos
consumidores independientes de la decisión "debe alertar". Los límites
de opacidad y duración son duros por cuestión de seguridad
fotosensible (WCAG 2.3.1 — máximo 3 flashes por segundo).

---

## Principios de diseño de código

### Cero colores fijos en el código

Ningún valor hexadecimal de color o medida visual se escribe directo
en Rust. Todo viene de un archivo de tema, el `theme.toml`. Eso
significa que la apariencia de Luma puede cambiar entera sin
recompilar nada.

### Accesibilidad como regla

Los hit targets siguen un mínimo de 40 pixels (48 o más en el preset
PCD Max). Toda funcionalidad debe ser alcanzable por teclado. El
contraste sigue las directivas WCAG AA, con AAA cuando se puede sin
costo (el scrim opaco del panel de notificaciones, por ejemplo,
garantiza AAA incluso sobre wallpaper claro). Esto no es negociable en
ninguna parte de la interfaz.

### Commits pequeños y funcionales

Cada commit del proyecto compila y corre. El trabajo se divide en
etapas nombradas (por ejemplo, "etapa H33.taskbar.4"), y cada etapa
es un paso verificable.

---

## Cómo compilar

### Dependencias del sistema

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

Herramientas runtime del system tray (red, bluetooth, audio) y sonido
de notificación:

```
sudo apt install network-manager bluez pipewire wireplumber libcanberra0
```

`libcanberra0` es opcional — si está ausente, el sonido de notificación
cae en silencio sin romper nada.

#### Fedora

```
sudo dnf install \
  gcc pkgconf-pkg-config \
  wayland-devel wayland-protocols-devel \
  libxkbcommon-devel libinput-devel \
  mesa-libEGL-devel mesa-libgbm-devel \
  libdrm-devel libseat-devel \
  systemd-devel dbus-devel pixman-devel \
  xorg-x11-server-Xwayland \
  libcanberra
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
sudo pacman -S --needed networkmanager bluez bluez-utils pipewire wireplumber libcanberra
```

También hace falta tener Rust instalado, vía rustup.

### Compilar

```
git clone <repositorio>
cd luma
cargo build
```

El fork de Smithay ya viene incluido en el repositorio, así que el
build funciona sin pasos extras. Para correr:

```
cargo run
```

---

## Estructura de carpetas

```
src/                código fuente en Rust
  backends/         backends de renderizado y entrada
  menu/             el menú de aplicaciones
  handlers/         handlers de protocolo Wayland
  input/            teclado, puntero y atajos
  notifications.rs  estado del stack de notificaciones + server D-Bus
  sound.rs          carga de libcanberra + playback
  state.rs          LumaState (estado global del compositor)
  theme.rs          tokens visuales + lectura del luma.toml
assets/             iconos, sonidos, identidad visual, capturas
vendor/             el fork local de Smithay
docs/               esta documentación
```

---

## Sistema de temas

Toda la apariencia se controla por `~/.config/luma/theme.toml`. Ese
archivo define colores, tamaños, espaciados y comportamientos de cada
componente: taskbar, menú, decoración de ventanas y applets.

La paleta por defecto es Golden Hour:

- Café — los tonos oscuros de fondo
- Coral — destaques y acciones importantes
- Miel — elementos secundarios y brillos
- Salvia — confirmaciones y estados positivos

---

## Cómo contribuir

Por ahora el repositorio es privado y el desarrollo es conducido por
una sola persona. Después de la versión 1.0, el proyecto será abierto
y este documento se expandirá con una guía completa de contribución.

Si estás leyendo esto antes, probablemente formás parte de un círculo
cercano. Sugerencias y revisiones son bienvenidas.
