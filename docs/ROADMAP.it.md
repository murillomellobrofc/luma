# 🗺️ Luma — Roadmap

> Desktop Environment in Rust + Smithay (Wayland).
> Identità: Golden Hour (coffee / coral / honey / sage).
> Obiettivo: v1.0 come portfolio · timeline ~2 anni · surprise drop.

---

## ✅ Fatto

### Base / Compositor

- Compositor Smithay funzionante (Wayland)
- Multi-monitor, multi-workspace
- Esegue app Wayland reali
- Rendering via GPU
- Rust 1.95 + edition 2024
- QA tecnico fatto — 0 bug critici

### Interfaccia

- Menu fullscreen ibrido (preferiti, ricerca, sessione)
- Taskbar in stile KDE (6 zone)
- Decorazione finestra (titlebar, pulsanti Golden Hour)
- Risolutore di icone (GTK + KDE, 7557 icone)

### Applet / OSD

- Applet del volume
- OSD di volume + luminosità
- OSD multimediale (now playing)
- System tray espandibile + SNI (D-Bus)
- H54 — Sistema di notifiche fase 1 (toast in stile macOS)
- **H57 — Applet di wifi, bluetooth e batteria** (popup)
  - Nasconde il wifi quando manca la scheda (hardware assente gestito)
  - Connessione cablata nel popup, hover unificato in tutti gli
    applet, scan tramite D-Bus diretto

### Desktop

- Menu contestuale del desktop
- Wallpaper Golden Hour di default + galleria
- Icone dei file sul desktop (leggere, disegnare, griglia)
- Selezione, drag, rubber-band, frecce
- Menu per-icona (apri, copia, rinomina, cestino)
- Rinomina inline (senza KDialog)
- **H55 — Calendario al click sull'orologio**
- **H56 — Attiva 3 item dim del menu contestuale**
  (Crea nuovo, Incolla, Organizza icone — il 4°, Modalità modifica,
  resta dim fino a quando esisterà il sistema di widget)

### In corso

- 🔨 **H58 — Schermata di blocco (lock screen)**
  Protocollo Wayland `ext-session-lock` · schermata Golden Hour con
  campo password e info utente · peso di sicurezza (blocco bypassabile
  = bug serio) · chiude il pulsante "blocca" del menu · crea il
  linguaggio visivo che il tema SDDM e lo splash riusano.

---

## 🔜 Prossimi passi (cammino verso la v1.0)

### Breve termine

(vuoto — item promossi a "In corso" o già fatti. Il prossimo lotto
esce da "Medio termine")

### Medio termine

- **Notifiche fase 2**: impilare, raggruppare per app, centro/storico

- **Sistema di widget configurabili** (in stile KDE Plasma)
  - Widget che l'utente aggiunge/rimuove/configura nel pannello e sul
    desktop (orologio, meteo, note, monitor di sistema, etc).
  - **Decisione**: widget = Luma-native in Rust, renderizzati dalla
    stessa luma-bar. NON sono GTK né Qt. Come i plasmoid di KDE, che
    non sono app Qt agganciabili. Solo renderizzando noi stessi si
    garantisce Golden Hour + hit target ≥ 40px.
  - GTK/Qt in Luma entra da un altro percorso: app di terzi come
    finestra (Wayland), tray via SNI, e generazione di tema GTK/Qt.
  - È una FONDAZIONE architetturale grande — una delle tappe maggiori
    del roadmap. Farla solo quando la base del DE sarà matura.
  - Sblocca: menu della taskbar + modalità modifica generale.

- **Modalità modifica generale di Luma** (in stile KDE, unificata)
  - Una modalità dove l'utente organizza tutto in un posto solo:
    riposiziona le icone del desktop E lavora su widget/layout dei
    pannelli.
  - Dipende dalle icone del desktop (✅ già c'è) + sistema di widget
    (❌ ancora no) — per questo arriva insieme/dopo i widget.
  - È ciò che attiva l'item dim "Modalità modifica" del menu
    contestuale.
  - Unisce 3 item del roadmap in uno: item dim del menu + sistema di
    widget + questa modalità di modifica generale.

  **Come funziona:**
  - Si entra dall'item "Modalità modifica" del menu contestuale +
    scorciatoia da tastiera; si esce con il pulsante "Fatto" o Esc
    (tutto si blocca di nuovo).
  - La schermata guadagna un overlay sottile, pannelli/widget hanno
    bordo evidenziato, barra in alto con "Aggiungi widget" e "Fatto".
  - Permette di: riposizionare icone del desktop · aggiungere/
    spostare/rimuovere/configurare widget (desktop e pannelli) ·
    lavorare sui pannelli (altezza, allineamento, fissare/flottare,
    crearne uno nuovo) · cambiare wallpaper e preset.

  **Maniglia del widget — differenziale PCD:**
  - Ogni widget focalizzato mostra una maniglia con pulsanti
    (configura, sposta, ridimensiona, rimuovi).
  - In KDE queste maniglie sono minuscole e fastidiose — in Luma,
    hit target ≥ 40px, pulsanti grandi, colore Golden Hour.
  - Tagliare il "ruota" di KDE (nessuno lo usa).
  - Modalità modifica usabile da chi ha tremore / bassa visione = la
    storia del PCD Max nella pratica.

  **Slice** (ognuna compila e gira da sola):
  1. Modalità che riposiziona solo le icone del desktop (base già c'è)
  2. + aggiungere/spostare/rimuovere/configurare widget
  3. + modifica del pannello (altezza, allineamento, crea pannello)

- **Menu contestuale della taskbar** (dipende dai widget)
- **Schermata di configurazione nativa di Luma** (include le pagine
  di tema delle Schermate di sistema: splash, login/SDDM, boot — un
  pannello solo, come Colori e Temi di KDE)
- **Configurazione schermo** (risoluzione, monitor)
- **Slider di luminosità dentro l'applet della batteria** — non è
  applet a sé, va insieme alla batteria, come fa KDE. Il backend già
  esiste, l'OSD di luminosità lo dimostra; manca solo il guscio
  cliccabile. Appare solo quando c'è backlight, stessa logica della
  batteria.

### App native di Luma

> Programmi propri di Luma, con identità Golden Hour. Ognuno è
> praticamente un progetto dentro il progetto — farne uno alla volta,
> e solo dopo che la base del DE sarà matura.

- **Gestore file** (il più importante — apre le cartelle del desktop,
  naviga, copia/sposta)
- **Strumento di cattura schermo** (ritaglio di schermo, finestra o
  area libera, con annotazione. Stile Cmd+Shift+5 del Mac /
  Strumento di Cattura di Windows. PRIORITÀ ALTA: ogni DE serio ce
  l'ha, farlo subito dopo il gestore file)
- **Editor di testo**
- **Visualizzatore di immagini**
- **Calcolatrice**
- **Terminale proprio** (opzionale — oggi si usa quello di sistema)
- **Gestore attività** (monitor di sistema completo: lista processi,
  uccidere app bloccate, schede di CPU / memoria / disco / rete.
  Condivide la lettura di /proc con il widget "monitor di sistema"
  delle idee future — widget è lo sguardo rapido sul pannello, l'app
  è la cosa completa. Bassa priorità, fine della fila delle app
  native, dopo il gestore file)
- **Centro software** (modello COSMIC Store, NON store indipendente)
  - Vetrina grafica con look Golden Hour per trovare, installare,
    aggiornare e rimuovere app.
  - Sotto usa ciò che già esiste: libflatpak per Flathub + packagekit
    per i pacchetti di sistema. Luma NON ospita niente, NON modera,
    NON ha infrastruttura propria.
  - Riferimento diretto: cosmic-store è Rust e open source, si può
    studiare come ha fatto System76.
  - App sostanziosa, bassa priorità, non blocca la v1.0 (all'inizio
    l'utente usa qualsiasi centro software, come il terminale oggi).
- **Centro di configurazione** (già elencato in "medio termine")

### Infra / tecnico

- **Backend DRM** (girare fuori da un altro compositor)
  - Sblocca: angoli arrotondati reali, multi-window nativo.
  - Sblocca ANCHE: VRR e multi-monitor con refresh misto (Tier 2 del
    Gaming) — e quelli sono pane e burro per il gamer.
  - Quindi, il DRM vale più priorità di quanto sembri: non è solo
    "angoli arrotondati", sblocca metà di ciò che la community gamer
    chiede. Spostarlo in alto nella fila del medio termine.

- **HiDPI / scaling** — fractional scaling SENZA sfocare lo schermo.
  PRIORITÀ: serve al gamer (monitor ad alta risoluzione) E al lato
  PCD (UI leggibile in alta densità) allo stesso tempo. Stesso
  lavoro, due pubblici. Non può uscire rotto.

- **Generazione tema GTK/Qt** (app di terzi adottano Golden Hour)

### Gaming (fase unica — "togliersi dai piedi del gioco")

> Supporto giochi. Sta in una fase sola, non è un mostro. In fondo è
> il compositor che impara a farsi da parte. Smithay già fornisce i
> blocchi per gran parte (direct scanout, VRR nel backend DRM,
> tearing) — è più collegare cavi che inventare da zero.
>
> **Posizione**: dopo la taskbar e il nucleo del DE solido. NON serve
> che sia nella v1.0 iniziale. Ma il Tier 1 si può inserire presto,
> perché è poco codice e il guadagno è grande.
>
> **Priorità** — ciò che la gente chiede a voce alta NON è ciò per cui
> ti punisce se manca. HDR è la richiesta più rumorosa, ma è la più
> difficile e nessuno demolisce un DE nuovo per mancanza di HDR nella
> v1. Invece VRR, fractional scaling e multi-monitor sono pane e
> burro: se mancano o arrivano rotti, il gamer prova 10 min e se ne va.
> Quelli NON possono uscire rotti. HDR resta come stella della v2,
> titolo per il surprise drop.

**Tier 1 — essenziale** (senza questo Luma è cattivo per i giochi):

- **Direct scanout in fullscreen** — il buffer del gioco va dritto
  all'hardware, senza che Luma copi niente in mezzo.
- **Idle inhibit** — protocollo Wayland che impedisce allo schermo di
  spegnersi mentre il gioco gira (es: giocare RedM col controller
  senza toccare la tastiera). Bug classico, costoso poco da risolvere.
- **Spegnere i propri effetti** — rilevato gioco fullscreen, taglia
  animazione, blur e ombra. Smette di sprecare GPU.

**Tier 2 — importante, può venire dopo:**

- **VRR (adaptive sync)** — toggle per monitor per FreeSync/G-Sync.
  Uccide lo stutter. Dipende dal backend DRM.
- **Multi-monitor con refresh misto** — un 144Hz e un 60Hz insieme
  senza che il sistema blocchi tutto sul più lento. Dipende dal
  backend DRM.
- **Tearing control** — protocollo `wp_tearing_control`, il gioco
  chiede presentazione asincrona per latenza minima (gioco
  competitivo).

**Tier 3 — lusso, molto più tardi:**

- **Status di GameMode nella barra** (decoro, opzionale).
- **HDR** — troppo complesso e troppo nuovo per la v1. Diventa la
  STELLA della v2: è la richiesta più rumorosa della community e il
  reclamo n.º 1 di chi arriva su Linux. Tenuto apposta per fare
  titolo.

### Verso la v1.0

**Schermate di sistema** (il "chrome" intorno alla sessione):

> Come KDE, Luma tratta queste come componenti temabili, tutte
> configurabili in un solo pannello della Schermata di configurazione
> nativa. NON si costruisce display manager — si tema SDDM.

- **File di sessione** (`.desktop`) — fa apparire Luma nella lista di
  SDDM / GDM / qualsiasi display manager. Banale, ma SENZA questo
  nessuno fa login in Luma. OBBLIGATORIO per la v1.0.
- **Tema SDDM (Golden Hour)** — la schermata di login. Un tema QML
  per SDDM, esattamente come fa KDE. DECISIONE: niente greeter
  proprio scritto da zero; si tema SDDM, che già esiste. Polish.
- **Splash screen** — la schermata rapida dopo il login mentre il DE
  carica (il KSplash di KDE). Luma-rendered, leggera, momento di
  brand. Polish, bassa priorità.
- **Tema di boot (Plymouth) Golden Hour** — solo grafico + config
  piccola, non si costruisce niente. Polish economico, alto ritorno.

**Chiusura:**

- Polish generale, QA completo
- I 4 preset visivi: Classic, Mint, Dock, PCD Max
- Documentazione finale
- Surprise drop: Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 Debiti tecnici (`docs/DEBT.md`)

- Angoli arrotondati delle finestre (aspetta backend DRM)
- Notifiche: impilare / centro / azioni / suono
- Menu contestuale: 1 item dim rimanente (resto risolto in H56):
  - Modalità modifica → aspetta sistema di widget + modalità modifica
    generale
- Menu della taskbar (aspetta sistema di widget)
- Pulsante "blocca" del menu di sessione → aspetta la schermata di
  blocco (`ext-session-lock`). Finché non esiste, il pulsante non ha
  niente da fare.

---

## 💭 Idee future (post-base, annotate per non dimenticare)

> Funzioni ispirate a mac/Windows che ci stanno in un DE. NON sono
> priorità — entrano solo dopo la base e le app native.

- **Ricerca globale** (in stile Spotlight) — cercare app, file e
  configurazioni in un posto solo. Estende la ricerca del menu che
  già c'è.
- **Snap di finestre** — trascinare al bordo aggancia la finestra a
  metà schermo. Funzione di produttività.
- **Modalità scuro/chiaro automatica per orario** — il tema segue il
  tramonto. Si abbina all'identità Golden Hour, sarebbe un
  differenziale tematico di Luma.
- **Scorciatoie da tastiera personalizzabili** dall'utente.
- **Anteprime file** — anteprima al selezionare un file sul desktop.
- **Cambia finestre con miniatura** — l'Alt+Tab che mostra preview di
  ogni finestra invece che solo icona.
- **Panoramica finestre** — stile Exposé/Mission Control di Mac,
  sparge tutte le finestre sullo schermo per scegliere.
- **Storico appunti** — il Win+V, tiene gli ultimi item copiati.
- **Angoli attivi** — buttare il mouse in un angolo scatena
  un'azione (mostra desktop, apri menu, etc).
- **Night light** — lo schermo si scalda di colore di notte. Si
  abbina all'identità Golden Hour, come la modalità scura automatica.

### Accessibilità (alimentano il preset PCD Max)

> Queste NON sono "idea futura" davvero — sono ciò che dà peso
> all'angolo "DE disegnato da persona con disabilità" nel surprise
> drop. Meritano di salire nella fila e entrare prima della v1.0.

- **Lente d'ingrandimento** — ingrandire un pezzo dello schermo.
- **Cursore grande ed evidenziato** — trovare il puntatore facile.
- **Tasti permanenti** — premere scorciatoia un tasto alla volta.

---

## 🚫 Fuori scopo (non perseguire)

> Funzioni da mega-azienda — fuggono da un DE di portfolio.

- Storia delle versioni di file (stile Time Machine)
- Continuità tra dispositivi
- Assistente vocale

---

## 🎯 Principi del progetto

- **Una tappa alla volta**, finita e testata prima della successiva
- Feature che dipende da un sistema non pronto = item dim + nota nel DEBT
- **Zero hex hardcoded** — tutto in `theme.toml`
- **Commit atomici**, ognuno compila e gira
- **Backup tag** prima di ogni tappa
- **Accessibilità PCD**: hit target ≥ 40px, WCAG AA
