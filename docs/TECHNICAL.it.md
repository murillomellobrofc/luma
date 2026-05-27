# Documentazione tecnica — Luma

Questo documento descrive l'architettura di Luma, come compilare il
progetto e come contribuire.

---

## Vista generale dell'architettura

Luma è modulare per design. Invece di un singolo binario gigante,
l'ambiente è diviso in parti indipendenti che dialogano tra loro:

- **Compositor** — il nucleo Wayland, responsabile di disegnare
  finestre, processare input e gestire lo schermo
- **luma-bar** — la taskbar
- **luma-menu** — il menu delle applicazioni
- **luma-applets** — i componenti del system tray

Questa separazione permette di evolvere ogni parte senza rompere le
altre, e apre la strada alla sostituzione futura dei componenti.

---

## Stack tecnico

- **Linguaggio:** Rust (edition 2024, MSRV 1.85)
- **Protocollo grafico:** Wayland
- **Framework compositor:** Smithay
- **Rendering SVG:** usvg, resvg e tiny-skia
- **Font:** fontdue (rasterizzazione BGRA premultiplied)
- **Ricerca:** fuzzy-matcher
- **Immagini:** image (PNG e JPEG)
- **Configurazione:** toml + serde
- **D-Bus:** zbus 5 (blocking, con thread dedicato per servizio)
- **Audio di notifica:** libcanberra caricata in runtime tramite
  libloading (dlopen, non richiede header di sviluppo nel build)
- **Client PipeWire/PulseAudio:** spawn di `wpctl` (controllo) e
  `paplay` (playback semplice) — senza dipendenza dal crate `pipewire`
- **Altre:** chrono (date/orari), meval (calcolatrice),
  tracing + tracing-subscriber (log strutturati), walkdir

Smithay è usato come fork locale (in `vendor/smithay`), con una patch
propria per il supporto multi-finestra nel backend winit.

### Architettura del sistema di notifiche

Il sistema di notifiche `org.freedesktop.Notifications` è un server
D-Bus che gira in un thread dedicato (zbus blocking). Lo stato dello
stack di toast vive in `Arc<Mutex<NotificationsState>>`, condiviso tra
il thread D-Bus (che riceve le `Notify`) e il main loop (che
renderizza e processa l'input).

I segnali `ActionInvoked` e `NotificationClosed` sono emessi dal main
loop tramite un `Arc<Mutex<Option<zbus::blocking::Connection>>>`
popolato dal thread server dopo il `RequestName`.

Il livello audio è separato (`src/sound.rs`) — carica
`libcanberra.so.0` in runtime, mantiene un singolo contesto
`ca_context` per tutta la sessione, e ha un debounce interno per
impedire che le raffiche diventino strobe udibili. Se il `.so` non è
installato, l'engine cade in silenzio senza rompere il build né
crashare in runtime.

Il flash dello schermo (visual bell) condivide con l'audio lo stesso
gate di allerta (`alert_pending` per voce) — audio e flash sono due
consumer indipendenti della decisione "deve allertare". I limiti di
opacità e durata sono rigidi per ragioni di sicurezza fotosensibile
(WCAG 2.3.1 — massimo 3 flash al secondo).

---

## Principi di design del codice

### Zero colori fissi nel codice

Nessun valore esadecimale di colore o misura visiva è scritto
direttamente in Rust. Tutto viene da un file di tema, `theme.toml`.
Questo significa che l'aspetto di Luma può cambiare interamente senza
ricompilare nulla.

### Accessibilità come regola

I hit target seguono un minimo di 40 pixel (48 o più nel preset PCD
Max). Ogni funzionalità deve essere raggiungibile da tastiera. Il
contrasto segue le linee guida WCAG AA, con AAA quando si può senza
costo (lo scrim opaco del pannello notifiche, per esempio, garantisce
AAA anche su wallpaper chiaro). Questo non è negoziabile in nessuna
parte dell'interfaccia.

### Commit piccoli e funzionanti

Ogni commit del progetto compila e gira. Il lavoro è diviso in tappe
nominate (per esempio, "etapa H33.taskbar.4"), e ogni tappa è un
passo verificabile.

---

## Come compilare

### Dipendenze di sistema

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

Strumenti runtime del system tray (rete, bluetooth, audio) e suono
di notifica:

```
sudo apt install network-manager bluez pipewire wireplumber libcanberra0
```

`libcanberra0` è opzionale — se assente, il suono di notifica cade
in silenzio senza rompere nulla.

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

Serve anche avere Rust installato, tramite rustup.

### Compilare

```
git clone <repository>
cd luma
cargo build
```

Il fork di Smithay è già incluso nel repository, quindi il build
funziona senza passi extra. Per girare:

```
cargo run
```

---

## Struttura delle cartelle

```
src/                codice sorgente in Rust
  backends/         backend di rendering e input
  menu/             il menu delle applicazioni
  handlers/         handler del protocollo Wayland
  input/            tastiera, puntatore e scorciatoie
  notifications.rs  stato dello stack di notifiche + server D-Bus
  sound.rs          caricamento di libcanberra + playback
  state.rs          LumaState (stato globale del compositor)
  theme.rs          token visivi + lettura del luma.toml
assets/             icone, suoni, identità visiva, screenshot
vendor/             il fork locale di Smithay
docs/               questa documentazione
```

---

## Sistema di temi

Tutto l'aspetto è controllato da `~/.config/luma/theme.toml`. Questo
file definisce colori, dimensioni, spaziature e comportamenti di ogni
componente: taskbar, menu, decorazione finestre e applet.

La paletta di default è Golden Hour:

- Caffè — i toni scuri di sfondo
- Corallo — risalti e azioni importanti
- Miele — elementi secondari e bagliori
- Salvia — conferme e stati positivi

---

## Come contribuire

Per ora il repository è privato e lo sviluppo è condotto da una sola
persona. Dopo la versione 1.0, il progetto sarà aperto e questo
documento sarà espanso con una guida completa di contribuzione.

Se stai leggendo questo prima, probabilmente fai parte di un circolo
ristretto. Suggerimenti e revisioni sono benvenuti.
