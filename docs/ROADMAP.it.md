# 🗺️ Luma — Roadmap

> Desktop Environment in Rust + Smithay (Wayland)
> Identità: Golden Hour (coffee / coral / honey / sage)
> Obiettivo: v1.0 come portfolio · timeline ~2 anni · surprise drop
>
> AMBITO: Luma = il Desktop Environment, gira su qualsiasi distro.
> Lumina OS (distro propria, con installer di SO proprio) è un
> progetto SEPARATO, con roadmap proprio. NON rientra in questo file.

---

## ✅ GIÀ FATTO

### Base / Compositor
- Compositor Smithay funzionante (Wayland)
- Multi-monitor, multi-workspace
- Esegue app Wayland reali
- Rendering via GPU
- Rust 1.95 + edizione 2024
- QA tecnico completato — 0 bug critici

### Interfaccia
- Menu fullscreen ibrido (preferiti, ricerca, sessione)
- Taskbar stile KDE (6 zone)
- Decorazione della finestra (titlebar, pulsanti Golden Hour)
- Risolutore di icone (GTK + KDE, 7557 icone)

### Applet / OSD
- Applet del volume
- OSD di volume + luminosità
- OSD multimediale (now playing)
- System tray espandibile + SNI (D-Bus)
- H54 — Sistema di notifiche fase 1 (toast stile macOS)
- H57 — Applet di wifi, bluetooth e batteria (popup)
  · nasconde il wifi quando non c'è la scheda (hardware assente gestito)
  · rete cablata nel popup, hover unificato in tutti gli applet,
    scan via D-Bus diretto

### Desktop
- Menu di contesto del desktop
- Wallpaper predefiniti Golden Hour + galleria
- Icone di file sul desktop (lettura, disegno, griglia)
- Selezione, drag, rubber-band, frecce
- Menu per icona (apri, copia, rinomina, cestino)
- Rinomina inline (senza KDialog)
- H55 — Calendario al clic sull'orologio
- H56 — Accendere 3 voci dim del menu di contesto
  (Crea nuovo, Incolla, Organizza icone — la 4a, Modo de edição,
  resta dim finché non esiste il sistema di widget)
- H58 — Schermata di blocco (lock screen)
  · protocollo Wayland ext-session-lock · schermata Golden Hour con
    orologio gigante, data e campo password · chiude il pulsante
    "blocca" del menu
  · card (Calendario, Email, ecc.) strettamente passivi: sola
    lettura, non cliccabili, non espongono contenuto sensibile
  · è la base visiva che il tema SDDM e lo splash riutilizzeranno
  · fatia 6 (hardening): hotkey Ctrl+Alt+L · whitelist di tasti
    XF86 (luminosità, volume, media passano anche a schermo bloccato)
    · soppressione delle surface client e del pointer durante il lock
    · OSD renderizzato sopra il lock_bg
- H58.1 — Notifiche nella schermata di blocco
  · pila discreta sulla copertina del lock (solo icona + app + count,
    modalità completa è opt-in) · aggregazione per app a partire da
    visible+history
  · seed LUMA_FAKE_NOTIFICATIONS=N per testare in modalità annidata
    senza far cadere il popup di KDE
- H59 — Sistema di widget, FONDAZIONE (codice pronto, DISATTIVATO
  finché non esiste il modo di edizione)
  · trait Widget (render + tick_interval + tick) nel crate
    luma-widget · registro name→builder nel crate luma ·
    RenderCtx host-aware (Panel { height } / Desktop) ·
    layout.toml come config ([[widget]] con type, placement,
    config)
  · POC: LabelWidget (testo fisso) e CounterWidget (1/s)
  · loop di tick inline nel redraw handler, stessa logica degli
    elapsed-check dell'OSD e dei toast
  · 2 host: fila del pannello + posizione libera sul desktop
    (z=5, dietro le finestre)
  · MIGRAZIONE ESEGUITA: monorepo Cargo workspace con 3 crate
    (luma, luma-theme, luma-widget). Quella nota di "migrare
    dopo i widget" è diventata storia
  · DISATTIVATO PER DECISIONE: il sistema resta SPENTO finché il
    modo di edizione non esiste. Senza modo di edizione, l'utente
    non può aggiungere/spostare/rimuovere widget — solo modificando
    layout.toml a mano, e questa è esperienza da dev, non da utente.
    Si riattiva con 1 uncomment in state.rs quando il modo di
    edizione arriva.

### In corso
- 🔨 Personalizzazione contestuale (stile Mint puro) — FASE in design
  Rinominato da "Modo di edizione generale". DECISIONE: sarà 100%
  stile Mint/Cinnamon, SENZA modo. La configurazione avviene tramite
  right-click diretto dove si trova il problema, come fa Cinnamon.
  Attiva il sistema di widget dormiente (H59). Il numero di H esce
  per fatia.

  Perché Mint puro (non ibrido con KDE):
  · Mint è 1 dei 4 pilastri dell'identità del Luma (Windows 7 menu
    + Linux Mint personalizzazione + COSMIC tech + PCD design)
  · PCD: senza modo significa senza frizione. Il right-click risolve
    senza che la persona debba ricordarsi di entrare/uscire da un modo
  · La "maniglia del widget" stile KDE (alone nel modo di edizione)
    scompare. L'intenzione PCD migra: i menu di right-click hanno
    voci grandi, contrasto forte e hit target ≥ 40px

  Come funziona (modello Cinnamon):
  · right-click sul pannello → menu con aggiungi widget,
    configura pannello, ecc.
  · right-click sul desktop → aggiungi widget, personalizza
    desktop, cambia wallpaper/preset
  · right-click su un widget → configura, sposta, rimuovi quel
    widget specifico
  · NON esiste un "modo di edizione" in cui entrare/uscire

  Conseguenze (scritte per non dimenticare):
  · la voce dim "Modo de edição" del menu di contesto del desktop è
    CANCELLATA (non accesa) — parte dei compiti di questa fase
  · il "Menu di contesto della taskbar" che era nel medio termine è
    assorbito da questa fase (right-click sul pannello È il punto di
    ingresso alla configurazione adesso)
  · la fatia precedente "il modo riposiziona solo le icone del
    desktop" scompare — nello stile Mint, l'utente trascina l'icona
    e funziona già

  Fatie proposte (la chat di design può rifinire):
  1. Menu di right-click (pannello, desktop, widget) + attivare
     H59. L'utente può già aggiungere/spostare/rimuovere widget
  2. Finestra di personalizzazione (gestire widget installati,
     cambiare wallpaper/preset). PUÒ diventare una scheda della
     Schermata di configurazione nativa anziché finestra a sé —
     decidere nella chat di design
  3. Configurare il pannello (altezza, allineamento, creare/rimuovere
     pannello)

  Sblocca alla chiusura della fase: il sistema di widget esce dal
  dormiente + la voce dim del menu è cancellata + il menu di contesto
  della taskbar inizia a esistere.

---

## 🔜 PROSSIME TAPPE (percorso verso il v1.0)

### Breve termine
- (vuoto — le voci sono state promosse a "In corso" o già fatte.
  Il prossimo gruppo esce da "Medio termine")

### Medio termine
- Notifiche fase 2: impilare, raggruppare per app, centrale/storico
- Schermata di configurazione nativa del Luma (include le pagine di
  tema delle Schermate di sistema: splash, login/SDDM, boot — un
  pannello unico, come quello di Colori e Temi di KDE)
- Configurazione dello schermo (risoluzione, monitor)
- Slider di luminosità dentro l'applet della batteria (non è un
  applet a parte — va insieme alla batteria, come fa KDE. Il backend
  esiste già, l'OSD di luminosità lo dimostra; manca solo il
  contenitore cliccabile. Appare solo quando c'è la retroilluminazione,
  stessa logica della batteria)

### App native del Luma
> Programmi propri del Luma, con identità Golden Hour.
> Ciascuno è praticamente un progetto dentro il progetto — farne
> uno alla volta, e solo dopo che la base del DE è matura.
- Gestore di file (il più importante — apre le cartelle del
  desktop, naviga, copia/sposta)
- Strumento di cattura schermo (ritaglio di schermo, finestra o
  area libera, con annotazione. Stile Cmd+Shift+5 del Mac / Strumento
  di Cattura di Windows. PRIORITÀ ALTA: ogni DE serio ce l'ha,
  farlo subito dopo il gestore di file)
- Editor di testo
- Visualizzatore di immagini
- Calcolatrice
- Terminale proprio (opzionale — oggi usa quello del sistema)
- Gestore attività (monitor di sistema completo: lista dei
  processi, terminare app bloccata, schede di CPU / memoria / disco /
  rete. Condivide la lettura di /proc con il widget "monitor di
  sistema" delle idee future — il widget è l'occhiata rapida nel
  pannello, l'app è la versione completa. Bassa priorità, in fondo
  alla fila delle app native, dopo il gestore di file)
- Centro software (modello COSMIC Store, NON store indipendente)
  → vetrina grafica con aspetto Golden Hour per trovare, installare,
    aggiornare e rimuovere app
  → sotto il cofano usa quello che già esiste: libflatpak per Flathub
    + packagekit per i pacchetti di sistema. Luma NON ospita nulla,
    NON fa moderazione, NON ha infrastruttura propria
  → riferimento diretto: il cosmic-store è Rust e open source,
    si può studiare come la System76 ha fatto
  → app sostanziale, bassa priorità, non blocca il v1.0 (all'inizio
    l'utente usa qualsiasi centro software, come il terminale oggi)
- Centro di configurazione (già elencato in "medio termine")

### Infrastruttura / tecnico
- Backend DRM (girare fuori da un altro compositor)
  → sblocca: angoli arrotondati reali, multi-window nativo
  → sblocca ANCHE: VRR e multi-monitor con refresh misto
    (Tier 2 del Gaming) — e questi sono il minimo indispensabile per
    il gamer. In pratica, il DRM merita più priorità di quanto
    sembri: non è solo "angoli arrotondati", sblocca metà di ciò
    che la comunità gamer esige. Salire nella fila del medio termine.
- HiDPI / scalatura — fractional scaling SENZA sfocare lo schermo.
  PRIORITÀ: serve il gamer (monitor ad alta risoluzione) E il lato
  PCD (UI leggibile ad alta densità) allo stesso tempo. Stesso
  lavoro, due pubblici. Non può uscire difettoso.
- Generazione di tema GTK/Qt (le app di terze parti adottano Golden Hour)
- Migrazione a monorepo (Cargo workspace) — ✅ COMPLETATA nell'H59.
  Oggi il Luma è monorepo con 3 crate: luma, luma-theme,
  luma-widget. Nota mantenuta solo per documentare la decisione e il
  momento della migrazione.

### Gaming (fase unica — "togliersi di mezzo al gioco")
> Supporto ai giochi. Sta in una fase sola, non è un mostro. In
> fondo è il compositor che impara a togliersi di mezzo. Smithay già
> fornisce i blocchi per la maggior parte (direct scanout, VRR nel
> backend DRM, tearing) — è più collegare i fili che inventare da
> zero.
> Posizione: dopo la taskbar e il nucleo del DE solido. NON deve
> essere nel v1.0 iniziale. Ma il Tier 1 si può inserire presto,
> perché è poco codice e il guadagno è grande.
>
> PRIORITÀ — ciò che la gente chiede a gran voce NON è ciò per cui
> ti punisce se manca. HDR è la richiesta più rumorosa, ma è la più
> difficile e nessuno stronca un DE nuovo perché manca l'HDR al v1.
> Invece VRR, fractional scaling e multi-monitor sono il minimo
> indispensabile: se mancano o escono difettosi, il gamer prova
> 10 min e se ne va. Questi NON possono uscire difettosi. L'HDR
> resta come la stella del v2, titolone per il surprise drop.

**Tier 1 — essenziale (senza questo il Luma è scadente per i giochi):**
- Direct scanout in fullscreen — il buffer del gioco va diretto
  all'hardware, senza che il Luma copi nulla nel mezzo
- Idle inhibit — protocollo Wayland che impedisce allo schermo di
  spegnersi mentre il gioco gira (es: giocare a RedM col controller
  senza toccare la tastiera). Bug classico, economico da risolvere
- Disattivare i propri effetti — rilevato gioco fullscreen, taglia
  animazioni, blur e ombra. Smette di sprecare GPU

**Tier 2 — importante, può arrivare dopo:**
- VRR (adaptive sync) — toggle per monitor per FreeSync/G-Sync.
  Elimina lo stutter. Dipende dal backend DRM
- Multi-monitor con refresh misto — un 144hz e un 60hz insieme
  senza che il sistema blocchi tutto sul minore. Dipende dal backend
  DRM
- Tearing control — protocollo wp_tearing_control, il gioco chiede
  presentazione asincrona per latenza minima (gioco competitivo)

**Tier 3 — lusso, molto più avanti:**
- Stato del GameMode nella barra (decorativo, opzionale)
- HDR — complesso e troppo nuovo per il v1. Diventa la STELLA del
  v2: è la richiesta più rumorosa della comunità e la lamentela nº 1
  di chi arriva su Linux. Tenuto da parte apposta per fare titolone

### Verso il v1.0

**Schermate di sistema (il "chrome" attorno alla sessione):**
> Come KDE, il Luma le tratta come componenti temabili, tutte
> configurabili in un pannello unico della Schermata di configurazione
> nativa. NON si costruisce un display manager — si tema l'SDDM.
- File di sessione (.desktop) — fa apparire il Luma nella lista di
  SDDM / GDM / qualsiasi display manager. Banale, ma SENZA questo
  nessuno accede al Luma. OBBLIGATORIO per il v1.0.
- Tema SDDM (Golden Hour) — la schermata di login. Un tema QML per
  SDDM, esattamente come fa KDE. DECISIONE: niente greeter proprio
  scritto da zero; si tema l'SDDM, che già esiste. Polish.
- Splash screen — la schermata rapida dopo il login mentre il DE
  carica (il KSplash di KDE). Renderizzato da Luma, leggero, momento
  di brand. Polish, bassa priorità.
- Tema di boot (Plymouth) Golden Hour — solo grafica + config
  piccolo, non si costruisce nulla. Polish economico, alto ritorno.

- Pacchettizzazione e installazione del Luma — è come il DE arriva
  sulla macchina delle persone. Copre: istruzioni di build del codice
  + pacchetti di distro (AUR per Arch, COPR per Fedora, .deb per
  Ubuntu). OBBLIGATORIO per il surprise drop — senza questo la gente
  di r/linux, Phoronix e HN apre il repo e non riesce a testare.
  Va in coppia col file di sessione. (Luma è un DE: NON diventa
  Flatpak, Flatpak è per le app, il DE si installa nel sistema.)
- Polish generale, QA completo
- I 4 preset visivi: Classic, Mint, Dock, PCD Max
- Documentazione finale
- Surprise drop: Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 DEBITI TECNICI ANNOTATI (docs/DEBT.md)
- Angoli arrotondati delle finestre (attende il backend DRM)
- Menu di contesto: 1 voce dim restante (il resto risolto nell'H56):
  · Modo de edição → questa voce sarà CANCELLATA (non accesa) nella
    fase di Personalizzazione contestuale, decisione Mint puro: senza
    modo, configurazione tramite right-click diretto dove si trova il
    problema
- Menu della taskbar → assorbito dalla fase di Personalizzazione
  contestuale (right-click sul pannello È il punto di ingresso alla
  configurazione)

---

## 💭 IDEE FUTURE (post-base, annotate per non dimenticare)
> Funzionalità ispirate a mac/Windows che stanno bene in un DE. NON
> sono priorità — entrano solo dopo la base e le app native.
- Ricerca globale (stile Spotlight) — cercare app, file e
  configurazioni in un posto solo. Espande la ricerca del menu che
  già esiste.
- Snap delle finestre — trascinare verso il bordo incastra la
  finestra a mezza schermata. Funzionalità di produttività.
- Modo scuro/chiaro automatico per orario — il tema segue il
  tramonto. Si abbina all'identità Golden Hour, sarebbe un
  differenziale tematico del Luma.
- Scorciatoie da tastiera personalizzabili dall'utente.
- Anteprima dei file — anteprima selezionando un file sul desktop.
- Selettore di finestre con miniatura — l'Alt+Tab mostra l'anteprima
  di ogni finestra anziché solo l'icona.
- Panoramica delle finestre — stile Exposé/Mission Control del Mac,
  distribuisce tutte le finestre sullo schermo per scegliere.
- Cronologia degli appunti — il Win+V, conserva gli ultimi
  elementi copiati.
- Angoli attivi — spostare il mouse in un angolo attiva un'azione
  (mostrare desktop, aprire menu, ecc.).
- Night light — lo schermo diventa più caldo di notte. Si abbina
  all'identità Golden Hour, come il modo scuro automatico.

### Accessibilità (alimentano il preset PCD Max)
> Queste NON sono "idee future" davvero — sono ciò che dà peso
> all'angolo "DE disegnato da una persona con disabilità" nel surprise
> drop. Meritano di salire nella fila ed entrare prima del v1.0.
- Lente d'ingrandimento — ingrandire un pezzo dello schermo.
- Cursore grande e con evidenziazione — trovare il puntatore
  facilmente.
- Tasti adesivi — premere una scorciatoia un tasto alla volta.

## 🚫 FUORI AMBITO (da non perseguire)
> Feature da azienda gigante — esulano da un DE da portfolio.
- Cronologia delle versioni dei file (tipo Time Machine)
- Continuità tra dispositivi
- Assistente vocale

## 🎯 PRINCIPI DEL PROGETTO
- Una tappa alla volta, completata e testata prima della successiva
- Feature che dipende da un sistema non pronto = voce dim + nota nel DEBT
- Zero hex hardcoded — tutto nel theme.toml
- Commit atomici, ciascuno compila e gira
- Tag di backup prima di ogni tappa
- Accessibilità PCD: hit target ≥ 40px, WCAG AA
