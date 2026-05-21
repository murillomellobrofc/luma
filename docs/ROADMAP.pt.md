# Visão e roadmap — Luma

---

## A visão

Luma quer ser um ambiente de desktop que não obrigue ninguém a
escolher entre bonito, configurável e acessível.

Hoje, quem usa Linux costuma fazer um trade. Alguns desktops são
lindos mas engessados. Outros são configuráveis mas frios. E a
acessibilidade quase sempre é um apêndice, um menu de ajustes que
alguém lembrou de adicionar no fim.

Luma rejeita esse trade. A proposta é um desktop com layout
tradicional e familiar, ergonomia no estilo Cinnamon e Windows,
configurabilidade no estilo KDE, e acessibilidade tratada como
fundação. Tudo embrulhado numa identidade visual própria, a Golden
Hour.

A referência técnica é o COSMIC, da System76, pela arquitetura
moderna em Rust. Mas Luma não depende dele nem o copia: segue caminho
próprio.

---

## O diferencial

Luma é desenhado por uma pessoa com deficiência física. Isso muda
tudo. Acessibilidade deixa de ser um item de lista e passa a ser a
lente pela qual cada decisão é tomada.

Não é sobre adicionar um modo especial. É sobre o desktop inteiro ser
confortável: alvos de clique grandes, navegação por teclado em todo
lugar, contraste que funciona, e a opção de um preset de
acessibilidade extrema para quem precisar.

Esse é o ângulo que torna o Luma diferente de qualquer outro ambiente
de desktop existente.

---

## Roadmap

O desenvolvimento é dividido em etapas. A linha do tempo realista é de
cerca de dois anos até a versão 1.0.

### Fundação visual — concluído ou em curso

- Compositor Wayland funcional
- Menu de aplicativos em tela cheia
- Workspaces
- Taskbar estilo KDE com seis zonas
- Bandeja do sistema lendo dados reais
- Resolvedor de ícones compatível com aplicativos GTK e KDE
- Decoração de janelas

### Sistema vivo — próximos passos

- Sistema de notificações
- Applets clicáveis: volume, rede, bluetooth, bateria
- Calendário ao clicar no relógio
- Bandeja expansível, com suporte a ícones de terceiros

### Aplicativos e refino

- Aplicativos nativos do Luma
- Suporte a HiDPI e escalonamento
- Backend DRM, para rodar direto sem depender de outro ambiente
- Suporte a múltiplas janelas

### Versão 1.0

- Polimento geral e correção de bugs
- Lançamento

### Depois da 1.0 — os presets

Luma terá quatro presets visuais, trocáveis pelo usuário:

- **Classic** — o menu em tela cheia, visual padrão
- **Mint** — um menu estilo Cinnamon, mais tradicional
- **Dock** — uma barra inferior estilo macOS
- **PCD Max** — acessibilidade levada ao extremo

Cada preset é uma forma diferente de viver o mesmo desktop.

---

## Compatibilidade

Luma roda aplicativos dos dois grandes mundos do Linux: GTK e KDE/Qt.
Como fala Wayland, qualquer aplicativo moderno funciona. O resolvedor
de ícones encontra os ícones de qualquer tema instalado.

Mais adiante, o Luma também vai gerar temas para GTK e Qt, para que
os aplicativos de terceiros vistam a identidade Golden Hour e o
desktop fique visualmente coerente de ponta a ponta.

---

## Filosofia

Algumas ideias guiam o projeto:

- Terminar é melhor que ser perfeito. Cada etapa é fechada antes da
  próxima começar.
- Constância vale mais que heroísmo. O Luma cresce um commit por vez.
- Nada de trabalho jogado fora. Componentes prontos são preservados,
  mesmo quando o foco muda.
- O usuário decide. Configurabilidade de verdade significa respeitar
  escolhas.

Luma é um projeto de longo prazo, construído com paciência. O destino
é claro, e o caminho é feito um passo de cada vez.
