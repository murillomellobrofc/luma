# 🗺️ Luma — Roadmap

> Desktop Environment em Rust + Smithay (Wayland)
> Identidade: Golden Hour (coffee / coral / honey / sage)
> Meta: v1.0 como portfólio · timeline ~2 anos · surprise drop

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

### Desktop
- Menu de contexto do desktop
- Wallpapers padrão Golden Hour + galeria
- Ícones de arquivo no desktop (ler, desenhar, grid)
- Seleção, drag, rubber-band, setas
- Menu per-icon (abrir, copiar, renomear, lixeira)
- Renomear inline (sem KDialog)

### Em andamento
- 🔨 H55 — Calendário ao clicar no relógio

---

## 🔜 PRÓXIMAS ETAPAS (caminho pro v1.0)

### Curto prazo
- Acender os 4 itens dim do menu de contexto
  (Criar novo, Colar, Organizar ícones, Modo de edição —
  agora que existem ícones no desktop)
- Applets restantes: wifi, bluetooth, bateria (popups)

### Médio prazo
- Notificações fase 2: empilhar, agrupar por app, central/histórico
- Sistema de widgets configuráveis (estilo KDE Plasma)
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
- Modo de edição geral do Luma (estilo KDE, unificado)
  → um modo onde o usuário organiza tudo num lugar só: reposiciona
    ícones do desktop E mexe nos widgets/layout dos painéis
  → depende dos ícones no desktop (✅ já tem) + sistema de widgets
    (❌ ainda não) — por isso vem junto/depois dos widgets
  → é o que acende o item dim "Modo de edição" do menu de contexto
  → junta 3 itens do roadmap num só: item dim do menu + sistema de
    widgets + este modo de edição geral

  Como funciona:
  · entra pelo item "Modo de edição" do menu de contexto + atalho
    de teclado; sai com botão "Pronto" ou Esc (tudo trava de novo)
  · tela ganha overlay sutil, painéis/widgets ficam com borda de
    realce, barra no topo com "Adicionar widget" e "Pronto"
  · dá pra: reposicionar ícones do desktop · adicionar/mover/
    remover/configurar widgets (desktop e painéis) · mexer nos
    painéis (altura, alinhamento, fixar/flutuar, criar novo) ·
    trocar wallpaper e preset

  Alça do widget — diferencial PCD:
  · cada widget focado mostra uma alça com botões (configurar,
    mover, redimensionar, remover)
  · no KDE essas alças são minúsculas e irritantes — no Luma,
    hit target ≥ 40px, botões grandes, cor Golden Hour
  · cortar o "rotacionar" do KDE (ninguém usa)
  · modo de edição usável por quem tem tremor / baixa visão = a
    história do PCD Max na prática

  Fatias (cada uma compila e roda sozinha):
  1. modo só reposiciona ícones do desktop (base já existe)
  2. + adicionar/mover/remover/configurar widgets
  3. + edição de painel (altura, alinhamento, criar painel)
- Menu de contexto da taskbar (depende dos widgets)
- Tela de configurações nativa do Luma
- Configuração de tela (resolução, monitores)

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
- Centro de configurações (já listado em "médio prazo")

### Infra / técnico
- Backend DRM (rodar fora de outro compositor)
  → desbloqueia: cantos arredondados reais, multi-window nativo
- HiDPI / escalonamento
- Geração de tema GTK/Qt (apps de terceiros adotam Golden Hour)

### Rumo ao v1.0
- Polish geral, QA completo
- Os 4 presets visuais: Classic, Mint, Dock, PCD Max
- Documentação final
- Surprise drop: Diolinux, r/linux, Phoronix, Hacker News

---

## 📌 DÍVIDAS TÉCNICAS ANOTADAS (docs/DEBT.md)
- Cantos arredondados de janela (espera backend DRM)
- Notificações: empilhar / central / ações / som
- Menu de contexto: itens dim que esperam features:
  · Criar novo, Colar, Organizar ícones → ícones no desktop (quase lá)
  · Modo de edição → sistema de widgets + modo de edição geral
- Menu da taskbar (espera sistema de widgets)

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
- Loja de apps própria
- Assistente de voz

## 🎯 PRINCÍPIOS DO PROJETO
- Uma etapa de cada vez, terminada e testada antes da próxima
- Feature que depende de sistema não-pronto = item dim + nota no DEBT
- Zero hex hardcoded — tudo no theme.toml
- Commits atômicos, cada um compila e roda
- Backup tag antes de cada etapa
- Acessibilidade PCD: hit targets ≥ 40px, WCAG AA
