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
