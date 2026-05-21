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
