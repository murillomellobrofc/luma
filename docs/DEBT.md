# Débito técnico — Luma

Lista de coisas que foram adiadas conscientemente, com motivo. Cada
item tem o porquê adiou + a condição pra desbloquear.

---

## Cantos arredondados de janela

**Status:** adiado.

**Decisão:** `[window].corner_radius = 0` (cantos retos limpos).

### Por quê adiou

O arredondamento real exige máscara alpha aplicada na textura do
conteúdo do app — o conteúdo é desenhado pelo cliente como retângulo
e cobre qualquer "recorte" tentado pela decoração.

Smithay/GLES no backend winit atual não tem shader de máscara nativa
acessível sem reescrever parte do renderer. Tentativas com máscara
de cor sólida nos 4 cantos (`make_corner_mask` em `src/gfx.rs`,
removida em `H_deco.round.revert`) deixavam o canto com aparência
de remendo em apps de fundo escuro: a cor da mask coincidia com o
bg do cliente (terminais, editores) e ficava invisível, ou
contrastava demais (coffee médio) e parecia gambiarra.

### Quando desbloqueia

Quando o renderer for reescrito pra suportar alpha mask (provável
na fase do backend DRM nativo, hoje a expectativa). Aí o token
`[window].corner_radius` volta a valer e os 4 cantos da janela
ficam arredondados pela mask de alpha sobre a textura do cliente.

### Referências

- Commits da tentativa: `a688230`, `4931194`, `8b7061a`, `d866a42`
  (todos antes do revert).
- Decisão final: commit `H_deco.round.revert` (cantos retos +
  débito documentado).
- Sombra 9-slice ainda é retangular (`src/window_shadow.rs`); volta
  a ter sentido revisitar quando os cantos voltarem a ser round.

---

## OSD duplicado em modo aninhado (H45.osd)

**Status:** débito aceito.

**Sintoma:** quando o luma roda dentro de um compositor host (KDE
em modo winit, hoje), ao pressionar XF86_AudioRaiseVolume etc. o
host **também** captura a tecla e mostra o OSD dele. O usuário
vê 2 OSDs em paralelo (luma centralizado + KDE no canto).

### Por quê adiou

Em modo nested (winit/wayland-1), o servidor host recebe os eventos
de input antes do luma. Não tem como interceptar XF86 antes do
host ver. A única correção é rodar em DRM nativo, onde o luma é o
único compositor e processa input direto do libinput.

### Quando desbloqueia

Migração pro backend DRM nativo (planejada). Daí o luma processa
XF86 sem intermediário e o OSD dele é o único.

### Mitigação interina

Em DEV, ignora o OSD do host. O caminho de input + render do luma
já está correto; só o host duplica.

---

## Brilho sem backend ativo (H45.osd)

**Status:** funcional como no-op.

**Sintoma:** na máquina de dev (desktop sem painel interno, sem
`/sys/class/backlight`, sem `brightnessctl`/`ddcutil` instalados),
XF86_MonBrightnessUp/Down não fazem nada — nem o OSD do luma
aparece pra brilho. `brightness::available()` retorna `false` e
`apply()` ignora a ação.

### Por quê é OK

Não inventa OSD sem ter dado pra mostrar. O caminho de input e o
component compartilhado de render já estão prontos: o dia que o
Murillo instalar `ddcutil` (DDC/CI pro monitor externo) ou
`brightnessctl` (laptop futuro), o backend resolve sozinho e o OSD
ligado começa a aparecer sem mudança de código.

### Como ligar

```bash
# Opção 1: monitor externo via DDC/CI
sudo apt install ddcutil
sudo modprobe i2c-dev
sudo usermod -a -G i2c $USER

# Opção 2: laptop com backlight interno
sudo apt install brightnessctl
sudo usermod -a -G video $USER
```

Após reiniciar a sessão, `brightness::probe()` detecta o backend
e o OSD de brilho passa a funcionar.

---

## OSD de mídia sem `playerctl` (H46.osd-media)

**Status:** funcional como no-op.

**Sintoma:** XF86_AudioPlay / Next / Prev / Stop não fazem nada
e o card de "now playing" não aparece quando `playerctl` não está
instalado. Mesmo padrão da degradação do brilho.

### Por quê é OK

Mesmo argumento de [[Brilho sem backend ativo]]: caminho de input
+ render já está pronto, só falta o binário userland. Instalar
desbloqueia sem mudança de código.

### Como ligar

```bash
sudo apt install playerctl
```

Sem grupo extra, sem udev rule — `playerctl` fala D-Bus com o
usuário da sessão.

---

## Arte de álbum via HTTPS (H46.osd-media)

**Status:** débito aceito.

**Sintoma:** quando o player MPRIS expõe `mpris:artUrl` como
`https://...` (alguns web players), o luma renderiza o
placeholder de nota musical em vez de baixar a imagem.

### Por quê adiou

Adicionar HTTP client (reqwest, ureq) puxa ~30-60 deps + TLS
(rustls/native-tls). Pesa demais pra um nice-to-have.

A maioria dos players relevantes (Spotify desktop, Brave Music,
Strawberry, mpv, audacious, rhythmbox) escreve arte em cache
local e expõe `file://` — o caminho rápido já cobre.

### Quando desbloqueia

Quando algum player que o Murillo usa regularmente expor só HTTP
e a falta da arte virar incômodo real. Aí pesa instalar `ureq`
(menor dos clientes HTTP, ~15 deps sem TLS).

---

## OSD de mídia duplicado em modo aninhado (H46.osd-media)

**Status:** débito aceito (mesma natureza do volume/brilho).

**Sintoma:** em modo winit, o KDE host também captura XF86 media
keys e mostra o OSD dele. 2 OSDs em paralelo. Some no DRM nativo.

Ver [[OSD duplicado em modo aninhado]] (H45) — mesma causa.

---

## Tray overflow sem itens SNI (H47a → H47b)

**Status:** infra visual pronta, falta backend D-Bus.

**Sintoma:** a setinha de overflow do tray aparece **dim** (alpha
`arrow_disabled_alpha`) e o painel abre com placeholder "Sem itens".
Nenhum ícone de app de terceiros (Discord, Steam, etc) é exibido.

### Por quê adiou

Suporte real exige protocolo SNI (StatusNotifierItem) via D-Bus:
- Adicionar crate `zbus` (~80 deps transitivos, ~30-60s compile)
- Thread dedicada com `zbus::blocking::Connection`
- Registrar luma como `org.kde.StatusNotifierHost-<pid>`
- Escutar signals do `org.kde.StatusNotifierWatcher`
- Pra cada item: ler `IconName/IconPixmap/ToolTip`, escutar `NewIcon`
  etc.

Etapa H47a entregou só a parte visual (botão + painel + estados
abertos/fechados/dim/placeholder). Esses ganchos ficam prontos pro
H47b conectar o backend SNI.

### Quando desbloqueia

Etapa H47b — `tray.sni_count` passa a refletir itens reais; setinha
acende quando há itens; painel renderiza grid 4-col 40×40 com SVGs
dos apps; click esquerdo chama `Activate`, click direito chama
`ContextMenu` (deixa o app desenhar o menu — DBusMenu nativo fica
pra H47c).

### Workaround interino

Nenhum — o tray nativo (battery/wifi/bt/volume/bell) continua
funcionando normal. Apenas apps de terceiros não aparecem.

---

## SNI: menu nativo via DBusMenu (H47c)

**Status:** delegando ao app por enquanto.

**Decisão:** click direito num item SNI chama `ContextMenu(x, y)` —
o app desenha o próprio menu, fora do luma. Não há renderização
nativa do menu.

### Por quê adiou

DBusMenu (`com.canonical.dbusmenu`) é outro protocolo D-Bus com
árvore de itens, properties por nó (label, type, icon, separator,
shortcut), signals (LayoutUpdated, ItemsPropertiesUpdated) e
ações (Event). Pra render nativo precisamos:
- Proxy `com.canonical.dbusmenu` em cada item
- Recursão na árvore via `GetLayout(parent, depth)`
- Cache de propriedades + escuta de updates
- Layout floating menu + render + hit-test

Trabalho equivalente a uma etapa inteira (H47c). Por enquanto
ContextMenu cobre o caso de uso (o app já sabe desenhar bonito).

### Quando desbloqueia

Etapa H47c — substituir `sni::context_menu` por abrir um popup do
luma renderizando o DBusMenu do item. Mantém ContextMenu como
fallback se DBusMenu falhar.

---

## SNI: thread D-Bus separada (H47b)

**Status:** funcional, mas com nuance.

**Nuance:** a thread D-Bus tem sua própria `blocking::Connection`
em loop infinito de `MessageIterator`. Ações de click (`Activate`,
`ContextMenu`) abrem conexões novas em vez de reutilizar a do
listener — fire-and-forget, ~10ms latência por click.

### Por quê

`MessageIterator` da thread bloqueia indefinidamente; não é
trivial intercalar com comandos vindos da main thread sem mpsc +
poll com timeout (que complicaria o loop). Conexão nova por click
é simples e a latência é imperceptível.

### Quando desbloqueia

Se medirmos latência percebível em clicks (improvável), trocar
por canal mpsc + `MessageStream` com `next_timeout`.

---

## Context menu: ícones no desktop (H48 → futuro)

**Status:** 4 items do menu de contexto ficam **dim** porque dependem
de uma feature que o Luma ainda não tem: arquivos visíveis como ícones
no desktop (como o KDE/GNOME fazem com Desktop folder).

**Items afetados:**
- Criar novo (submenu: Pasta, Arquivo de texto)
- Colar
- Organizar ícones (submenu: por nome, tipo, data)
- Modo de edição

### Por quê adiou

"Ícones no desktop" pede:
- Watcher do `~/Desktop` (inotify ou polling)
- Render de ícone + label por arquivo, layout em grid
- Drag-and-drop entre ícones
- Hit-test de click direito em ícone vs. área vazia (menus diferentes)
- Clipboard interno (pra "Colar")

Etapa grande, separada. Hoje os 4 ficam visualmente presentes mas não
clicáveis — preserva o layout final do menu pra quando ligar.

### Quando desbloqueia

Etapa futura "H49 (ou similar) — Ícones de arquivo no desktop". Aí os
4 items viram `enabled = true` em `default_items()` e ganham handlers.

---

## Context menu: hot-reload do wallpaper (H48.ctx.6)

**Status:** funcional, mas exige restart.

**Sintoma:** "Mudar papel de parede" copia o arquivo pra
`~/.config/luma/wallpaper.<ext>` corretamente, mas o luma só lê esse
arquivo no boot (em `Wallpaper::load`). Pra ver o novo wallpaper, o
user precisa reiniciar o luma.

### Por quê adiou

O `Wallpaper` é dono de um `MemoryRenderBuffer` que está dentro de um
`Arc` compartilhado com o `Space`. Recarregar envolve:
- Decodificar nova imagem (já temos `try_load`)
- Construir novo `Wallpaper`
- Atualizar todos os pontos que referenciam o `Arc<Wallpaper>` (state +
  Space)

Operação não-trivial; ctx.6 entrega o caminho copy-and-pick que é o
**90% do valor** percebido (user escolhe imagem, luma armazena). O
último passo (reload runtime) fica pra polish.

### Quando desbloqueia

Etapa polish: adicionar `Wallpaper::reload(&mut Arc)` que re-lê do
disco. Substituir os `Arc` no state + Space. ~1 commit.

---

## Context menu: "Aqui" do terminal = $HOME (H48.ctx.5)

**Status:** semântica reduzida.

**Sintoma:** "Abrir terminal aqui" abre o terminal padrão com
`current_dir = $HOME`, não com o "aqui" real (pasta navegada no
desktop, que não existe ainda).

### Por quê

Sem ícones de arquivo no desktop (ver acima), não há conceito de
"pasta atual do desktop". `$HOME` é o fallback honesto.

### Quando desbloqueia

Junto com a feature de ícones de arquivo — daí "aqui" passa a ser a
pasta navegada/exibida no momento do right-click.

---

## Context menu: Configurar tela = dim (H48.ctx.7)

**Status:** dim definitivo (visual presente, sem ação).

**Por quê:** o Luma é o próprio compositor. Lançar `kcmshell` (KDE) ou
`gnome-control-center` configuraria o **host** (em modo aninhado) ou
não faria nada (em DRM nativo). Sem UI de config de tela própria,
ficar dim é mais honesto que abrir algo que não controla o luma.

### Quando desbloqueia

Etapa futura "Config de tela nativa do Luma" — UI dentro do luma pra
mexer em resolução, orientação, ordem de monitores. Em DRM nativo
isso vai usar as APIs do kernel KMS; em modo aninhado, fica dim ou
abre algo placeholder.

---

## Wallpapers — galeria no menu + multi-resolução (H49)

**Status:** ship-vel hoje, melhorias adiadas.

Shipados 6 wallpapers Golden Hour em `assets/wallpapers/`, todos
1920×1080. O luma seleciona um default (`luma-gradient-dusk`)
quando o user não tem `~/.config/luma/wallpaper.{png,jpg,jpeg}`.

### O que falta

- **Multi-resolução**: o spec original previa 3 resoluções por wallpaper
  (1920×1080, 2560×1440, 3840×2160). Só a base 1080 está shipada;
  resvg/image fazem upscale OK pra resoluções maiores, mas wallpapers
  nativos seriam melhores. Adicionar 1440/4K quando disponíveis e
  selecionar o mais próximo do output ativo.

- **Galeria de wallpapers no context menu**: o item "Mudar papel de
  parede" hoje chama kdialog/zenity (file picker do SO). Uma melhoria
  é listar os 6 wallpapers shipados como opções rápidas direto no
  menu (ou submenu), com thumb 64×40. O `wallpapers-preview.png` em
  `assets/wallpapers/` já existe pra UI dessa galeria.

### Quando desbloqueia

Quando houver tela de configurações nativa do luma (mesma etapa que
"Configurar tela" do ctx menu). Galeria entra como sub-painel dessa
config.

---

## Notificações — toast simples (H54)

**Status:** v1 entregue. Empilhar, central e ações ficam pra etapas
futuras.

### O que funciona

- Servidor `org.freedesktop.Notifications` env-gated
  (`LUMA_TAKE_NOTIFICATIONS=1`) — não rouba notificações do host KDE
  em modo aninhado.
- `Notify` / `CloseNotification` / `GetCapabilities` / `GetServerInformation`.
- Decode de ícone via `hints["image-data"]`, `hints["image-path"]` ou
  `app_icon` (XDG name).
- Toast no canto superior direito, slide-in + fade, auto-dismiss em
  ~5s (ou `expire_timeout` do app se >0), click fecha.

### O que falta (fases futuras)

- **Empilhar múltiplos toasts** — hoje a nova substitui a atual.
  Solução: `Vec<Notification>` em vez de `Option`, render vertical
  stack com offset progressivo.
- **Central de notificações / histórico** — clique em ícone do tray
  abre painel com últimas N.
- **Actions** — `actions: as` do `Notify`. Botões clicáveis no toast
  que disparam `ActionInvoked(id, key)`. Adicionar variant em
  `GetCapabilities` ("actions"), renderizar botões, dispatch via
  signal.
- **Markup** — `body-markup` em capabilities + parser básico de
  HTML-like (`<b>`, `<i>`, `<a href>`).
- **Sound** — `hints["sound-name"]` ou `hints["sound-file"]` →
  spawnar `aplay` / `paplay`.
- **REPLACE_EXISTING** no RequestName — hoje se outro servidor (ex:
  plasmashell) já é dono, luma falha silenciosamente. Spec permite
  REPLACE; útil em DRM nativo quando luma boot-takes ou no env-gated
  test.

### Quando desbloqueia

Quando alguma feature acima virar incômodo real. Empilhar é a mais
provável (apps que disparam várias notif em sequência). Central
faz sentido após H55+ (UI de configurações nativa).

---

## Notificações P3 — calloop one-shot wake (H54.P3)

**Status:** funcional em modo aninhado, debt em DRM nativo.

A Phase 3 do toast estilo macOS pediu um calloop one-shot timer
armado pro deadline mais próximo do stack, re-armado em
add/remove/pause/resume/replace, e que dispare `request_redraw`
quando expira.

### O que está em vigor

Implementação atual usa **elapsed-based no render loop** —
luma chama `backend.window().request_redraw()` a cada `WinitEvent::Redraw`,
mantendo a cadência contínua. Cada frame chama `notifications::tick()`
que processa expiração + promoção da queue + remoção das closing.
Em ~16ms de granularidade, o expire é detectado e o fade-out começa.

Em modo aninhado (KDE host) isso funciona perfeito — luma renderiza
continuamente.

### O que falta (DRM nativo / power-save)

Em DRM nativo no futuro, se quisermos parar de renderizar quando
ocioso (power-saving), o timer one-shot precisa ser real:
1. Patchar smithay vendored pra expor `Arc<WinitWindow>` (ou
   equivalente no path DRM).
2. Stashar essa Arc em LumaState.
3. Adicionar calloop `Timer::from_deadline` no event_loop, armado
   pelo `tick()` quando há toast com timer ativo.
4. Callback do Timer chama `window.request_redraw()` → próximo
   frame processa.
5. Re-armar quando add/remove/pause/resume/replace mudar o conjunto.

### Quando desbloqueia

Etapa de DRM nativo OU quando começar a importar economia de
energia (laptop). O código atual já funciona — só rende mais
frames do que o estritamente necessário.

### Pílula "+N" da queue (H54.P3.5)

A queue funciona (overflow do `MAX_VISIBLE_TOASTS=4`) mas não há
indicador visual. Adicionar um chip pequeno tipo "+N" no canto
inferior do último toast quando `queue.len() > 0`. Custo: layout +
1 SolidColorBuffer + 1 text rasterize. Pequeno, vale virar etapa
separada (P3.5) pra não acumular escopo na P3.
