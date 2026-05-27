# 🗺️ Luma — Roadmap

> Desktop-Umgebung in Rust + Smithay (Wayland)
> Identität: Golden Hour (coffee / coral / honey / sage)
> Ziel: v1.0 als Portfolio · Zeitrahmen ~2 Jahre · surprise drop
>
> GELTUNGSBEREICH: Luma = die Desktop-Umgebung, läuft auf jeder Distro.
> Lumina OS (eigene Distro mit eigenem Betriebssystem-Installer) ist ein
> SEPARATES Projekt mit eigenem Roadmap. Gehört NICHT in diese Datei.

---

## ✅ BEREITS ERLEDIGT

### Basis / Compositor
- Smithay-Compositor funktionsfähig (Wayland)
- Multi-Monitor, Multi-Workspace
- Führt echte Wayland-Apps aus
- GPU-Rendering
- Rust 1.95 + Edition 2024
- Technische QA abgeschlossen — 0 kritische Bugs

### Oberfläche
- Hybrides Vollbild-Menü (Favoriten, Suche, Sitzung)
- Taskbar im KDE-Stil (6 Zonen)
- Fensterdekoration (Titelleiste, Golden-Hour-Schaltflächen)
- Symbolauflösung (GTK + KDE, 7557 Symbole)

### Applets / OSD
- Lautstärke-Applet
- OSD für Lautstärke + Helligkeit
- Medien-OSD (aktueller Titel)
- Erweiterbarer System Tray + SNI (D-Bus)
- H54 — Benachrichtigungssystem Phase 1 (Toast im macOS-Stil)
- H57 — Applets für WLAN, Bluetooth und Akku (Popups)
  · blendet WLAN aus, wenn kein Adapter vorhanden (fehlende Hardware behandelt)
  · Kabelverbindung im Popup, einheitlicher Hover in allen Applets,
    Scan direkt über D-Bus

### Desktop
- Desktop-Kontextmenü
- Golden-Hour-Standardhintergründe + Galerie
- Dateisymbole auf dem Desktop (lesen, zeichnen, Raster)
- Auswahl, Drag, Gummiband, Pfeiltasten
- Menü pro Symbol (öffnen, kopieren, umbenennen, Papierkorb)
- Inline-Umbenennung (ohne KDialog)
- H55 — Kalender beim Klick auf die Uhr
- H56 — 3 abgeblendete Einträge im Kontextmenü aktivieren
  (Neu erstellen, Einfügen, Symbole anordnen — der 4., Modo de edição,
  bleibt abgeblendet, bis das Widget-System existiert)
- H58 — Sperrbildschirm (Lock Screen)
  · Wayland-Protokoll ext-session-lock · Golden-Hour-Bildschirm mit
    großer Uhr, Datum und Passwortfeld · schließt die Schaltfläche
    „Sperren" im Menü ab
  · Karten (Kalender, E-Mail usw.) strikt passiv: nur lesen,
    nicht klickbar, kein Durchsickern sensibler Inhalte
  · dient als visuelle Grundlage, die das SDDM-Theme und der Splash
    wiederverwenden werden
  · Scheibe 6 (Härtung): Hotkey Ctrl+Alt+L · Whitelist für XF86-Tasten
    (Helligkeit, Lautstärke, Medien funktionieren auch im gesperrten
    Zustand) · Unterdrückung von Client-Surfaces und Pointer während
    des Locks · OSD rendert über dem Lock-Hintergrund
- H58.1 — Benachrichtigungen auf dem Sperrbildschirm
  · dezenter Stapel auf der Lock-Oberfläche (nur Symbol + App + Anzahl,
    Vollmodus ist opt-in) · Gruppierung nach App aus visible+history
  · Seed LUMA_FAKE_NOTIFICATIONS=N zum Testen im verschachtelten Modus,
    ohne das KDE-Popup abzuschießen
- H59 — Widget-System, FUNDAMENT (Code fertig, DEAKTIVIERT bis
  ein Bearbeitungsmodus existiert)
  · Trait Widget (render + tick_interval + tick) im Crate
    luma-widget · Registry name→builder im Crate luma ·
    RenderCtx host-aware (Panel { height } / Desktop) ·
    layout.toml als Konfiguration ([[widget]] mit type, placement,
    config)
  · POCs: LabelWidget (fester Text) und CounterWidget (1/s)
  · Tick-Schleife inline im Redraw-Handler, gleiche Machart wie die
    elapsed-checks beim OSD und den Toasts
  · 2 Hosts: Panel-Zeile + freie Position auf dem Desktop
    (z=5, hinter Fenstern)
  · MIGRATION DURCHGEFÜHRT: Monorepo Cargo Workspace mit 3 Crates
    (luma, luma-theme, luma-widget). Die Notiz „nach den Widgets
    migrieren" wurde zur Geschichte
  · BEWUSST DEAKTIVIERT: System bleibt AUS, bis der
    Bearbeitungsmodus existiert. Ohne Bearbeitungsmodus kann der
    Nutzer keine Widgets hinzufügen/verschieben/entfernen — nur
    durch manuelles Bearbeiten der layout.toml, und das ist eine
    Entwickler-Erfahrung, keine Nutzer-Erfahrung. Reaktivierung
    mit 1 Uncomment in state.rs, sobald der Bearbeitungsmodus da ist.

### In Arbeit
- 🔨 Kontextuelle Personalisierung (reiner Mint-Stil) — PHASE im Design
  Umbenannt von „Allgemeiner Bearbeitungsmodus". ENTSCHEIDUNG: wird 100%
  im Mint/Cinnamon-Stil, OHNE Modus. Konfiguration geschieht per
  Rechtsklick direkt dort, wo das Problem ist, genau wie Cinnamon es
  macht. Aktiviert das ruhende Widget-System (H59). H-Nummer wird pro
  Scheibe vergeben.

  Warum reiner Mint-Stil (nicht hybrid mit KDE):
  · Mint ist einer der 4 Pfeiler der Luma-Identität (Windows-7-Menü
    + Linux Mint Anpassung + COSMIC-Technik + PCD-Design)
  · PCD: kein Modus bedeutet keine Reibung. Rechtsklick löst das
    Problem, ohne dass man sich merken muss, einen Modus zu
    betreten/verlassen
  · Der „Widget-Griff" im KDE-Stil (Halo im Bearbeitungsmodus) entfällt.
    Die PCD-Absicht wandert: Rechtsklick-Menüs haben große Einträge,
    starken Kontrast und Hit-Target ≥ 40px

  So funktioniert es (Cinnamon-Modell):
  · Rechtsklick auf das Panel → Menü mit Widget hinzufügen,
    Panel konfigurieren usw.
  · Rechtsklick auf den Desktop → Widget hinzufügen, Desktop
    personalisieren, Hintergrundbild/Preset wechseln
  · Rechtsklick auf ein Widget → konfigurieren, verschieben, dieses
    spezifische Widget entfernen
  · Es gibt KEINEN „Bearbeitungsmodus" zum Betreten/Verlassen

  Konsequenzen (aufgeschrieben, damit nichts vergessen wird):
  · der abgeblendete Eintrag „Modo de edição" im Desktop-Kontextmenü
    wird GELÖSCHT (nicht aktiviert) — Teil der Aufgaben dieser Phase
  · das „Taskbar-Kontextmenü", das im mittelfristigen Bereich stand,
    wird von dieser Phase absorbiert (Rechtsklick auf das Panel IST
    jetzt der Konfigurationseinstieg)
  · die alte Scheibe „Modus positioniert nur Desktop-Symbole" entfällt
    — im Mint-Stil zieht der Nutzer einfach das Symbol, es funktioniert
    bereits

  Vorgeschlagene Scheiben (Design-Chat kann verfeinern):
  1. Rechtsklick-Menüs (Panel, Desktop, Widget) + H59 aktivieren.
     Nutzer kann bereits Widgets hinzufügen/verschieben/entfernen
  2. Personalisierungsdialog (installierte Widgets verwalten,
     Hintergrundbild/Preset wechseln). KANN ein Tab im nativen
     Einstellungsbildschirm werden statt eines separaten Dialogs —
     im Design-Chat entscheiden
  3. Panel konfigurieren (Höhe, Ausrichtung, Panel erstellen/entfernen)

  Wird beim Abschluss der Phase freigeschaltet: Widget-System verlässt
  den Ruhezustand + abgeblendeter Menüeintrag wird gelöscht +
  Taskbar-Kontextmenü wird erstellt.

---

## 🔜 NÄCHSTE SCHRITTE (Weg zur v1.0)

### Kurzfristig
- (leer — Einträge wurden zu „In Arbeit" befördert oder sind bereits
  erledigt. Nächster Schwung kommt aus „Mittelfristig")

### Mittelfristig
- Benachrichtigungen Phase 2: stapeln, nach App gruppieren, Zentrale/Verlauf
- Nativer Luma-Einstellungsbildschirm (enthält die Theme-Seiten der
  Systembildschirme: Splash, Login/SDDM, Boot — ein einziges Panel,
  wie das Panel „Farben und Themes" in KDE)
- Bildschirmkonfiguration (Auflösung, Monitore)
- Helligkeitsregler innerhalb des Akku-Applets (kein eigenes Applet —
  sitzt zusammen mit dem Akku, wie KDE es macht. Backend existiert
  bereits, das Helligkeits-OSD beweist es; es fehlt nur die klickbare
  Hülle. Erscheint nur bei vorhandenem Backlight, gleiche Logik wie
  beim Akku)

### Native Luma-Apps
> Eigene Programme von Luma mit Golden-Hour-Identität.
> Jede davon ist praktisch ein Projekt im Projekt — immer nur eine
> gleichzeitig umsetzen, und erst nachdem die DE-Basis ausgereift ist.
- Dateimanager (der wichtigste — öffnet Desktop-Ordner, navigiert,
  kopiert/verschiebt)
- Screenshot-Werkzeug (Bildschirmausschnitt, Fenster oder freier
  Bereich, mit Anmerkung. Im Stil von Cmd+Shift+5 auf dem Mac /
  Snipping Tool unter Windows. HOHE PRIORITÄT: jede ernsthafte DE hat
  das, direkt nach dem Dateimanager umsetzen)
- Texteditor
- Bildbetrachter
- Taschenrechner
- Eigenes Terminal (optional — nutzt derzeit das Systemterminal)
- Aufgabenmanager (vollständiger Systemmonitor: Prozessliste, hängende
  App beenden, Tabs für CPU / Arbeitsspeicher / Festplatte / Netzwerk.
  Teilt sich das Auslesen von /proc mit dem Widget „Systemmonitor" aus
  den Zukunftsideen — Widget ist der schnelle Blick im Panel, die App
  ist die vollständige Ansicht. Niedrige Priorität, Ende der Warteschlange
  der nativen Apps, nach dem Dateimanager)
- Software-Center (Modell COSMIC Store, KEIN unabhängiger Store)
  → grafische Oberfläche im Golden-Hour-Stil zum Suchen, Installieren,
    Aktualisieren und Entfernen von Apps
  → nutzt im Hintergrund vorhandene Infrastruktur: libflatpak für
    Flathub + PackageKit für Systempakete. Luma hostet NICHTS,
    moderiert NICHT, hat KEINE eigene Infrastruktur
  → direkte Referenz: cosmic-store ist in Rust und quelloffen,
    man kann studieren, wie System76 es umgesetzt hat
  → substanzielle App, niedrige Priorität, blockiert nicht die v1.0
    (anfangs nutzt der Anwender ein beliebiges Software-Center, wie
    heute das Terminal)
- Einstellungszentrale (bereits unter „Mittelfristig" aufgeführt)

### Infrastruktur / Technik
- DRM-Backend (außerhalb eines anderen Compositors laufen)
  → schaltet frei: echte abgerundete Ecken, natives Multi-Window
  → schaltet AUCH frei: VRR und Multi-Monitor mit gemischter
    Bildwiederholrate (Tier 2 von Gaming) — und die sind
    Grundvoraussetzung für Gamer. Das heißt, DRM verdient mehr
    Priorität als es scheint: es ist nicht nur „abgerundete Ecken",
    es entsperrt die Hälfte dessen, was die Gaming-Community
    verlangt. In der mittelfristigen Warteschlange nach oben rücken.
- HiDPI / Skalierung — Fractional Scaling OHNE unscharfe Darstellung.
  PRIORITÄT: dient Gamern (hochauflösender Monitor) UND der
  PCD-Seite (lesbare UI bei hoher Dichte) gleichzeitig. Gleiche
  Arbeit, zwei Zielgruppen. Darf nicht fehlerhaft ausgeliefert werden.
- GTK/Qt-Theme-Generierung (Drittanbieter-Apps übernehmen Golden Hour)
- Migration zu Monorepo (Cargo Workspace) — ✅ ABGESCHLOSSEN in H59.
  Heute ist Luma ein Monorepo mit 3 Crates: luma, luma-theme,
  luma-widget. Notiz beibehalten nur zur Dokumentation der Entscheidung
  und des Migrationszeitpunkts.

### Gaming (einzelne Phase — „dem Spiel aus dem Weg gehen")
> Spieleunterstützung. Passt in eine einzige Phase, ist kein Monster.
> Im Kern muss der Compositor lernen, sich zurückzunehmen. Smithay
> liefert bereits die Bausteine für den Großteil (Direct Scanout, VRR
> im DRM-Backend, Tearing) — es geht mehr ums Verdrahten als ums
> Neuerfinden.
> Position: nach der Taskbar und einem soliden DE-Kern. MUSS NICHT
> in der ersten v1.0 enthalten sein. Aber Tier 1 lässt sich früh
> einbauen, weil es wenig Code ist und der Gewinn groß.
>
> PRIORITÄT — was die Leute laut fordern, ist NICHT das, wofür sie
> dich abstrafen. HDR ist die lauteste Forderung, aber auch die
> schwierigste, und niemand verreißt eine neue DE, weil sie kein HDR
> in v1 hat. VRR, Fractional Scaling und Multi-Monitor dagegen sind
> Grundvoraussetzung: fehlen sie oder sind kaputt, testet der Gamer
> 10 Minuten und geht. Die DÜRFEN NICHT fehlerhaft ausgeliefert werden.
> HDR wird der Star von v2, Schlagzeile für den surprise drop.

**Tier 1 — essentiell (ohne das ist Luma schlecht für Spiele):**
- Direct Scanout im Vollbild — der Spielpuffer geht direkt an die
  Hardware, ohne dass Luma dazwischen kopiert
- Idle Inhibit — Wayland-Protokoll, das verhindert, dass der Bildschirm
  sich ausschaltet, während das Spiel läuft (z.B. RedM mit Controller
  spielen, ohne die Tastatur zu berühren). Klassischer Bug, günstig
  zu beheben
- Eigene Effekte abschalten — erkennt Vollbild-Spiel, deaktiviert
  Animation, Blur und Schatten. Hört auf, GPU sinnlos zu belasten

**Tier 2 — wichtig, kann später kommen:**
- VRR (Adaptive Sync) — Toggle pro Monitor für FreeSync/G-Sync.
  Eliminiert Stutter. Hängt vom DRM-Backend ab
- Multi-Monitor mit gemischter Bildwiederholrate — ein 144-Hz- und ein
  60-Hz-Monitor zusammen, ohne dass das System alles auf den niedrigsten
  Wert drosselt. Hängt vom DRM-Backend ab
- Tearing Control — Protokoll wp_tearing_control, Spiel fordert
  asynchrone Darstellung für minimale Latenz (Wettkampfspiele)

**Tier 3 — Luxus, deutlich später:**
- GameMode-Status in der Leiste (Schmuck, optional)
- HDR — zu komplex und zu neu für v1. Wird der STAR von v2:
  die lauteste Forderung der Community und die Beschwerde Nr. 1
  von Linux-Neulingen. Absichtlich aufgespart für die Schlagzeile

### Weg zur v1.0

**Systembildschirme (das „Chrome" rund um die Sitzung):**
> Wie KDE behandelt Luma diese als thembare Komponenten,
> alle konfigurierbar in einem einzigen Panel des nativen
> Einstellungsbildschirms. Man baut KEINEN Display Manager —
> man themet SDDM.
- Sitzungsdatei (.desktop) — lässt Luma in der Liste von
  SDDM / GDM / jedem Display Manager erscheinen. Trivial, aber OHNE
  das kann sich niemand bei Luma anmelden. PFLICHT für v1.0.
- SDDM-Theme (Golden Hour) — der Login-Bildschirm. Ein QML-Theme für
  SDDM, exakt wie KDE es macht. ENTSCHEIDUNG: kein eigener Greeter
  von Grund auf; man themet SDDM, das bereits existiert. Polish.
- Splash Screen — der kurze Bildschirm nach dem Login, während die DE
  lädt (der KSplash von KDE). Luma-gerendert, leicht, Markenmoment.
  Polish, niedrige Priorität.
- Boot-Theme (Plymouth) Golden Hour — nur Grafik + kleine
  Konfiguration, nichts Eigenes bauen. Günstiges Polish, hoher Ertrag.

- Paketierung und Installation von Luma — so gelangt die DE auf die
  Rechner der Nutzer. Umfasst: Build-Anweisungen für den Quellcode +
  Distro-Pakete (AUR für Arch, COPR für Fedora, .deb für Ubuntu).
  PFLICHT für den surprise drop — ohne das öffnen die Leute von
  r/linux, Phoronix und HN das Repo und können nicht testen.
  Gehört zusammen mit der Sitzungsdatei. (Luma ist eine DE: wird NICHT
  zum Flatpak, Flatpak ist für Apps, DE wird im System installiert.)
- Allgemeines Polish, vollständige QA
- Die 4 visuellen Presets: Classic, Mint, Dock, PCD Max
- Abschließende Dokumentation
- Surprise Drop: Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 FESTGEHALTENE TECHNISCHE SCHULDEN (docs/DEBT.md)
- Abgerundete Fensterecken (wartet auf DRM-Backend)
- Kontextmenü: 1 verbliebener abgeblendeter Eintrag (Rest in H56 gelöst):
  · Modo de edição → dieser Eintrag wird in der Phase der kontextuellen
    Personalisierung GELÖSCHT (nicht aktiviert), Entscheidung reiner
    Mint-Stil: kein Modus, Konfiguration per Rechtsklick direkt dort,
    wo das Problem ist
- Taskbar-Menü → absorbiert von der Phase der kontextuellen
  Personalisierung (Rechtsklick auf das Panel IST der
  Konfigurationseinstieg)

---

## 💭 ZUKUNFTSIDEEN (nach der Basis, festgehalten zum Nicht-Vergessen)
> Funktionen inspiriert von macOS/Windows, die in eine DE passen. KEINE
> Priorität — kommen erst nach der Basis und den nativen Apps.
- Globale Suche (im Spotlight-Stil) — Apps, Dateien und Einstellungen
  an einem Ort suchen. Erweitert die bereits vorhandene Menüsuche.
- Fenster-Snapping — an den Rand ziehen rastet das Fenster auf halber
  Bildschirmbreite ein. Produktivitätsfunktion.
- Automatischer Dunkel-/Hellmodus nach Uhrzeit — das Theme folgt dem
  Sonnenuntergang. Passt zur Golden-Hour-Identität, wäre ein
  thematisches Alleinstellungsmerkmal von Luma.
- Vom Nutzer anpassbare Tastenkürzel.
- Dateivorschau — Vorschau beim Auswählen einer Datei auf dem Desktop.
- Fensterwechsler mit Miniaturansicht — Alt+Tab zeigt eine Vorschau
  jedes Fensters statt nur eines Symbols.
- Fensterübersicht — im Exposé-/Mission-Control-Stil von macOS,
  verteilt alle Fenster auf dem Bildschirm zur Auswahl.
- Zwischenablage-Verlauf — das Win+V, speichert die zuletzt kopierten
  Einträge.
- Aktive Ecken — die Maus in eine Ecke bewegen löst eine Aktion aus
  (Desktop anzeigen, Menü öffnen usw.).
- Nachtlicht — der Bildschirm wird nachts wärmer im Farbton. Passt
  zur Golden-Hour-Identität, wie der automatische Dunkelmodus.

### Barrierefreiheit (speisen das Preset PCD Max)
> Diese sind KEINE echten „Zukunftsideen" — sie verleihen dem
> Narrativ „DE, entworfen von einer Person mit Behinderung" beim
> surprise drop Gewicht. Verdienen es, in der Warteschlange aufzusteigen
> und vor v1.0 eingebaut zu werden.
- Bildschirmlupe — einen Bereich des Bildschirms vergrößern.
- Großer Cursor mit Hervorhebung — den Zeiger leicht finden.
- Einrastfunktion — Tastenkürzel eine Taste nach der anderen drücken.

## 🚫 AUSSERHALB DES GELTUNGSBEREICHS (nicht verfolgen)
> Features großer Unternehmen — gehen über eine Portfolio-DE hinaus.
- Dateiversionsverlauf (à la Time Machine)
- Geräteübergreifende Kontinuität
- Sprachassistent

## 🎯 PROJEKTPRINZIPIEN
- Ein Schritt nach dem anderen, abgeschlossen und getestet bevor der nächste beginnt
- Feature, das ein noch nicht fertiges System voraussetzt = abgeblendeter Eintrag + Notiz in DEBT
- Null hartcodierte Hex-Werte — alles in theme.toml
- Atomare Commits, jeder kompiliert und läuft
- Backup-Tag vor jeder Etappe
- Barrierefreiheit PCD: Hit-Targets ≥ 40px, WCAG AA
