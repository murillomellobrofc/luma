# Technische Dokumentation — Luma

Dieses Dokument beschreibt die Architektur von Luma, wie man das
Projekt baut und wie man beiträgt.

---

## Architektur-Überblick

Luma ist modular per Design. Statt einer einzigen riesigen Binärdatei
ist die Umgebung in unabhängige Teile gegliedert, die miteinander
kommunizieren:

- **Compositor** — der Wayland-Kern, verantwortlich für das Zeichnen
  von Fenstern, Eingabeverarbeitung und Bildschirmverwaltung
- **luma-bar** — die Taskbar
- **luma-menu** — das Anwendungsmenü
- **luma-applets** — die System-Tray-Komponenten

Diese Trennung erlaubt es, jeden Teil weiterzuentwickeln, ohne die
anderen zu brechen, und öffnet den Weg, Komponenten in Zukunft zu
ersetzen.

---

## Technischer Stack

- **Sprache:** Rust (Edition 2024, MSRV 1.85)
- **Grafikprotokoll:** Wayland
- **Compositor-Framework:** Smithay
- **SVG-Rendering:** usvg, resvg und tiny-skia
- **Schriftarten:** fontdue (BGRA premultiplied Rasterisierung)
- **Suche:** fuzzy-matcher
- **Bilder:** image (PNG und JPEG)
- **Konfiguration:** toml + serde
- **D-Bus:** zbus 5 (blocking, mit dediziertem Thread pro Service)
- **Benachrichtigungs-Audio:** libcanberra zur Laufzeit über libloading
  geladen (dlopen, benötigt keine Entwicklungs-Header im Build)
- **PipeWire/PulseAudio-Client:** spawn von `wpctl` (Steuerung) und
  `paplay` (einfache Wiedergabe) — keine Abhängigkeit vom `pipewire`-
  Crate
- **Andere:** chrono (Datum/Uhrzeit), meval (Taschenrechner),
  tracing + tracing-subscriber (strukturierte Logs), walkdir

Smithay wird als lokaler Fork (in `vendor/smithay`) verwendet, mit
eigenem Patch für Multi-Window-Support im winit-Backend.

### Architektur des Benachrichtigungssystems

Das `org.freedesktop.Notifications`-Benachrichtigungssystem ist ein
D-Bus-Server, der in einem dedizierten Thread läuft (zbus blocking).
Der Zustand des Toast-Stacks lebt in `Arc<Mutex<NotificationsState>>`,
geteilt zwischen dem D-Bus-Thread (der `Notify` empfängt) und dem
Main Loop (der rendert und Eingaben verarbeitet).

`ActionInvoked`- und `NotificationClosed`-Signale werden vom Main
Loop über eine `Arc<Mutex<Option<zbus::blocking::Connection>>>`
emittiert, die vom Server-Thread nach `RequestName` befüllt wird.

Die Audio-Schicht ist separat (`src/sound.rs`) — lädt `libcanberra.so.0`
zur Laufzeit, hält einen einzigen `ca_context` für die gesamte Sitzung,
und hat internen Debounce, um zu verhindern, dass Bursts zu hörbaren
Strobes werden. Wenn das `.so` nicht installiert ist, fällt die Engine
still aus, ohne den Build zu brechen oder zur Laufzeit zu crashen.

Der Bildschirm-Flash (Visual Bell) teilt mit dem Audio dasselbe
Alert-Gate (`alert_pending` pro Eintrag) — Audio und Flash sind zwei
unabhängige Konsumenten der "soll alarmieren"-Entscheidung. Die
Opazitäts- und Dauergrenzen sind streng aus Gründen der photo-
sensitiven Sicherheit (WCAG 2.3.1 — maximal 3 Flashes pro Sekunde).

---

## Code-Design-Prinzipien

### Null fest kodierte Farben im Code

Kein Hexadezimalwert für Farbe oder visuelle Messung ist direkt in
Rust geschrieben. Alles kommt aus einer Theme-Datei, `theme.toml`.
Das bedeutet, das Aussehen von Luma kann komplett geändert werden,
ohne irgendetwas neu zu kompilieren.

### Barrierefreiheit als Regel

Klickziele folgen einem Minimum von 40 Pixeln (48 oder mehr im
PCD-Max-Preset). Jede Funktionalität muss per Tastatur erreichbar
sein. Der Kontrast folgt den WCAG-AA-Richtlinien, mit AAA wo es
kostenlos geht (das opake Scrim des Benachrichtigungspanels zum
Beispiel garantiert AAA auch über hellem Wallpaper). Das ist in
keinem Teil der Oberfläche verhandelbar.

### Kleine und funktionierende Commits

Jeder Commit des Projekts kompiliert und läuft. Die Arbeit ist in
benannte Etappen unterteilt (zum Beispiel "etapa H33.taskbar.4"),
und jede Etappe ist ein verifizierbarer Schritt.

---

## Wie man baut

### Systemabhängigkeiten

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

Laufzeit-Tools des System-Trays (Netzwerk, Bluetooth, Audio) und
Benachrichtigungston:

```
sudo apt install network-manager bluez pipewire wireplumber libcanberra0
```

`libcanberra0` ist optional — wenn nicht vorhanden, fällt der
Benachrichtigungston still aus, ohne etwas zu brechen.

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

Laufzeit:

```
sudo pacman -S --needed networkmanager bluez bluez-utils pipewire wireplumber libcanberra
```

Es ist auch nötig, Rust installiert zu haben, über rustup.

### Bauen

```
git clone <repository>
cd luma
cargo build
```

Der Smithay-Fork ist bereits im Repository enthalten, also funktioniert
der Build ohne extra Schritte. Zum Ausführen:

```
cargo run
```

---

## Ordnerstruktur

```
src/                Rust-Quellcode
  backends/         Rendering- und Eingabe-Backends
  menu/             das Anwendungsmenü
  handlers/         Wayland-Protokoll-Handler
  input/            Tastatur, Zeiger und Tastenkürzel
  notifications.rs  Benachrichtigungs-Stack-Zustand + D-Bus-Server
  sound.rs          libcanberra-Laden + Wiedergabe
  state.rs          LumaState (globaler Compositor-Zustand)
  theme.rs          visuelle Tokens + luma.toml-Reader
assets/             Icons, Sounds, visuelle Identität, Screenshots
vendor/             der lokale Smithay-Fork
docs/               diese Dokumentation
```

---

## Theme-System

Das gesamte Aussehen wird von `~/.config/luma/theme.toml` gesteuert.
Diese Datei definiert Farben, Größen, Abstände und Verhaltensweisen
jeder Komponente: Taskbar, Menü, Fensterdekoration und Applets.

Die Standard-Palette ist Golden Hour:

- Kaffee — die dunklen Hintergrundtöne
- Koralle — Hervorhebungen und wichtige Aktionen
- Honig — sekundäre Elemente und Glanzlichter
- Salbei — Bestätigungen und positive Zustände

---

## Wie man beiträgt

Vorerst ist das Repository privat und die Entwicklung wird von einer
einzigen Person geleitet. Nach Version 1.0 wird das Projekt geöffnet
und dieses Dokument um einen vollständigen Beitragsleitfaden
erweitert.

Wenn du das vorher liest, gehörst du wahrscheinlich zu einem engen
Kreis. Vorschläge und Reviews sind willkommen.
