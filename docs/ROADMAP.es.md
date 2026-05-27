# 🗺️ Luma — Roadmap

> Entorno de escritorio en Rust + Smithay (Wayland).
> Identidad: Golden Hour (coffee / coral / honey / sage).
> Meta: v1.0 como portafolio · cronograma ~2 años · surprise drop.

---

## ✅ Hecho

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
- **H57 — Applets de wifi, bluetooth y batería** (popups)
  - Oculta el wifi cuando no hay tarjeta (hardware ausente tratado)
  - Red cableada en el popup, hover unificado en todos los applets,
    scan vía D-Bus directo

### Escritorio

- Menú de contexto del escritorio
- Wallpapers Golden Hour por defecto + galería
- Iconos de archivo en el escritorio (leer, dibujar, grilla)
- Selección, drag, rubber-band, flechas
- Menú per-icon (abrir, copiar, renombrar, papelera)
- Renombrar inline (sin KDialog)
- **H55 — Calendario al hacer click en el reloj**
- **H56 — Activar 3 items dim del menú de contexto**
  (Crear nuevo, Pegar, Organizar iconos — el 4º, Modo de edición,
  sigue dim hasta que exista el sistema de widgets)

### En curso

- 🔨 **H58 — Pantalla de bloqueo (lock screen)**
  Protocolo Wayland `ext-session-lock` · pantalla Golden Hour con
  campo de contraseña e info del usuario · peso de seguridad
  (bloqueo evadible = bug serio) · cierra el botón "bloquear" del
  menú · crea el lenguaje visual que el tema de SDDM y el splash
  reutilizan.

---

## 🔜 Próximos pasos (camino al v1.0)

### Corto plazo

(vacío — items promovidos a "En curso" o ya hechos. La próxima
hornada sale de "Mediano plazo")

### Mediano plazo

- **Notificaciones fase 2**: apilar, agrupar por app, centro/historial

- **Sistema de widgets configurables** (estilo KDE Plasma)
  - Widgets que el usuario agrega/quita/configura en el panel y en
    el escritorio (reloj, clima, notas, monitor de sistema, etc).
  - **Decisión**: widgets = Luma-native en Rust, renderizados por
    la propia luma-bar. NO son GTK ni Qt. Como los plasmoids de KDE,
    que no son apps Qt enchufables. Solo renderizando nosotros
    mismos se garantiza Golden Hour + hit target ≥ 40px.
  - GTK/Qt en Luma entra por otro camino: apps de terceros como
    ventana (Wayland), bandeja vía SNI, y generación de tema GTK/Qt.
  - Es una FUNDACIÓN arquitectónica grande — una de las mayores
    etapas del roadmap. Hacerla solo cuando la base del DE esté
    madura.
  - Desbloquea: menú de la taskbar + modo de edición general.

- **Modo de edición general de Luma** (estilo KDE, unificado)
  - Un modo donde el usuario organiza todo en un solo lugar:
    reposiciona iconos del escritorio Y maneja widgets/layout
    de los paneles.
  - Depende de los iconos del escritorio (✅ ya está) + sistema de
    widgets (❌ aún no) — por eso viene junto/después de los widgets.
  - Es lo que activa el item dim "Modo de edición" del menú de
    contexto.
  - Junta 3 items del roadmap en uno: item dim del menú + sistema
    de widgets + este modo de edición general.

  **Cómo funciona:**
  - Se entra por el item "Modo de edición" del menú de contexto +
    atajo de teclado; se sale con botón "Listo" o Esc (todo se
    traba de nuevo).
  - La pantalla gana un overlay sutil, paneles/widgets quedan con
    borde de realce, barra arriba con "Agregar widget" y "Listo".
  - Permite: reposicionar iconos del escritorio · agregar/mover/
    quitar/configurar widgets (escritorio y paneles) · ajustar los
    paneles (altura, alineación, fijar/flotar, crear nuevo) ·
    cambiar wallpaper y preset.

  **Asa del widget — diferencial PCD:**
  - Cada widget enfocado muestra un asa con botones (configurar,
    mover, redimensionar, eliminar).
  - En KDE estas asas son minúsculas y molestas — en Luma, hit
    target ≥ 40px, botones grandes, color Golden Hour.
  - Cortar el "rotar" de KDE (nadie lo usa).
  - Modo de edición usable por quien tiene temblor / baja visión =
    la historia del PCD Max en la práctica.

  **Rebanadas** (cada una compila y corre sola):
  1. Modo que solo reposiciona iconos del escritorio (base ya existe)
  2. + agregar/mover/quitar/configurar widgets
  3. + edición de panel (altura, alineación, crear panel)

- **Menú de contexto de la taskbar** (depende de los widgets)
- **Pantalla de configuración nativa de Luma** (incluye las páginas
  de tema de las Pantallas del sistema: splash, login/SDDM, boot —
  un solo panel, igual que el de Colores y Temas de KDE)
- **Configuración de pantalla** (resolución, monitores)
- **Slider de brillo dentro del applet de batería** — no es applet
  propio, va junto a la batería, como hace KDE. El backend ya existe,
  el OSD de brillo lo prueba; falta solo la cáscara clickeable.
  Aparece solo cuando hay backlight, misma lógica que la batería.

### Apps nativas de Luma

> Programas propios de Luma, con identidad Golden Hour. Cada uno es
> prácticamente un proyecto dentro del proyecto — hacer uno a la vez,
> y solo después de que la base del DE esté madura.

- **Gestor de archivos** (el más importante — abre las carpetas del
  escritorio, navega, copia/mueve)
- **Herramienta de captura de pantalla** (recorte de pantalla, ventana
  o área libre, con anotación. Estilo Cmd+Shift+5 de Mac / Herramienta
  de Recortes de Windows. PRIORIDAD ALTA: todo DE serio tiene esto,
  hacer enseguida después del gestor de archivos)
- **Editor de texto**
- **Visor de imágenes**
- **Calculadora**
- **Terminal propia** (opcional — hoy usa la del sistema)
- **Administrador de tareas** (monitor de sistema completo: lista de
  procesos, matar app trabada, pestañas de CPU / memoria / disco /
  red. Comparte la lectura de /proc con el widget "monitor de sistema"
  de las ideas futuras — widget es la mirada rápida en el panel, app
  es la cosa completa. Baja prioridad, final de la fila de apps
  nativas, después del gestor de archivos)
- **Centro de software** (modelo COSMIC Store, NO tienda independiente)
  - Vitrina gráfica con cara Golden Hour para encontrar, instalar,
    actualizar y eliminar apps.
  - Por debajo usa lo que ya existe: libflatpak para Flathub +
    packagekit para los paquetes del sistema. Luma NO aloja nada,
    NO modera, NO tiene infra propia.
  - Referencia directa: cosmic-store es Rust y open source, se puede
    estudiar cómo lo hizo System76.
  - App sustancial, baja prioridad, no traba el v1.0 (al principio el
    usuario usa cualquier centro de software, igual que el terminal
    hoy).
- **Centro de configuración** (ya listado en "mediano plazo")

### Infra / técnico

- **Backend DRM** (correr fuera de otro compositor)
  - Desbloquea: esquinas redondeadas reales, multi-window nativo.
  - TAMBIÉN desbloquea: VRR y multi-monitor con refresh mixto
    (Tier 2 de Gaming) — y esos son arroz y frijoles para el gamer.
  - O sea, el DRM vale más prioridad de la que parece: no es solo
    "esquinas redondeadas", destranca la mitad de lo que la comunidad
    gamer exige. Subirlo en la fila del mediano plazo.

- **HiDPI / escalado** — fractional scaling SIN borronear la pantalla.
  PRIORIDAD: sirve al gamer (monitor de alta resolución) Y al lado
  PCD (UI legible en alta densidad) al mismo tiempo. Mismo trabajo,
  dos públicos. No puede shipear roto.

- **Generación de tema GTK/Qt** (apps de terceros adoptan Golden Hour)

### Gaming (fase única — "salir del camino del juego")

> Soporte de juegos. Entra en una sola fase, no es un monstruo. En el
> fondo es el compositor aprendiendo a salirse del camino. Smithay ya
> entrega los bloques para la mayoría (direct scanout, VRR en el
> backend DRM, tearing) — es más conectar cables que inventar de cero.
>
> **Posición**: después de la taskbar y el núcleo del DE firme. NO
> necesita estar en el v1.0 inicial. Pero Tier 1 se puede meter
> temprano, porque es poco código y la ganancia es grande.
>
> **Prioridad** — lo que la gente pide a los gritos NO es por lo que
> te castiga si falta. HDR es el pedido más ruidoso, pero es el más
> difícil y nadie destruye un DE nuevo por no tener HDR en v1. En
> cambio VRR, fractional scaling y multi-monitor son arroz y frijoles:
> si faltan o vienen rotos, el gamer prueba 10 min y se va. Esos NO
> pueden shipear rotos. HDR queda como la estrella del v2, titular
> para el surprise drop.

**Tier 1 — esencial** (sin esto Luma es malo para juegos):

- **Direct scanout en fullscreen** — el buffer del juego va directo
  al hardware, sin que Luma copie nada en el medio.
- **Idle inhibit** — protocolo Wayland que evita que la pantalla se
  apague mientras el juego corre (ej: jugar RedM con joystick sin
  tocar el teclado). Bug clásico, barato de resolver.
- **Apagar nuestros propios efectos** — detectó juego fullscreen,
  corta animación, blur y sombra. Deja de gastar GPU al pedo.

**Tier 2 — importante, puede venir después:**

- **VRR (adaptive sync)** — toggle por monitor para FreeSync/G-Sync.
  Mata el stutter. Depende del backend DRM.
- **Multi-monitor con refresh mixto** — un 144Hz y un 60Hz juntos
  sin que el sistema clave todo en el más lento. Depende del
  backend DRM.
- **Tearing control** — protocolo `wp_tearing_control`, el juego pide
  presentación asíncrona para latencia mínima (juego competitivo).

**Tier 3 — lujo, mucho después:**

- **Status de GameMode en la barra** (adorno, opcional).
- **HDR** — demasiado complejo y nuevo para v1. Se convierte en la
  ESTRELLA del v2: es el pedido más ruidoso de la comunidad y la
  queja n.º 1 de quien llega a Linux. Guardado a propósito para dar
  titular.

### Camino al v1.0

**Pantallas del sistema** (el "chrome" alrededor de la sesión):

> Como KDE, Luma trata estas como componentes temáticos, todas
> configurables en un solo panel de la Pantalla de configuración
> nativa. NO se construye display manager — se tematiza SDDM.

- **Archivo de sesión** (`.desktop`) — hace que Luma aparezca en la
  lista de SDDM / GDM / cualquier display manager. Trivial, pero SIN
  esto nadie logea en Luma. OBLIGATORIO para v1.0.
- **Tema de SDDM (Golden Hour)** — la pantalla de login. Un tema QML
  para SDDM, exactamente como hace KDE. DECISIÓN: nada de greeter
  propio escrito de cero; se tematiza SDDM, que ya existe. Polish.
- **Splash screen** — la pantalla rápida después del login mientras
  el DE carga (el KSplash de KDE). Luma-rendered, liviano, momento de
  marca. Polish, baja prioridad.
- **Tema de boot (Plymouth) Golden Hour** — solo gráfico + config
  chico, no se construye nada. Polish barato, alto retorno.

**Cierre:**

- Polish general, QA completo
- Los 4 presets visuales: Classic, Mint, Dock, PCD Max
- Documentación final
- Surprise drop: Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 Deudas técnicas (`docs/DEBT.md`)

- Esquinas redondeadas de ventana (espera backend DRM)
- Notificaciones: apilar / centro / acciones / sonido
- Menú de contexto: 1 item dim restante (resto resuelto en H56):
  - Modo de edición → espera sistema de widgets + modo de edición general
- Menú de la taskbar (espera sistema de widgets)
- Botón "bloquear" del menú de sesión → espera la pantalla de
  bloqueo (`ext-session-lock`). Mientras no exista, el botón no
  tiene qué hacer.

---

## 💭 Ideas futuras (post-base, anotadas para no olvidar)

> Funciones inspiradas en mac/Windows que entran en un DE. NO son
> prioridad — entran solo después de la base y las apps nativas.

- **Búsqueda global** (estilo Spotlight) — buscar apps, archivos y
  configuraciones en un solo lugar. Expande la búsqueda del menú que
  ya existe.
- **Snap de ventanas** — arrastrar al borde encaja la ventana en
  media pantalla. Función de productividad.
- **Modo oscuro/claro automático por horario** — el tema acompaña la
  puesta del sol. Combina con la identidad Golden Hour, sería un
  diferencial temático de Luma.
- **Atajos de teclado personalizables** por el usuario.
- **Preview de archivos** — vista previa al seleccionar un archivo
  en el escritorio.
- **Cambiador de ventanas con miniatura** — el Alt+Tab mostrando
  preview de cada ventana en vez de solo icono.
- **Vista general de ventanas** — estilo Exposé/Mission Control de
  Mac, esparce todas las ventanas en la pantalla para elegir.
- **Historial de portapapeles** — el Win+V, guarda los últimos items
  copiados.
- **Esquinas activas** — tirar el mouse a una esquina dispara una
  acción (mostrar escritorio, abrir menú, etc).
- **Night light** — la pantalla se calienta de color de noche.
  Combina con la identidad Golden Hour, igual que el modo oscuro
  automático.

### Accesibilidad (alimentan el preset PCD Max)

> Estas NO son "idea futura" de verdad — son lo que da peso al ángulo
> "DE diseñado por persona con discapacidad" en el surprise drop.
> Merecen subir en la fila y entrar antes del v1.0.

- **Lupa de pantalla** — ampliar un pedazo de la pantalla.
- **Cursor grande y con realce** — encontrar el puntero fácil.
- **Teclas adhesivas** — apretar atajo una tecla a la vez.

---

## 🚫 Fuera de alcance (no perseguir)

> Funciones de mega-corporación — escapan de un DE de portafolio.

- Historial de versiones de archivo (estilo Time Machine)
- Continuidad entre dispositivos
- Asistente de voz

---

## 🎯 Principios del proyecto

- **Una etapa a la vez**, terminada y testeada antes de la siguiente
- Feature que depende de un sistema no listo = item dim + nota en DEBT
- **Cero hex hardcoded** — todo en `theme.toml`
- **Commits atómicos**, cada uno compila y corre
- **Backup tag** antes de cada etapa
- **Accesibilidad PCD**: hit targets ≥ 40px, WCAG AA
