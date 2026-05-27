# 🗺️ Luma — Roadmap

Desktop Environment em Rust + Smithay (Wayland)
Identidade: Golden Hour (coffee / coral / honey / sage)
Meta: v1.0 como portfólio · timeline ~2 anos · surprise drop
ESCOPO: Luma = o Desktop Environment, roda em qualquer distro.
Lumina OS (distro própria, com instalador de SO próprio) é um
projeto SEPARADO, com roadmap próprio. NÃO entra neste arquivo.

---

## ✅ JÁ FEITO

### Base / Compositor

- Compositor Smithay funcionando (Wayland)
- Multi-monitor, multi-workspace
- Roda apps Wayland de verdade
- Renderização via GPU
- Rust 1.95 + edição 2024
- QA técnico feito — 0 bugs críticos

### Interface

- Menu fullscreen híbrido (favoritos, busca, sessão)
- Taskbar estilo KDE (6 zonas)
- Decoração de janela (titlebar, botões Golden Hour)
- Resolvedor de ícones (GTK + KDE, 7557 ícones)

### Applets / OSD

- Applet de volume
- OSD de volume + brilho
- OSD de mídia (now playing)
- System tray expansível + SNI (D-Bus)
- H54 — Sistema de notificações fase 1 (toast estilo macOS)
- H57 — Applets de wifi, bluetooth e bateria (popups)
  · esconde o wifi quando não há placa (hardware ausente tratado)
  · rede cabeada no popup, hover unificado em todos os applets,
    scan via D-Bus direto

### Desktop

- Menu de contexto do desktop
- Wallpapers padrão Golden Hour + galeria
- Ícones de arquivo no desktop (ler, desenhar, grid)
- Seleção, drag, rubber-band, setas
- Menu per-icon (abrir, copiar, renomear, lixeira)
- Renomear inline (sem KDialog)
- H55 — Calendário ao clicar no relógio
- H56 — Acender 3 itens dim do menu de contexto
  (Criar novo, Colar, Organizar ícones — o 4o, Modo de edição,
  segue dim até existir o sistema de widgets)
- **H58 — Tela de bloqueio (lock screen)**
  · protocolo Wayland ext-session-lock · tela Golden Hour com
    relógio gigante, data e campo de senha · fecha o botão
    "bloquear" do menu
  · cards (Calendário, Email, etc) estritamente passivos: só
    leitura, não clicam, não vazam conteúdo sensível
  · é a base visual que o tema do SDDM e o splash vão reusar

### Em andamento

(nada no momento — próxima etapa sai de "Médio prazo")

---

## 🔜 PRÓXIMAS ETAPAS (caminho pro v1.0)

### Curto prazo

(vazio — itens promovidos pra "Em andamento" ou já feitos.
Próxima leva sai de "Médio prazo")

### Médio prazo

- **Notificações fase 2:** empilhar, agrupar por app, central/histórico
- **Sistema de widgets configuráveis** (estilo KDE Plasma)
  → widgets que o usuário adiciona/remove/configura no painel e
    no desktop (relógio, clima, notas, monitor de sistema, etc)
  → DECISÃO: widgets = Luma-native em Rust, renderizados pela
    própria luma-bar. NÃO são GTK nem Qt. Igual os plasmoids do
    KDE, que não são apps Qt encaixáveis. Só renderizando a gente
    mesmo é que se garante o Golden Hour + hit target ≥ 40px.
    GTK/Qt no Luma entra por outro caminho: apps de terceiro como
    janela (Wayland), bandeja via SNI, e a geração de tema GTK/Qt.
  → é uma FUNDAÇÃO arquitetural grande — uma das maiores etapas
    do roadmap. Fazer só quando a base do DE estiver madura.
  → desbloqueia: menu da taskbar + modo de edição geral
- **Modo de edição geral do Luma** (estilo KDE, unificado)
  → um modo onde o usuário organiza tudo num lugar só: reposiciona
    ícones do desktop E mexe nos widgets/layout dos painéis
  → depende dos ícones no desktop (✅ já tem) + sistema de widgets
    (❌ ainda não) — por isso vem junto/depois dos widgets
  → é o que acende o item dim "Modo de edição" do menu de contexto
  → junta 3 itens do roadmap num só: item dim do menu + sistema de
    widgets + este modo de edição geral

  **Como funciona:**
  · entra pelo item "Modo de edição" do menu de contexto + atalho
    de teclado; sai com botão "Pronto" ou Esc (tudo trava de novo)
  · tela ganha overlay sutil, painéis/widgets ficam com borda de
    realce, barra no topo com "Adicionar widget" e "Pronto"
  · dá pra: reposicionar ícones do desktop · adicionar/mover/
    remover/configurar widgets (desktop e painéis) · mexer nos
    painéis (altura, alinhamento, fixar/flutuar, criar novo) ·
    trocar wallpaper e preset

  **Alça do widget — diferencial PCD:**
  · cada widget focado mostra uma alça com botões (configurar,
    mover, redimensionar, remover)
  · no KDE essas alças são minúsculas e irritantes — no Luma,
    hit target ≥ 40px, botões grandes, cor Golden Hour
  · cortar o "rotacionar" do KDE (ninguém usa)
  · modo de edição usável por quem tem tremor / baixa visão = a
    história do PCD Max na prática

  **Fatias (cada uma compila e roda sozinha):**
  1. modo só reposiciona ícones do desktop (base já existe)
  2. adicionar/mover/remover/configurar widgets
  3. edição de painel (altura, alinhamento, criar painel)

- **Menu de contexto da taskbar** (depende dos widgets)
- **Tela de configurações nativa do Luma** (inclui as páginas de tema
  das Telas do sistema: splash, login/SDDM, boot — um painel só,
  igual o de Cores e Temas do KDE)
- **Configuração de tela** (resolução, monitores)
- **Slider de brilho dentro do applet de bateria** (não é applet
  próprio — vai junto da bateria, igual o KDE faz. Backend já
  existe, o OSD de brilho prova isso; falta só a casca clicável.
  Aparece só quando há backlight, mesma lógica da bateria)

### Apps nativos do Luma

Programas próprios do Luma, com identidade Golden Hour.
Cada um é praticamente um projeto dentro do projeto — fazer
um de cada vez, e só depois da base do DE estar madura.

1. **Gerenciador de arquivos** (o mais importante — abre as pastas
   do desktop, navega, copia/move)
2. **Ferramenta de captura de tela** (recorte de tela, janela ou
   área livre, com anotação. Estilo Cmd+Shift+5 do Mac / Ferramenta
   de Captura do Windows. PRIORIDADE ALTA: todo DE sério tem isso,
   fazer logo depois do gerenciador de arquivos)
3. **Editor de texto**
4. **Visualizador de imagem**
5. **Calculadora**
6. **Terminal próprio** (opcional — hoje usa o do sistema)
7. **Gerenciador de tarefas** (monitor de sistema completo: lista de
   processos, matar app travado, abas de CPU / memória / disco /
   rede. Compartilha a leitura do /proc com o widget "monitor de
   sistema" das ideias futuras — widget é a olhada rápida no
   painel, o app é a coisa completa. Baixa prioridade, fim da fila
   dos apps nativos, depois do gerenciador de arquivos)
8. **Central de software** (modelo COSMIC Store, NÃO loja independente)
   → vitrine gráfica com cara Golden Hour pra achar, instalar,
     atualizar e remover apps
   → por baixo usa o que já existe: libflatpak pro Flathub +
     packagekit pros pacotes do sistema. Luma NÃO hospeda nada,
     NÃO faz moderação, NÃO tem infra própria
   → referência direta: o cosmic-store é Rust e código aberto,
     dá pra estudar como a System76 fez
   → app substancial, baixa prioridade, não trava o v1.0 (no
     começo o usuário usa qualquer central de software, igual o
     terminal hoje)
9. **Centro de configurações** (já listado em "médio prazo")

### Infra / técnico

- **Backend DRM** (rodar fora de outro compositor)
  → desbloqueia: cantos arredondados reais, multi-window nativo
  → desbloqueia TAMBÉM: VRR e multi-monitor com refresh misto
    (Tier 2 do Gaming) — e esses são feijão com arroz pro gamer.
    Ou seja, o DRM vale mais prioridade do que parece: ele não é
    só "cantos arredondados", ele destranca metade do que a
    comunidade gamer exige. Subir na fila do médio prazo.
- **HiDPI / escalonamento** — fractional scaling SEM borrar a tela.
  PRIORIDADE: serve gamer (monitor de alta resolução) E o lado
  PCD (UI legível em alta densidade) ao mesmo tempo. Mesmo
  trabalho, dois públicos. Não pode shipar quebrado.
- **Geração de tema GTK/Qt** (apps de terceiros adotam Golden Hour)

### Gaming (fase única — "sair da frente do jogo")

Suporte a jogos. Cabe numa fase só, não é monstro. No fundo é
o compositor aprender a sair da frente. O Smithay já entrega
os blocos pra maior parte (direct scanout, VRR no backend DRM,
tearing) — é mais ligar os fios do que inventar do zero.
Posição: depois da taskbar e do núcleo do DE firme. NÃO precisa
estar no v1.0 inicial. Mas o Tier 1 dá pra encaixar cedo,
porque é pouco código e o ganho é grande.

PRIORIDADE — o que o povo pede alto NÃO é o que ele te castiga
por não ter. HDR é o pedido mais barulhento, mas é o mais
difícil e ninguém esculacha um DE novo por não ter HDR no v1.
Já VRR, fractional scaling e multi-monitor são feijão com
arroz: se faltar ou vier quebrado, o gamer testa 10 min e vai
embora. Esses NÃO podem shipar quebrados. HDR fica como a
estrela do v2, manchete pro surprise drop.

**Tier 1 — essencial (sem isso o Luma é ruim pra jogo):**
- Direct scanout em fullscreen — buffer do jogo vai direto pro
  hardware, sem o Luma copiar nada no meio
- Idle inhibit — protocolo Wayland que impede a tela de apagar
  enquanto o jogo roda (ex: jogar RedM no controle sem encostar
  no teclado). Bug clássico, barato de resolver
- Desligar os próprios efeitos — detectou jogo fullscreen, corta
  animação, blur e sombra. Para de gastar GPU à toa

**Tier 2 — importante, pode vir depois:**
- VRR (adaptive sync) — toggle por monitor pra FreeSync/G-Sync.
  Mata o stutter. Depende do backend DRM
- Multi-monitor com refresh misto — um 144hz e um 60hz juntos
  sem o sistema travar tudo no menor. Depende do backend DRM
- Tearing control — protocolo wp_tearing_control, jogo pede
  apresentação assíncrona pra latência mínima (jogo competitivo)

**Tier 3 — luxo, bem mais tarde:**
- Status do GameMode na barra (enfeite, opcional)
- HDR — complexo e novo demais pro v1. Vira a ESTRELA do v2:
  é o pedido mais barulhento da comunidade e a reclamação nº 1
  de quem chega no Linux. Guardado de propósito pra dar manchete

### Rumo ao v1.0

**Telas do sistema (o "chrome" em volta da sessão):**
Igual o KDE, o Luma trata estas como componentes temáveis,
todas configuráveis num painel só da Tela de configurações
nativa. NÃO se constrói display manager — tema-se o SDDM.

- **Arquivo de sessão (.desktop)** — faz o Luma aparecer na lista do
  SDDM / GDM / qualquer display manager. Trivial, mas SEM isso
  ninguém loga no Luma. OBRIGATÓRIO pro v1.0.
- **Tema do SDDM (Golden Hour)** — a tela de login. Um tema QML pro
  SDDM, exatamente como o KDE faz. DECISÃO: nada de greeter
  próprio escrito do zero; tema-se o SDDM, que já existe. Polish.
- **Splash screen** — a tela rápida depois do login enquanto o DE
  carrega (o KSplash do KDE). Luma-rendered, leve, momento de
  marca. Polish, baixa prioridade.
- **Tema de boot (Plymouth) Golden Hour** — só gráfico + config
  pequeno, não constrói nada. Polish barato, alto retorno.
- **Empacotamento e instalação do Luma** — é como o DE chega na
  máquina das pessoas. Cobre: instruções de build do código +
  pacotes de distro (AUR pro Arch, COPR pro Fedora, .deb pro
  Ubuntu). OBRIGATÓRIO pro surprise drop — sem isso o pessoal do
  r/linux, Phoronix e HN abre o repo e não consegue testar.
  Pareia com o arquivo de sessão. (Luma é DE: NÃO vira Flatpak,
  Flatpak é pra app, DE instala no sistema.)
- **Polish geral, QA completo**
- **Os 4 presets visuais:** Classic, Mint, Dock, PCD Max
- **Documentação final**
- **Surprise drop:** Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 DÍVIDAS TÉCNICAS ANOTADAS (docs/DEBT.md)

- Cantos arredondados de janela (espera backend DRM)
- Notificações: empilhar / central / ações / som
- Menu de contexto: 1 item dim restante (resolvido o resto no H56):
  · Modo de edição → espera sistema de widgets + modo de edição geral
- Menu da taskbar (espera sistema de widgets)
- Botão "bloquear" do menu de sessão → espera a tela de bloqueio
  (ext-session-lock). Enquanto ela não existir, o botão não tem
  o que fazer.

---

## 💭 IDEIAS FUTURAS (pós-base, anotadas pra não esquecer)

Recursos inspirados em mac/Windows que cabem num DE. NÃO são
prioridade — entram só depois da base e dos apps nativos.

- **Busca global (estilo Spotlight)** — buscar apps, arquivos e
  configurações num lugar só. Expande a busca do menu que já existe.
- **Snap de janelas** — arrastar pra borda encaixa a janela em meia
  tela. Recurso de produtividade.
- **Modo escuro/claro automático por horário** — o tema acompanha o
  pôr do sol. Combina com a identidade Golden Hour, seria um
  diferencial temático do Luma.
- **Atalhos de teclado customizáveis pelo usuário.**
- **Preview de arquivos** — prévia ao selecionar um arquivo no desktop.
- **Trocador de janelas com miniatura** — o Alt+Tab mostrando preview
  de cada janela em vez de só ícone.
- **Visão geral de janelas** — estilo Exposé/Mission Control do Mac,
  espalha todas as janelas na tela pra escolher.
- **Histórico de área de transferência** — o Win+V, guarda os últimos
  itens copiados.
- **Cantos ativos** — jogar o mouse num canto dispara uma ação
  (mostrar desktop, abrir menu, etc).
- **Night light** — a tela esquenta de cor à noite. Combina com a
  identidade Golden Hour, igual o modo escuro automático.

### Acessibilidade (alimentam o preset PCD Max)

Estas NÃO são "ideia futura" de verdade — são o que dá peso ao
ângulo "DE desenhado por pessoa com deficiência" no surprise
drop. Merecem subir na fila e entrar antes do v1.0.

- **Lupa de tela** — ampliar um pedaço da tela.
- **Cursor grande e com realce** — achar o ponteiro fácil.
- **Teclas de aderência** — apertar atalho uma tecla por vez.

---

## 🚫 FORA DO ESCOPO (não perseguir)

Features de empresa gigante — fogem de um DE de portfólio.

- Histórico de versões de arquivo (tipo Time Machine)
- Continuidade entre dispositivos
- Assistente de voz

---

## 🎯 PRINCÍPIOS DO PROJETO

- Uma etapa de cada vez, terminada e testada antes da próxima
- Feature que depende de sistema não-pronto = item dim + nota no DEBT
- Zero hex hardcoded — tudo no theme.toml
- Commits atômicos, cada um compila e roda
- Backup tag antes de cada etapa
- Acessibilidade PCD: hit targets ≥ 40px, WCAG AA
