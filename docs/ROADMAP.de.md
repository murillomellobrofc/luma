# 🗺️ Luma — Roadmap

Desktop-Umgebung in Rust + Smithay (Wayland)
Identität: Golden Hour (coffee / coral / honey / sage)
Ziel: v1.0 als Portfolio · Zeitplan ~2 Jahre · Surprise Drop
SCOPE: Luma = die Desktop-Umgebung, läuft auf jeder Distro.
Lumina OS (eigene Distro, mit eigenem OS-Installer) ist ein
SEPARATES Projekt mit eigener Roadmap. Gehört NICHT in diese Datei.

---

## ✅ ERLEDIGT

### Basis / Compositor

- Smithay-Compositor läuft (Wayland)
- Multi-Monitor, Multi-Workspace
- Echte Wayland-Apps laufen
- GPU-Rendering
- Rust 1.95 + Edition 2024
- Technisches QA fertig — 0 kritische Bugs

### Interface

- Hybrides Vollbildmenü (Favoriten, Suche, Sitzung)
- KDE-artige Taskbar (6 Zonen)
- Fensterdekoration (Titelleiste, Golden-Hour-Buttons)
- Icon-Resolver (GTK + KDE, 7557 Icons)

### Applets / OSD

- Lautstärke-Applet
- OSD für Lautstärke + Helligkeit
- Medien-OSD (Now Playing)
- Erweiterbares System-Tray + SNI (D-Bus)
- H54 — Benachrichtigungssystem Phase 1 (macOS-artige Toasts)
- H57 — Applets für WLAN, Bluetooth und Akku (Popups)
  · WLAN wird ausgeblendet, wenn keine Karte vorhanden ist
    (fehlende Hardware behandelt)
  · Kabelverbindung im Popup, einheitlicher Hover bei allen Applets,
    Scan über direkten D-Bus

### Desktop

- Kontextmenü des Desktops
- Standard-Golden-Hour-Wallpaper + Galerie
- Datei-Icons auf dem Desktop (lesen, zeichnen, Raster)
- Auswahl, Drag, Rubber-Band, Pfeile
- Pro-Icon-Menü (öffnen, kopieren, umbenennen, Papierkorb)
- Inline-Umbenennen (ohne KDialog)
- H55 — Kalender beim Klick auf die Uhr
- H56 — 3 dim-Items des Kontextmenüs aktiviert
  (Neu erstellen, Einfügen, Symbole anordnen — das 4., Modo de edição,
  bleibt dim bis das Widget-System existiert)
- **H58 — Sperrbildschirm (Lock Screen)**
  · Wayland-Protokoll ext-session-lock · Golden-Hour-Bildschirm mit
    riesiger Uhr, Datum und Passwortfeld · schließt den "Sperren"-
    Button im Menü
  · Cards (Kalender, E-Mail usw.) strikt passiv: nur lesend, nicht
    klickbar, keine sensiblen Inhalte
  · ist die visuelle Basis, die das SDDM-Theme und der Splash
    wiederverwenden

### In Arbeit

(nichts im Moment — der nächste Schritt kommt aus "Mittelfristig")

---

## 🔜 NÄCHSTE SCHRITTE (Weg zur v1.0)

### Kurzfristig

(leer — Items zu "In Arbeit" befördert oder bereits fertig.
Nächste Charge kommt aus "Mittelfristig")

### Mittelfristig

- **Benachrichtigungen Phase 2:** Stapeln, nach App gruppieren,
  Zentrum/Verlauf
- **Konfigurierbares Widget-System** (KDE-Plasma-artig)
  → Widgets, die der Nutzer im Panel und auf dem Desktop hinzufügt/
    entfernt/konfiguriert (Uhr, Wetter, Notizen, Systemmonitor usw.)
  → ENTSCHEIDUNG: Widgets = Luma-native in Rust, von der luma-bar
    selbst gerendert. KEIN GTK, KEIN Qt. Wie KDE-Plasmoids — die sind
    keine einsteckbaren Qt-Apps. Nur durch eigenes Rendern garantiert
    man Golden Hour + Hit Target ≥ 40px.
    GTK/Qt kommt anders nach Luma: Drittanbieter-Apps als Fenster
    (Wayland), Tray über SNI, und GTK/Qt-Theme-Generierung.
  → ist ein großes architektonisches FUNDAMENT — einer der größten
    Schritte der Roadmap. Erst wenn die DE-Basis reif ist.
  → entsperrt: Taskbar-Kontextmenü + allgemeiner Bearbeitungsmodus
- **Allgemeiner Bearbeitungsmodus von Luma** (KDE-artig, vereint)
  → ein Modus, in dem der Nutzer alles an einem Ort organisiert:
    Desktop-Icons neu positioniert UND Widgets/Panel-Layout anpasst
  → hängt von Desktop-Icons (✅ vorhanden) + Widget-System (❌ noch
    nicht) ab — deshalb zusammen mit/nach den Widgets
  → aktiviert das dim "Modo de edição"-Item des Kontextmenüs
  → vereint 3 Roadmap-Items in einem: dim-Menüitem + Widget-System +
    dieser allgemeine Bearbeitungsmodus

  **Funktionsweise:**
  · Eintritt über das "Modo de edição"-Item im Kontextmenü +
    Tastenkürzel; Austritt mit "Fertig"-Button oder Esc (alles
    sperrt wieder)
  · Bildschirm bekommt subtiles Overlay, Panels/Widgets haben
    Hervorhebungsrand, oben Leiste mit "Widget hinzufügen" und
    "Fertig"
  · erlaubt: Desktop-Icons neu positionieren · Widgets hinzufügen/
    bewegen/entfernen/konfigurieren (Desktop und Panels) · Panels
    anpassen (Höhe, Ausrichtung, fixieren/schweben, neu erstellen) ·
    Wallpaper und Preset wechseln

  **Widget-Griff — PCD-Differenzierer:**
  · jedes fokussierte Widget zeigt einen Griff mit Buttons
    (konfigurieren, bewegen, Größe ändern, entfernen)
  · in KDE sind diese Griffe winzig und nervig — in Luma,
    Hit Target ≥ 40px, große Buttons, Golden-Hour-Farbe
  · "Drehen" aus KDE entfernt (niemand benutzt es)
  · Bearbeitungsmodus nutzbar für Menschen mit Tremor / Sehschwäche =
    die PCD Max-Geschichte in der Praxis

  **Scheiben (jede kompiliert und läuft alleine):**
  1. Modus, der nur Desktop-Icons neu positioniert (Basis vorhanden)
  2. Widgets hinzufügen/bewegen/entfernen/konfigurieren
  3. Panel-Bearbeitung (Höhe, Ausrichtung, Panel erstellen)

- **Taskbar-Kontextmenü** (hängt von Widgets ab)
- **Native Luma-Einstellungsoberfläche** (enthält die Theme-Seiten der
  System-Bildschirme: Splash, Login/SDDM, Boot — ein Panel, wie das
  Farben-und-Themen von KDE)
- **Bildschirmkonfiguration** (Auflösung, Monitore)
- **Helligkeits-Slider im Akku-Applet** (kein eigenes Applet — geht
  mit dem Akku zusammen, wie KDE es macht. Das Backend existiert
  bereits, das Helligkeits-OSD beweist es; nur die klickbare Hülle
  fehlt. Erscheint nur, wenn Backlight vorhanden ist, gleiche Logik
  wie Akku)

### Native Luma-Apps

Eigene Programme von Luma mit Golden-Hour-Identität.
Jedes ist praktisch ein Projekt im Projekt — eines nach dem
anderen machen, und erst nachdem die DE-Basis reif ist.

1. **Dateimanager** (das wichtigste — öffnet die Desktop-Ordner,
   navigiert, kopiert/verschiebt)
2. **Screenshot-Tool** (Ausschnitt von Bildschirm, Fenster oder
   freiem Bereich, mit Anmerkungen. Stil Cmd+Shift+5 von Mac /
   Snipping Tool von Windows. HOHE PRIORITÄT: jedes ernsthafte DE
   hat das, direkt nach dem Dateimanager machen)
3. **Texteditor**
4. **Bildbetrachter**
5. **Taschenrechner**
6. **Eigenes Terminal** (optional — heute wird das des Systems benutzt)
7. **Task-Manager** (vollständiger Systemmonitor: Prozessliste,
   hängende App killen, Tabs für CPU / Speicher / Festplatte /
   Netzwerk. Teilt /proc-Lesen mit dem "Systemmonitor"-Widget aus
   den zukünftigen Ideen — Widget ist der schnelle Blick auf dem
   Panel, die App ist die vollständige Sache. Niedrige Priorität,
   Ende der Native-Apps-Warteschlange, nach dem Dateimanager)
8. **Software-Zentrum** (COSMIC-Store-Modell, KEIN eigenständiger Store)
   → grafische Vitrine mit Golden-Hour-Look zum Finden, Installieren,
     Aktualisieren und Entfernen von Apps
   → verwendet darunter, was bereits existiert: libflatpak für
     Flathub + packagekit für Systempakete. Luma hostet NICHTS,
     moderiert NICHT, hat KEINE eigene Infrastruktur
   → direkte Referenz: cosmic-store ist Rust und Open Source, man
     kann studieren, wie System76 es gemacht hat
   → substanzielle App, niedrige Priorität, blockiert v1.0 nicht (am
     Anfang nutzt der Nutzer irgendein Software-Zentrum, wie das
     Terminal heute)
9. **Konfigurationszentrum** (bereits unter "mittelfristig" aufgeführt)

### Infra / technisch

- **DRM-Backend** (außerhalb eines anderen Compositors laufen)
  → entsperrt: echte abgerundete Ecken, natives Multi-Window
  → entsperrt AUCH: VRR und Multi-Monitor mit gemischtem Refresh
    (Gaming Tier 2) — und das ist Brot und Butter für den Gamer.
    Also, DRM verdient mehr Priorität als es scheint: es ist nicht
    nur "abgerundete Ecken", es entsperrt die Hälfte dessen, was die
    Gaming-Community fordert. In der mittelfristigen Warteschlange
    nach oben schieben.
- **HiDPI / Skalierung** — fractional scaling OHNE den Bildschirm zu
  verwischen. PRIORITÄT: dient dem Gamer (hochauflösender Monitor)
  UND der PCD-Seite (lesbare UI bei hoher Dichte) gleichzeitig.
  Gleiche Arbeit, zwei Publika. Darf nicht kaputt ausgeliefert werden.
- **GTK/Qt-Theme-Generierung** (Drittanbieter-Apps übernehmen Golden Hour)

### Gaming (einzelne Phase — "dem Spiel aus dem Weg gehen")

Spielsupport. Passt in eine einzige Phase, kein Monster. Im Grunde
lernt der Compositor, beiseite zu treten. Smithay liefert bereits
die Blöcke für das meiste (Direct Scanout, VRR im DRM-Backend,
Tearing) — es ist mehr Verkabelung als Erfindung.
Position: nach der Taskbar und solidem DE-Kern. Muss NICHT im
ersten v1.0 sein. Aber Tier 1 kann früh dazukommen, weil es wenig
Code ist und der Gewinn groß.

PRIORITÄT — was die Leute laut fordern, ist NICHT das, wofür sie
einen bestrafen, wenn es fehlt. HDR ist die lauteste Forderung,
aber die schwierigste, und niemand zerstört ein neues DE, weil v1
kein HDR hat. VRR, fractional scaling und Multi-Monitor sind
dagegen Brot und Butter: fehlen sie oder kommen kaputt, testet der
Gamer 10 Min und geht. Diese DÜRFEN nicht kaputt ausgeliefert
werden. HDR bleibt als Star von v2, Schlagzeile für den Surprise Drop.

**Tier 1 — essenziell (ohne das ist Luma schlecht für Spiele):**
- Direct Scanout im Vollbild — der Spielpuffer geht direkt zur
  Hardware, ohne dass Luma in der Mitte kopiert
- Idle Inhibit — Wayland-Protokoll, das verhindert, dass der
  Bildschirm abschaltet, während das Spiel läuft (z.B. RedM mit
  Controller spielen ohne die Tastatur zu berühren). Klassischer
  Bug, billig zu lösen
- Eigene Effekte ausschalten — Vollbildspiel erkannt, Animation,
  Blur und Schatten abschalten. Spart GPU

**Tier 2 — wichtig, kann später kommen:**
- VRR (Adaptive Sync) — Pro-Monitor-Toggle für FreeSync/G-Sync.
  Tötet das Stuttering. Hängt vom DRM-Backend ab
- Multi-Monitor mit gemischtem Refresh — ein 144Hz und ein 60Hz
  zusammen, ohne dass das System alles auf den langsameren festklemmt.
  Hängt vom DRM-Backend ab
- Tearing Control — wp_tearing_control-Protokoll, Spiel fordert
  asynchrone Präsentation für minimale Latenz (kompetitives Spielen)

**Tier 3 — Luxus, viel später:**
- GameMode-Status in der Leiste (Dekoration, optional)
- HDR — zu komplex und zu neu für v1. Wird der STAR von v2:
  die lauteste Forderung der Community und Beschwerde Nr. 1 von
  Linux-Neulingen. Absichtlich aufgehoben für Schlagzeilen

### Auf v1.0 zu

**System-Bildschirme (das "Chrome" um die Sitzung):**
Wie KDE behandelt Luma diese als themable Komponenten, alle
konfigurierbar in einem einzigen Panel der nativen
Einstellungsoberfläche. Es wird KEIN Display Manager gebaut —
SDDM wird thematisiert.

- **Sitzungsdatei (.desktop)** — lässt Luma in der Liste von SDDM /
  GDM / jedem Display Manager erscheinen. Trivial, aber OHNE das
  loggt sich niemand in Luma ein. PFLICHT für v1.0.
- **SDDM-Theme (Golden Hour)** — der Login-Bildschirm. Ein QML-Theme
  für SDDM, genau wie KDE es macht. ENTSCHEIDUNG: kein eigener
  Greeter von Grund auf geschrieben; SDDM wird thematisiert, das
  schon existiert. Polish.
- **Splash-Screen** — der schnelle Bildschirm nach dem Login, während
  das DE lädt (das KSplash von KDE). Luma-gerendert, leicht,
  Markenmoment. Polish, niedrige Priorität.
- **Boot-Theme (Plymouth) Golden Hour** — nur Grafik + kleine
  Konfiguration, nichts wird gebaut. Billiger Polish, hoher Ertrag.
- **Packaging und Installation von Luma** — wie das DE auf die
  Maschine der Leute kommt. Deckt ab: Build-Anweisungen für den Code
  + Distro-Pakete (AUR für Arch, COPR für Fedora, .deb für Ubuntu).
  PFLICHT für den Surprise Drop — ohne das öffnen die Leute von
  r/linux, Phoronix und HN das Repo und können nicht testen.
  Paart sich mit der Sitzungsdatei. (Luma ist DE: wird NICHT zu
  Flatpak, Flatpak ist für Apps, ein DE installiert ins System.)
- **Allgemeiner Polish, vollständiges QA**
- **Die 4 visuellen Presets:** Classic, Mint, Dock, PCD Max
- **Endgültige Dokumentation**
- **Surprise Drop:** Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 NOTIERTE TECHNISCHE SCHULDEN (docs/DEBT.md)

- Abgerundete Fensterecken (wartet auf DRM-Backend)
- Benachrichtigungen: Stapeln / Zentrum / Aktionen / Ton
- Kontextmenü: 1 dim-Item übrig (Rest in H56 gelöst):
  · Modo de edição → wartet auf Widget-System + allgemeinen
    Bearbeitungsmodus
- Taskbar-Kontextmenü (wartet auf Widget-System)
- "Sperren"-Button im Sitzungsmenü → wartet auf den Sperrbildschirm
  (ext-session-lock). Solange er nicht existiert, hat der Button
  nichts zu tun.

---

## 💭 ZUKÜNFTIGE IDEEN (nach der Basis, notiert um nicht zu vergessen)

Features inspiriert von mac/Windows, die in ein DE passen. KEINE
Priorität — kommen erst nach der Basis und den nativen Apps.

- **Globale Suche (Spotlight-artig)** — Apps, Dateien und
  Einstellungen an einem Ort suchen. Erweitert die bereits
  vorhandene Menüsuche.
- **Fenster-Snap** — Ziehen zum Rand rastet das Fenster auf halbem
  Bildschirm ein. Produktivitäts-Feature.
- **Automatischer Dunkel-/Hell-Modus nach Tageszeit** — das Theme
  folgt dem Sonnenuntergang. Passt zur Golden-Hour-Identität, wäre
  ein thematischer Differenzierer von Luma.
- **Benutzerdefinierbare Tastenkürzel** vom Nutzer.
- **Dateivorschau** — Vorschau beim Auswählen einer Datei auf dem
  Desktop.
- **Fensterwechsler mit Miniatur** — das Alt+Tab, das eine Vorschau
  jedes Fensters statt nur Icon zeigt.
- **Fensterübersicht** — Stil Exposé/Mission Control von Mac,
  breitet alle Fenster auf dem Bildschirm aus zur Auswahl.
- **Zwischenablage-Verlauf** — das Win+V, hält die letzten kopierten
  Items.
- **Aktive Ecken** — die Maus in eine Ecke werfen löst eine Aktion
  aus (Desktop zeigen, Menü öffnen usw.).
- **Night Light** — der Bildschirm wärmt sich nachts farblich auf.
  Passt zur Golden-Hour-Identität, wie der automatische Dunkelmodus.

### Barrierefreiheit (füttert das PCD Max-Preset)

Diese sind KEINE echten "zukünftigen Ideen" — sie geben dem
Winkel "von einer Person mit Behinderung gestaltetes DE" Gewicht
beim Surprise Drop. Sie verdienen es, in der Schlange aufzusteigen
und vor v1.0 zu kommen.

- **Bildschirmlupe** — einen Bildschirmausschnitt vergrößern.
- **Großer und hervorgehobener Cursor** — den Zeiger leicht finden.
- **Einrastetasten** — Tastenkürzel eine Taste auf einmal drücken.

---

## 🚫 AUSSERHALB DES UMFANGS (nicht verfolgen)

Features von Megakonzernen — passen nicht zu einem Portfolio-DE.

- Dateiversionshistorie (Time-Machine-Stil)
- Kontinuität zwischen Geräten
- Sprachassistent

---

## 🎯 PROJEKTPRINZIPIEN

- Ein Schritt nach dem anderen, fertig und getestet vor dem nächsten
- Feature, das von einem nicht bereiten System abhängt = dim-Item +
  Notiz im DEBT
- Null Hardcoded-Hex — alles in theme.toml
- Atomare Commits, jeder kompiliert und läuft
- Backup-Tag vor jeder Etappe
- PCD-Barrierefreiheit: Hit Targets ≥ 40px, WCAG AA
