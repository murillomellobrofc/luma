# 🗺️ Luma — Roadmap

> Entorno de escritorio en Rust + Smithay (Wayland)
> Identidad: Golden Hour (coffee / coral / honey / sage)
> Meta: v1.0 como portafolio · cronograma ~2 años · surprise drop
>
> ALCANCE: Luma = el Desktop Environment, corre en cualquier distro.
> Lumina OS (distro propia, con instalador de SO propio) es un
> proyecto SEPARADO, con roadmap propio. NO entra en este archivo.

---

## ✅ YA HECHO

### Base / Compositor
- Compositor Smithay funcionando (Wayland)
- Multi-monitor, multi-workspace
- Corre apps Wayland de verdad
- Renderizado por GPU
- Rust 1.95 + edición 2024
- QA técnico hecho — 0 bugs críticos

### Interfaz
- Menú a pantalla completa híbrido (favoritos, búsqueda, sesión)
- Taskbar estilo KDE (6 zonas)
- Decoración de ventana (titlebar, botones Golden Hour)
- Resolvedor de iconos (GTK + KDE, 7557 iconos)

### Applets / OSD
- Applet de volumen
- OSD de volumen + brillo
- OSD de medios (now playing)
- System tray expandible + SNI (D-Bus)
- H54 — Sistema de notificaciones fase 1 (toast estilo macOS)
- H57 — Applets de wifi, bluetooth y batería (popups)
  · oculta el wifi cuando no hay tarjeta (hardware ausente tratado)
  · red cableada en el popup, hover unificado en todos los applets,
    scan vía D-Bus directo

### Escritorio
- Menú de contexto del escritorio
- Wallpapers Golden Hour por defecto + galería
- Iconos de archivo en el escritorio (leer, dibujar, grilla)
- Selección, drag, rubber-band, flechas
- Menú per-icon (abrir, copiar, renombrar, papelera)
- Renombrar inline (sin KDialog)
- H55 — Calendario al hacer clic en el reloj
- H56 — Activar 3 ítems dim del menú de contexto
  (Crear nuevo, Pegar, Organizar iconos — el 4.º, Modo de edição,
  sigue dim hasta que exista el sistema de widgets)
- H58 — Pantalla de bloqueo (lock screen)
  · protocolo Wayland ext-session-lock · pantalla Golden Hour con
    reloj gigante, fecha y campo de contraseña · cierra el botón
    "bloquear" del menú
  · cards (Calendario, Email, etc.) estrictamente pasivos: solo
    lectura, no se hace clic, no filtran contenido sensible
  · es la base visual que el tema del SDDM y el splash van a reusar
  · rebanada 6 (hardening): hotkey Ctrl+Alt+L · whitelist de teclas
    XF86 (brillo, volumen, medios pasan aun con la pantalla
    bloqueada) · supresión de surfaces de cliente y de pointer
    durante el lock · OSD renderiza por encima del lock_bg
- H58.1 — Notificaciones en la pantalla de bloqueo
  · pila discreta en la portada del lock (solo icono + app + count,
    modo completo es opt-in) · agregación por app a partir de
    visible+history · seed LUMA_FAKE_NOTIFICATIONS=N para probar en
    modo anidado sin tumbar el popup de KDE
- H59 — Sistema de widgets, FUNDACIÓN (código listo, DESACTIVADO
  hasta que exista modo de edición)
  · trait Widget (render + tick_interval + tick) en el crate
    luma-widget · registro name→builder en el crate luma ·
    RenderCtx host-aware (Panel { height } / Desktop) ·
    layout.toml como config ([[widget]] con type, placement,
    config)
  · POCs: LabelWidget (texto fijo) y CounterWidget (1/s)
  · loop de tick inline en el redraw handler, misma cara que los
    elapsed-checks del OSD y de los toasts
  · 2 hosts: hilera del panel + posición libre en el escritorio
    (z=5, detrás de ventanas)
  · MIGRACIÓN REALIZADA: monorepo Cargo workspace con 3 crates
    (luma, luma-theme, luma-widget). Aquella nota de "migrar
    después de los widgets" se convirtió en historia
  · DESACTIVADO POR DECISIÓN: el sistema queda APAGADO hasta que
    el modo de edición exista. Sin modo de edición, el usuario no
    tiene cómo agregar/mover/quitar widget — solo editando
    layout.toml a mano, y eso es experiencia de dev, no de
    usuario. Se reactiva con 1 uncomment en state.rs cuando el
    modo de edición llegue.

### En curso
- 🔨 Personalización contextual (estilo Mint puro) — FASE en diseño
  Renombrado a partir de "Modo de edición general". DECISIÓN: va
  100% estilo Mint/Cinnamon, SIN modo. La configuración ocurre por
  right-click directo donde está el problema, igual que Cinnamon.
  Activa el sistema de widgets durmiente (H59). Número de H sale por
  rebanada.

  Por qué Mint puro (no híbrido con KDE):
  · Mint es 1 de los 4 pilares de la identidad de Luma (Windows 7
    menú + Linux Mint personalización + COSMIC tech + PCD diseño)
  · PCD: sin modo significa sin fricción. Right-click resuelve sin
    que la persona necesite recordar entrar/salir de modo
  · El "asa del widget" estilo KDE (halo en el modo de edición)
    desaparece. La intención PCD migra: los menús de right-click
    tienen ítems grandes, contraste fuerte y hit target ≥ 40px

  Cómo funciona (modelo Cinnamon):
  · right-click en el panel → menú con agregar widget,
    configurar panel, etc.
  · right-click en el escritorio → agregar widget, personalizar
    escritorio, cambiar wallpaper/preset
  · right-click en un widget → configurar, mover, quitar ese
    widget específico
  · NO existe "modo de edición" para entrar/salir

  Consecuencias (escritas para no olvidar):
  · el ítem dim "Modo de edição" del menú de contexto del
    escritorio es BORRADO (no activado) — parte de las tareas de
    esta fase
  · el "Menú de contexto de la taskbar" que estaba en el mediano
    plazo es absorbido por esta fase (right-click en el panel ES la
    entrada de configuración ahora)
  · la rebanada antigua "modo solo reposiciona iconos del
    escritorio" desaparece — en el estilo Mint, el usuario solo
    arrastra el icono, ya funciona

  Rebanadas propuestas (el chat de diseño puede refinar):
  1. Menús de right-click (panel, escritorio, widget) + activar el
     H59. El usuario ya puede agregar/mover/quitar widget
  2. Diálogo de personalización (gestionar widgets instalados,
     cambiar wallpaper/preset). PUEDE convertirse en pestaña de la
     Pantalla de configuración nativa en vez de diálogo suelto —
     decidir en el chat de diseño
  3. Configurar panel (altura, alineación, crear/quitar panel)

  Desbloquea al cerrar la fase: el sistema de widgets sale del
  estado durmiente + el ítem dim del menú es borrado + el menú de
  contexto de la taskbar pasa a existir.

---

## 🔜 PRÓXIMOS PASOS (camino al v1.0)

### Corto plazo
- (vacío — ítems promovidos a "En curso" o ya hechos.
  La próxima hornada sale de "Mediano plazo")

### Mediano plazo
- Notificaciones fase 2: apilar, agrupar por app, centro/historial
- Pantalla de configuración nativa de Luma (incluye las páginas de
  tema de las Pantallas del sistema: splash, login/SDDM, boot — un
  solo panel, igual que el de Colores y Temas de KDE)
- Configuración de pantalla (resolución, monitores)
- Slider de brillo dentro del applet de batería (no es applet
  propio — va junto a la batería, igual que KDE. El backend ya
  existe, el OSD de brillo lo demuestra; falta solo la cáscara
  clickeable. Aparece solo cuando hay backlight, misma lógica de
  la batería)

### Apps nativos de Luma
> Programas propios de Luma, con identidad Golden Hour.
> Cada uno es prácticamente un proyecto dentro del proyecto — hacer
> uno a la vez, y solo después de que la base del DE esté madura.
- Gestor de archivos (el más importante — abre las carpetas
  del escritorio, navega, copia/mueve)
- Herramienta de captura de pantalla (recorte de pantalla, ventana o
  área libre, con anotación. Estilo Cmd+Shift+5 de Mac / Herramienta
  de Recortes de Windows. PRIORIDAD ALTA: todo DE serio tiene esto,
  hacer enseguida después del gestor de archivos)
- Editor de texto
- Visor de imágenes
- Calculadora
- Terminal propia (opcional — hoy usa la del sistema)
- Administrador de tareas (monitor de sistema completo: lista de
  procesos, matar app trabada, pestañas de CPU / memoria / disco /
  red. Comparte la lectura de /proc con el widget "monitor de
  sistema" de las ideas futuras — widget es la mirada rápida en el
  panel, la app es la cosa completa. Baja prioridad, final de la fila
  de apps nativos, después del gestor de archivos)
- Centro de software (modelo COSMIC Store, NO tienda independiente)
  → vitrina gráfica con cara Golden Hour para encontrar, instalar,
    actualizar y eliminar apps
  → por debajo usa lo que ya existe: libflatpak para Flathub +
    packagekit para los paquetes del sistema. Luma NO aloja nada,
    NO modera, NO tiene infra propia
  → referencia directa: cosmic-store es Rust y código abierto,
    se puede estudiar cómo lo hizo System76
  → app sustancial, baja prioridad, no traba el v1.0 (al principio
    el usuario usa cualquier centro de software, igual que el
    terminal hoy)
- Centro de configuración (ya listado en "mediano plazo")

### Infra / técnico
- Backend DRM (correr fuera de otro compositor)
  → desbloquea: esquinas redondeadas reales, multi-window nativo
  → desbloquea TAMBIÉN: VRR y multi-monitor con refresh mixto
    (Tier 2 de Gaming) — y esos son arroz con frijoles para el
    gamer. O sea, el DRM vale más prioridad de la que parece: no es
    solo "esquinas redondeadas", destranca la mitad de lo que la
    comunidad gamer exige. Subirlo en la fila del mediano plazo.
- HiDPI / escalado — fractional scaling SIN emborronar la pantalla.
  PRIORIDAD: sirve al gamer (monitor de alta resolución) Y al lado
  PCD (UI legible en alta densidad) al mismo tiempo. Mismo
  trabajo, dos públicos. No puede shipear roto.
- Generación de tema GTK/Qt (apps de terceros adoptan Golden Hour)
- Migración a monorepo (Cargo workspace) — ✅ COMPLETADA en H59.
  Hoy Luma es monorepo con 3 crates: luma, luma-theme,
  luma-widget. Nota mantenida solo para documentar la decisión y el
  momento de la migración.

### Gaming (fase única — "salirse del camino del juego")
> Soporte de juegos. Cabe en una sola fase, no es un monstruo. En el
> fondo es el compositor aprendiendo a salirse del camino. Smithay ya
> entrega los bloques para la mayoría (direct scanout, VRR en el
> backend DRM, tearing) — es más conectar cables que inventar de cero.
> Posición: después de la taskbar y el núcleo del DE firme. NO necesita
> estar en el v1.0 inicial. Pero el Tier 1 se puede meter temprano,
> porque es poco código y la ganancia es grande.
>
> PRIORIDAD — lo que la gente pide a gritos NO es por lo que te
> castiga si falta. HDR es el pedido más ruidoso, pero es el más
> difícil y nadie destruye un DE nuevo por no tener HDR en v1.
> En cambio VRR, fractional scaling y multi-monitor son arroz con
> frijoles: si faltan o vienen rotos, el gamer prueba 10 min y se
> va. Esos NO pueden shipear rotos. HDR queda como la estrella
> del v2, titular para el surprise drop.

**Tier 1 — esencial (sin esto Luma es malo para juegos):**
- Direct scanout en fullscreen — el buffer del juego va directo al
  hardware, sin que Luma copie nada en el medio
- Idle inhibit — protocolo Wayland que impide que la pantalla se
  apague mientras el juego corre (ej.: jugar RedM con control sin
  tocar el teclado). Bug clásico, barato de resolver
- Apagar los propios efectos — detectó juego fullscreen, corta
  animación, blur y sombra. Deja de gastar GPU al pedo

**Tier 2 — importante, puede venir después:**
- VRR (adaptive sync) — toggle por monitor para FreeSync/G-Sync.
  Mata el stutter. Depende del backend DRM
- Multi-monitor con refresh mixto — un 144Hz y un 60Hz juntos
  sin que el sistema clave todo en el más lento. Depende del
  backend DRM
- Tearing control — protocolo wp_tearing_control, el juego pide
  presentación asíncrona para latencia mínima (juego competitivo)

**Tier 3 — lujo, mucho después:**
- Status del GameMode en la barra (adorno, opcional)
- HDR — demasiado complejo y nuevo para v1. Se convierte en la
  ESTRELLA del v2: es el pedido más ruidoso de la comunidad y la
  queja n.º 1 de quien llega a Linux. Guardado a propósito para
  dar titular

### Camino al v1.0

**Pantallas del sistema (el "chrome" alrededor de la sesión):**
> Igual que KDE, Luma trata estas como componentes temáticos,
> todas configurables en un solo panel de la Pantalla de
> configuración nativa. NO se construye display manager — se
> tematiza el SDDM.
- Archivo de sesión (.desktop) — hace que Luma aparezca en la lista
  del SDDM / GDM / cualquier display manager. Trivial, pero SIN
  esto nadie inicia sesión en Luma. OBLIGATORIO para v1.0.
- Tema del SDDM (Golden Hour) — la pantalla de login. Un tema QML
  para el SDDM, exactamente como hace KDE. DECISIÓN: nada de
  greeter propio escrito de cero; se tematiza el SDDM, que ya
  existe. Polish.
- Splash screen — la pantalla rápida después del login mientras el
  DE carga (el KSplash de KDE). Luma-rendered, liviano, momento de
  marca. Polish, baja prioridad.
- Tema de boot (Plymouth) Golden Hour — solo gráfico + config
  chico, no se construye nada. Polish barato, alto retorno.

- Empaquetado e instalación de Luma — es cómo el DE llega a la
  máquina de la gente. Cubre: instrucciones de build del código +
  paquetes de distro (AUR para Arch, COPR para Fedora, .deb para
  Ubuntu). OBLIGATORIO para el surprise drop — sin esto la gente de
  r/linux, Phoronix y HN abre el repo y no puede probarlo.
  Va de la mano con el archivo de sesión. (Luma es DE: NO se vuelve
  Flatpak, Flatpak es para app, el DE se instala en el sistema.)
- Polish general, QA completo
- Los 4 presets visuales: Classic, Mint, Dock, PCD Max
- Documentación final
- Surprise drop: Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 DEUDAS TÉCNICAS ANOTADAS (docs/DEBT.md)
- Esquinas redondeadas de ventana (espera backend DRM)
- Menú de contexto: 1 ítem dim restante (resuelto el resto en H56):
  · Modo de edição → este ítem será BORRADO (no activado) en la
    fase de Personalización contextual, decisión Mint puro: sin
    modo, configuración por right-click directo donde está el
    problema
- Menú de la taskbar → absorbido por la fase de Personalización
  contextual (right-click en el panel ES la entrada de configuración)

---

## 💭 IDEAS FUTURAS (post-base, anotadas para no olvidar)
> Funciones inspiradas en mac/Windows que caben en un DE. NO son
> prioridad — entran solo después de la base y las apps nativos.
- Búsqueda global (estilo Spotlight) — buscar apps, archivos y
  configuraciones en un solo lugar. Expande la búsqueda del menú
  que ya existe.
- Snap de ventanas — arrastrar al borde encaja la ventana en media
  pantalla. Función de productividad.
- Modo oscuro/claro automático por horario — el tema acompaña la
  puesta del sol. Combina con la identidad Golden Hour, sería un
  diferencial temático de Luma.
- Atajos de teclado personalizables por el usuario.
- Preview de archivos — vista previa al seleccionar un archivo en
  el escritorio.
- Cambiador de ventanas con miniatura — el Alt+Tab mostrando
  preview de cada ventana en vez de solo icono.
- Vista general de ventanas — estilo Exposé/Mission Control de Mac,
  esparce todas las ventanas en la pantalla para elegir.
- Historial de portapapeles — el Win+V, guarda los últimos ítems
  copiados.
- Esquinas activas — llevar el mouse a una esquina dispara una
  acción (mostrar escritorio, abrir menú, etc.).
- Night light — la pantalla se calienta de color de noche. Combina
  con la identidad Golden Hour, igual que el modo oscuro automático.

### Accesibilidad (alimentan el preset PCD Max)
> Estas NO son "idea futura" de verdad — son lo que da peso al
> ángulo "DE diseñado por persona con discapacidad" en el surprise
> drop. Merecen subir en la fila y entrar antes del v1.0.
- Lupa de pantalla — ampliar un pedazo de la pantalla.
- Cursor grande y con realce — encontrar el puntero fácil.
- Teclas adhesivas — presionar atajo una tecla a la vez.

## 🚫 FUERA DE ALCANCE (no perseguir)
> Funciones de mega-corporación — escapan de un DE de portafolio.
- Historial de versiones de archivo (estilo Time Machine)
- Continuidad entre dispositivos
- Asistente de voz

## 🎯 PRINCIPIOS DEL PROYECTO
- Una etapa a la vez, terminada y testeada antes de la siguiente
- Feature que depende de sistema no listo = ítem dim + nota en DEBT
- Cero hex hardcoded — todo en theme.toml
- Commits atómicos, cada uno compila y corre
- Backup tag antes de cada etapa
- Accesibilidad PCD: hit targets ≥ 40px, WCAG AA
