# 🗺️ Luma — Roadmap

> Desktop Environment em Rust + Smithay (Wayland)
> Identidade: Golden Hour (coffee / coral / honey / sage)
> Meta: v1.0 como portfólio · timeline ~2 anos · surprise drop
>
> ESCOPO: Luma = o Desktop Environment, roda em qualquer distro.
> Lumina OS (distro própria, com instalador de SO próprio) é um
> projeto SEPARADO, com roadmap próprio. NÃO entra neste arquivo.

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
- H58 — Tela de bloqueio (lock screen)
  · protocolo Wayland ext-session-lock · tela Golden Hour com
    relógio gigante, data e campo de senha · fecha o botão
    "bloquear" do menu
  · cards (Calendário, Email, etc) estritamente passivos: só
    leitura, não clicam, não vazam conteúdo sensível
  · é a base visual que o tema do SDDM e o splash vão reusar
  · fatia 6 (hardening): hotkey Ctrl+Alt+L · whitelist de teclas
    XF86 (brilho, volume, mídia passam mesmo trancado) · supressão
    de surfaces de cliente e de pointer durante o lock · OSD
    renderiza por cima do lock_bg
- H58.1 — Notificações na tela de bloqueio
  · pilha discreta na capa do lock (só ícone + app + count, modo
    cheio é opt-in) · agregação por app a partir de visible+history
  · seed LUMA_FAKE_NOTIFICATIONS=N pra testar em modo aninhado
    sem derrubar o popup do KDE
- H59 — Sistema de widgets, FUNDAÇÃO (código pronto, DESATIVADO
  até existir modo de edição)
  · trait Widget (render + tick_interval + tick) no crate
    luma-widget · registro name→builder no crate luma ·
    RenderCtx host-aware (Panel { height } / Desktop) ·
    layout.toml como config ([[widget]] com type, placement,
    config)
  · POCs: LabelWidget (texto fixo) e CounterWidget (1/s)
  · loop de tick inline no redraw handler, mesma cara dos
    elapsed-checks do OSD e dos toasts
  · 2 hosts: fileira do painel + posição livre no desktop
    (z=5, atrás de janelas)
  · MIGRAÇÃO REALIZADA: monorepo Cargo workspace com 3 crates
    (luma, luma-theme, luma-widget). Aquela nota de "migrar
    depois dos widgets" virou história
  · DESATIVADO POR DECISÃO: sistema fica DESLIGADO até o modo
    de edição existir. Sem modo de edição, usuário não tem como
    adicionar/mover/remover widget — só editando layout.toml na
    mão, e isso é experiência de dev, não de usuário. Reativa
    com 1 uncomment em state.rs quando o modo de edição chegar.

### Em andamento
- 🔨 Personalização contextual (estilo Mint puro) — FASE em design
  Renomeado a partir de "Modo de edição geral". DECISÃO: vai 100%
  estilo Mint/Cinnamon, SEM modo. Configuração acontece por
  right-click direto onde tá o problema, igual o Cinnamon faz.
  Liga o sistema de widgets dormente (H59). Número de H sai por
  fatia.

  Por que Mint puro (não híbrido com KDE):
  · Mint é 1 dos 4 pilares da identidade do Luma (Windows 7 menu
    + Linux Mint customização + COSMIC tech + PCD design)
  · PCD: sem modo significa sem fricção. Right-click resolve sem
    a pessoa precisar lembrar de entrar/sair de modo
  · O "alça do widget" estilo KDE (halo no modo de edição) some.
    A intenção PCD migra: menus de right-click têm itens grandes,
    contraste forte e hit target ≥ 40px

  Como funciona (modelo Cinnamon):
  · right-click no painel → menu com adicionar widget,
    configurar painel, etc
  · right-click no desktop → adicionar widget, personalizar
    desktop, trocar wallpaper/preset
  · right-click num widget → configurar, mover, remover esse
    widget específico
  · NÃO existe "modo de edição" pra entrar/sair

  Consequências (escritas pra não esquecer):
  · o item dim "Modo de edição" do menu de contexto do desktop é
    APAGADO (não acendido) — parte das tarefas desta fase
  · o "Menu de contexto da taskbar" que estava no médio prazo é
    absorvido por esta fase (right-click no painel É a entrada
    de configuração agora)
  · a fatia antiga "modo só reposiciona ícones do desktop" some
    — no estilo Mint, o usuário só arrasta o ícone, já funciona

  Fatias propostas (chat de design pode refinar):
  1. Menus de right-click (painel, desktop, widget) + ligar o
     H59. Usuário já consegue adicionar/mover/remover widget
  2. Diálogo de personalização (gerenciar widgets instalados,
     trocar wallpaper/preset). PODE virar tab da Tela de
     configurações nativa em vez de diálogo solto — decidir no
     chat de design
  3. Configurar painel (altura, alinhamento, criar/remover painel)

  Destrava ao fechar a fase: sistema de widgets sai do dormente +
  item dim do menu é apagado + menu de contexto da taskbar passa
  a existir.

---

## 🔜 PRÓXIMAS ETAPAS (caminho pro v1.0)

### Curto prazo
- (vazio — itens promovidos pra "Em andamento" ou já feitos.
  Próxima leva sai de "Médio prazo")

### Médio prazo
- Notificações fase 2: empilhar, agrupar por app, central/histórico
- Tela de configurações nativa do Luma (inclui as páginas de tema
  das Telas do sistema: splash, login/SDDM, boot — um painel só,
  igual o de Cores e Temas do KDE)
- Configuração de tela (resolução, monitores)
- Slider de brilho dentro do applet de bateria (não é applet
  próprio — vai junto da bateria, igual o KDE faz. Backend já
  existe, o OSD de brilho prova isso; falta só a casca clicável.
  Aparece só quando há backlight, mesma lógica da bateria)

### Apps nativos do Luma
> Programas próprios do Luma, com identidade Golden Hour.
> Cada um é praticamente um projeto dentro do projeto — fazer
> um de cada vez, e só depois da base do DE estar madura.
- Gerenciador de arquivos (o mais importante — abre as pastas
  do desktop, navega, copia/move)
- Ferramenta de captura de tela (recorte de tela, janela ou
  área livre, com anotação. Estilo Cmd+Shift+5 do Mac / Ferramenta
  de Captura do Windows. PRIORIDADE ALTA: todo DE sério tem isso,
  fazer logo depois do gerenciador de arquivos)
- Editor de texto
- Visualizador de imagem
- Calculadora
- Terminal próprio (opcional — hoje usa o do sistema)
- Gerenciador de tarefas (monitor de sistema completo: lista de
  processos, matar app travado, abas de CPU / memória / disco /
  rede. Compartilha a leitura do /proc com o widget "monitor de
  sistema" das ideias futuras — widget é a olhada rápida no
  painel, o app é a coisa completa. Baixa prioridade, fim da fila
  dos apps nativos, depois do gerenciador de arquivos)
- Central de software (modelo COSMIC Store, NÃO loja independente)
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
- Centro de configurações (já listado em "médio prazo")

### Infra / técnico
- Backend DRM (rodar fora de outro compositor)
  → desbloqueia: cantos arredondados reais, multi-window nativo
  → desbloqueia TAMBÉM: VRR e multi-monitor com refresh misto
    (Tier 2 do Gaming) — e esses são feijão com arroz pro gamer.
    Ou seja, o DRM vale mais prioridade do que parece: ele não é
    só "cantos arredondados", ele destranca metade do que a
    comunidade gamer exige. Subir na fila do médio prazo.
- HiDPI / escalonamento — fractional scaling SEM borrar a tela.
  PRIORIDADE: serve gamer (monitor de alta resolução) E o lado
  PCD (UI legível em alta densidade) ao mesmo tempo. Mesmo
  trabalho, dois públicos. Não pode shipar quebrado.
- Geração de tema GTK/Qt (apps de terceiros adotam Golden Hour)
- Migração para monorepo (Cargo workspace) — ✅ CONCLUÍDA no H59.
  Hoje o Luma é monorepo com 3 crates: luma, luma-theme,
  luma-widget. Nota mantida só pra documentar a decisão e o
  momento da migração.

### Gaming (fase única — "sair da frente do jogo")
> Suporte a jogos. Cabe numa fase só, não é monstro. No fundo é
> o compositor aprender a sair da frente. O Smithay já entrega
> os blocos pra maior parte (direct scanout, VRR no backend DRM,
> tearing) — é mais ligar os fios do que inventar do zero.
> Posição: depois da taskbar e do núcleo do DE firme. NÃO precisa
> estar no v1.0 inicial. Mas o Tier 1 dá pra encaixar cedo,
> porque é pouco código e o ganho é grande.
>
> PRIORIDADE — o que o povo pede alto NÃO é o que ele te castiga
> por não ter. HDR é o pedido mais barulhento, mas é o mais
> difícil e ninguém esculacha um DE novo por não ter HDR no v1.
> Já VRR, fractional scaling e multi-monitor são feijão com
> arroz: se faltar ou vier quebrado, o gamer testa 10 min e vai
> embora. Esses NÃO podem shipar quebrados. HDR fica como a
> estrela do v2, manchete pro surprise drop.

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
> Igual o KDE, o Luma trata estas como componentes temáveis,
> todas configuráveis num painel só da Tela de configurações
> nativa. NÃO se constrói display manager — tema-se o SDDM.
- Arquivo de sessão (.desktop) — faz o Luma aparecer na lista do
  SDDM / GDM / qualquer display manager. Trivial, mas SEM isso
  ninguém loga no Luma. OBRIGATÓRIO pro v1.0.
- Tema do SDDM (Golden Hour) — a tela de login. Um tema QML pro
  SDDM, exatamente como o KDE faz. DECISÃO: nada de greeter
  próprio escrito do zero; tema-se o SDDM, que já existe. Polish.
- Splash screen — a tela rápida depois do login enquanto o DE
  carrega (o KSplash do KDE). Luma-rendered, leve, momento de
  marca. Polish, baixa prioridade.
- Tema de boot (Plymouth) Golden Hour — só gráfico + config
  pequeno, não constrói nada. Polish barato, alto retorno.

- Empacotamento e instalação do Luma — é como o DE chega na
  máquina das pessoas. Cobre: instruções de build do código +
  pacotes de distro (AUR pro Arch, COPR pro Fedora, .deb pro
  Ubuntu). OBRIGATÓRIO pro surprise drop — sem isso o pessoal do
  r/linux, Phoronix e HN abre o repo e não consegue testar.
  Pareia com o arquivo de sessão. (Luma é DE: NÃO vira Flatpak,
  Flatpak é pra app, DE instala no sistema.)
- Polish geral, QA completo
- Os 4 presets visuais: Classic, Mint, Dock, PCD Max
- Documentação final
- Surprise drop: Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 DÍVIDAS TÉCNICAS ANOTADAS (docs/DEBT.md)
- Cantos arredondados de janela (espera backend DRM)
- Menu de contexto: 1 item dim restante (resolvido o resto no H56):
  · Modo de edição → este item será APAGADO (não acendido) na
    fase de Personalização contextual, decisão Mint puro: sem
    modo, configuração por right-click direto onde tá o problema
- Menu da taskbar → absorvido pela fase de Personalização
  contextual (right-click no painel É a entrada de configuração)

---

## 💭 IDEIAS FUTURAS (pós-base, anotadas pra não esquecer)
> Recursos inspirados em mac/Windows que cabem num DE. NÃO são
> prioridade — entram só depois da base e dos apps nativos.
- Busca global (estilo Spotlight) — buscar apps, arquivos e
  configurações num lugar só. Expande a busca do menu que já existe.
- Snap de janelas — arrastar pra borda encaixa a janela em meia
  tela. Recurso de produtividade.
- Modo escuro/claro automático por horário — o tema acompanha o
  pôr do sol. Combina com a identidade Golden Hour, seria um
  diferencial temático do Luma.
- Atalhos de teclado customizáveis pelo usuário.
- Preview de arquivos — prévia ao selecionar um arquivo no desktop.
- Trocador de janelas com miniatura — o Alt+Tab mostrando preview
  de cada janela em vez de só ícone.
- Visão geral de janelas — estilo Exposé/Mission Control do Mac,
  espalha todas as janelas na tela pra escolher.
- Histórico de área de transferência — o Win+V, guarda os últimos
  itens copiados.
- Cantos ativos — jogar o mouse num canto dispara uma ação
  (mostrar desktop, abrir menu, etc).
- Night light — a tela esquenta de cor à noite. Combina com a
  identidade Golden Hour, igual o modo escuro automático.

### Acessibilidade (alimentam o preset PCD Max)
> Estas NÃO são "ideia futura" de verdade — são o que dá peso ao
> ângulo "DE desenhado por pessoa com deficiência" no surprise
> drop. Merecem subir na fila e entrar antes do v1.0.
- Lupa de tela — ampliar um pedaço da tela.
- Cursor grande e com realce — achar o ponteiro fácil.
- Teclas de aderência — apertar atalho uma tecla por vez.

## 🚫 FORA DO ESCOPO (não perseguir)
> Features de empresa gigante — fogem de um DE de portfólio.
- Histórico de versões de arquivo (tipo Time Machine)
- Continuidade entre dispositivos
- Assistente de voz

## 🎯 PRINCÍPIOS DO PROJETO
- Uma etapa de cada vez, terminada e testada antes da próxima
- Feature que depende de sistema não-pronto = item dim + nota no DEBT
- Zero hex hardcoded — tudo no theme.toml
- Commits atômicos, cada um compila e roda
- Backup tag antes de cada etapa
- Acessibilidade PCD: hit targets ≥ 40px, WCAG AA
