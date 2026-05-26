# Documentação técnica — Luma

Este documento descreve a arquitetura do Luma, como compilar o projeto
e como contribuir.

---

## Visão geral da arquitetura

Luma é modular por design. Em vez de um único binário gigante, o
ambiente é dividido em partes independentes que conversam entre si:

- **Compositor** — o núcleo Wayland, responsável por desenhar janelas,
  processar entrada e gerenciar a tela
- **luma-bar** — a taskbar
- **luma-menu** — o menu de aplicativos
- **luma-applets** — os componentes da bandeja do sistema

Essa separação permite evoluir cada parte sem quebrar as outras, e
abre caminho para que componentes possam ser substituídos no futuro.

---

## Stack técnica

- **Linguagem:** Rust (edição 2024, MSRV 1.85)
- **Protocolo gráfico:** Wayland
- **Framework de compositor:** Smithay
- **Renderização de SVG:** usvg, resvg e tiny-skia
- **Fontes:** fontdue (rasterização BGRA premultiplied)
- **Busca:** fuzzy-matcher
- **Imagens:** image (PNG e JPEG)
- **Configuração:** toml + serde
- **D-Bus:** zbus 5 (blocking, com thread dedicada por serviço)
- **Áudio de notificação:** libcanberra carregada em runtime via
  libloading (dlopen, não exige headers de desenvolvimento no build)
- **Cliente PipeWire/PulseAudio:** spawn de `wpctl` (controle) e
  `paplay` (playback simples) — sem dependência da crate `pipewire`
- **Outras:** chrono (datas/horário), meval (calculadora),
  tracing + tracing-subscriber (logs estruturados), walkdir

O Smithay é usado como um fork local (em `vendor/smithay`), com um
patch próprio para suporte a múltiplas janelas no backend winit.

### Arquitetura do sistema de notificações

O sistema de notificações `org.freedesktop.Notifications` é um
servidor D-Bus rodando numa thread dedicada (zbus blocking). O estado
do stack de toasts vive em `Arc<Mutex<NotificationsState>>`,
compartilhado entre a thread D-Bus (que recebe os `Notify`) e o main
loop (que renderiza e processa input).

Sinais `ActionInvoked` e `NotificationClosed` são emitidos a partir
do main loop via uma `Arc<Mutex<Option<zbus::blocking::Connection>>>`
populada pela thread server depois do `RequestName`.

A camada de som é separada (`src/sound.rs`) — carrega `libcanberra.so.0`
em runtime, mantém um único contexto `ca_context` por toda a sessão,
e tem debounce interno para impedir rajadas de virarem strobes
audíveis. Se a `.so` não estiver instalada, a engine cai em silêncio
sem quebrar o build nem crashar em runtime.

A piscada de tela (visual bell) compartilha com o som o mesmo gate
de alerta (`alert_pending` por entrada) — som e piscada são dois
consumidores independentes da decisão "deve alertar". Os limites de
opacidade e duração são duros por questão de segurança fotossensível
(WCAG 2.3.1 — máximo de 3 flashes por segundo).

---

## Princípios de design de código

### Zero cores fixas no código

Nenhum valor hexadecimal de cor ou medida visual é escrito direto no
Rust. Tudo vem de um arquivo de tema, o `theme.toml`. Isso significa
que a aparência do Luma pode mudar inteira sem recompilar nada.

### Acessibilidade como regra

Alvos de clique seguem um mínimo de 40 pixels (48 ou mais no preset
PCD Max). Toda funcionalidade precisa ser alcançável por teclado.
Contraste segue as diretrizes WCAG AA, com AAA quando dá sem custo
(o scrim opaco do painel de notificações, por exemplo, garante AAA
mesmo sobre wallpaper claro). Isso não é negociável em nenhuma parte
da interface.

### Commits pequenos e funcionais

Cada commit do projeto compila e roda. O trabalho é dividido em etapas
nomeadas (por exemplo, "etapa H33.taskbar.4"), e cada etapa é um passo
verificável.

---

## Como compilar

### Dependências do sistema

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

Ferramentas runtime do system tray (rede, bluetooth, áudio) e som
de notificação:

```
sudo apt install network-manager bluez pipewire wireplumber libcanberra0
```

A `libcanberra0` é opcional — se ausente, o som de notificação cai
em silêncio sem quebrar nada.

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

É preciso também ter o Rust instalado, via rustup.

### Compilar

```
git clone <repositório>
cd luma
cargo build
```

O fork do Smithay já vem incluído no repositório, então o build
funciona sem passos extras. Para rodar:

```
cargo run
```

---

## Estrutura de pastas

```
src/                código-fonte em Rust
  backends/         backends de renderização e entrada
  menu/             o menu de aplicativos
  handlers/         handlers de protocolo Wayland
  input/            teclado, ponteiro e atalhos
  notifications.rs  estado do stack de notificações + servidor D-Bus
  sound.rs          carregamento de libcanberra + playback
  state.rs          LumaState (estado global do compositor)
  theme.rs          tokens visuais + leitura do luma.toml
assets/             ícones, sons, identidade visual, capturas de tela
vendor/             o fork local do Smithay
docs/               esta documentação
```

---

## Sistema de temas

Toda a aparência é controlada por `~/.config/luma/theme.toml`. Esse
arquivo define cores, tamanhos, espaçamentos e comportamentos de cada
componente: taskbar, menu, decoração de janelas e applets.

A paleta padrão é a Golden Hour:

- Café — os tons escuros de fundo
- Coral — destaques e ações importantes
- Mel — elementos secundários e brilhos
- Sálvia — confirmações e estados positivos

---

## Como contribuir

Por enquanto o repositório é privado e o desenvolvimento é conduzido
por uma pessoa só. Após a versão 1.0, o projeto será aberto e este
documento será expandido com um guia de contribuição completo.

Se você está lendo isto antes disso, provavelmente faz parte de um
círculo próximo. Sugestões e revisões são bem-vindas.
