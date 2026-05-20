# Visual debt — H32.5 / H32.6 / H32.7 / H32.9

Itens da spec H32.5 que não foram implementados nesta etapa por
dependerem de infra que ainda não existe. São débitos visuais
documentados pra resolver em etapa futura quando a base estiver pronta.

## H32.5.6 — Glow no indicador âmbar do bloco ativo

A spec pede `blur 3px` no indicador. Implementado como linha sólida sem
glow porque não temos passo de pós-processamento (filtro gaussiano)
disponível pelo Smithay/GLES — só blits de buffer simples.

**Como resolver:** pré-renderizar uma textura Memory de
`(width + glow*2) × (h + glow*2)` com a linha sólida no centro + halos
de alpha decrescente nas bordas (tipo `make_horizontal_glow_line(w, h,
glow_radius, color)` em `gfx.rs`).

## H32.5.7 — Transição de hover 150ms ease-out

A spec pede transição suave entre estados Inactive↔Hover↔Active. Hoje
é instantâneo: a cada frame o `block_state_at` decide o estado e o
render escolhe o buffer pré-renderizado correspondente.

**Como resolver:** introduzir um sistema de animação (opcional —
animações em compositor são polêmicas, alguns usuários odeiam). Quando
existir, o `BlockState` vira interpolação `f32` 0..1 entre 2 estados,
e o render faz blend dos 2 buffers via alpha.

## H32.5.11 — Tooltip do show-desktop ("Mostrar área de trabalho")

Não temos sistema de tooltip ainda. Hoje o usuário tem que descobrir
o que é a barrinha do canto direito.

**Como resolver:** criar `crate::tooltip` com um `TooltipState` que
contém o texto + posição + timer (mostra após 500ms hover, esconde
imediatamente em mouse leave). Renderiza como popup acima da
taskbar.

## H32.5.14 — Adaptação dos window controls pra header light (parcial)

**Implementado:** detecção pelo `theme.border` global. Se a luminância
rec.601 da cor do header for > 0.5, os ícones default vêm em Coffee
alpha 0.7 em vez de Cream alpha 0.85. Hover (squircle Honey/Sage/Coral)
mantém a mesma lógica de contraste por região.

**Pendente:** detecção POR JANELA. Se o cliente Wayland negociar um
tema light no `xdg_decoration` (ou via outro protocol futuro tipo
xdg-shell preferred-color-scheme), cada janela poderia ter ícones
adaptados independentemente. Hoje a decisão é global — todas as
janelas usam a mesma cor de ícone, derivada do `theme.border` único.

**Como resolver:** ler o color-scheme da janela cliente (se existir),
guardar no `WindowDecoration`, e passar pra `window_control_elements_at`
escolher o ícone certo por janela. Vai precisar pré-renderizar 6
buffers em vez de 4 (light + dark de cada controle).

## H32.5 — Applets que não existem

A seção 6 do spec SVG cita os applets `tray`, `volume`, `wifi`,
`battery`, `bluetooth`. Esses não existem hoje no luma — vão entrar
como módulos próprios em H42–H44 quando viraremos applets independentes.

Por enquanto a taskbar tem só: launcher · window-list · clock ·
show-desktop.

## H32.6.7 — Débitos da decoração de janela

Esta seção lista o que a spec H32.6 pediu mas que NÃO foi implementado
nesta etapa por dependência de infra ausente. Tudo é pintável no
futuro sem refactor arquitetural.

### Corner radius da janela (token `window.corner_radius = 8`)

A spec pede cantos arredondados em todo o retângulo da janela. Hoje a
decoração é renderizada como 4 retângulos separados (header +
left/right/bottom border + content surface do cliente), todos
quadrados. Aplicar corner radius exigiria:

- Header virar `MemoryRenderBuffer` pré-renderizado com cantos top
  arredondados (mesma técnica do menu H32.5).
- Bordas laterais/bottom também precisam de transparência nos cantos
  pra "envelopar" o canto inferior.
- A surface do cliente é independente; recortar ela exige render-to-
  texture (passo extra de GLES não usado hoje).

**Como resolver:** trocar `WindowDecoration.background` por uma
textura Memory já com cantos. Bordas idem. Surface do cliente continua
retangular, mas sem cantos visíveis pra fora porque o header e bordas
cobrem.

### Sombra externa da janela (tokens `window.shadow_*` + `window_shadow_*`)

A spec pede blur 8px offset y=4 alpha 0.4. Sem filtro pós-render
disponível no Smithay+GLES. Idêntica limitação do glow âmbar da
H32.5.6.

**Como resolver:** pré-renderizar textura RGBA `(w + 2*blur) ×
(h + 2*blur)` com sombra desenhada em volta (gaussian falloff em
software). Renderizar atrás de cada `LumaWindow`. Custo de memória
considerável (~4MB pra janela 1920×1080) — fica pra implementação
quando GLES filter primitives entrarem (ou via Smithay damage
tracking otimizado).

### Glow externo da borda focada (token `window_border_focused_glow = 0.5`)

Mesmo limitação do shadow. Mas como a spec aceita "instantâneo" no
fallback, a borda âmbar alpha 0.7 sozinha já é suficiente pra
diferenciar foco.

**Como resolver:** mesma técnica do shadow. Pré-renderizar 4 buffers
de halo (top/bottom/left/right) com gradient alpha decrescente, pintar
fora do bounding box visível.

### Fonte Manrope + weight 500 (tokens `window.title_font_family/_weight`)

Spec pede Manrope. Sistema usa fallback chain (NotoSans → DejaVu →
Liberation → Adwaita → Carlito → Cantarell). Manrope não está
instalado em hosts comuns. Weight 500 também ignorado — `fontdue` só
carrega 1 weight por Font instance, e o caminho ainda não suporta
variants.

**Como resolver:**
- Empacotar Manrope nos assets do Luma (~200KB) OU
- Detectar Manrope via fontconfig e fallback documentado.
- Weight: carregar arquivo separado pro 500 (Manrope-Medium.ttf) e
  abstrair via enum no `TextRenderer`.

### Animação 150ms ease-out

Já documentado no H32.5.7. A H32.6 deveria ter transição entre estado
focada/sem-foco da borda externa — hoje é instantâneo. Aceitável (o
contraste âmbar/cream é claro o suficiente sem animação).

### Detecção dark/light POR JANELA

O `ButtonGlyphs` é `OnceLock<ButtonGlyphs>` — escolhe a cor do ícone
1× no init. Hoje todas as janelas têm o mesmo `window_header_bg_dark`,
então isso "funciona". Mas se no futuro tivermos clients informando
seu próprio color-scheme (xdg-decoration ou xdg-shell
preferred-color-scheme), precisariamos:

- Pré-renderizar 2 conjuntos de buffers em `build_button_glyphs`:
  `light_variant` (Coffee 0.7) e `dark_variant` (Cream 0.85).
- `window_control_elements_at` escolheria por janela qual conjunto
  usar baseado no header dessa janela.

Hoje `Theme::is_dark_header` aceita o argumento `header_bg` por flexi-
bilidade futura, mas todos os call sites passam a cor global.

### Title font weight 500 (mesmo motivo do Manrope)

Mesmo problema do `title_font_family`. fontdue carrega 1 weight. Weight
500 fica como string no theme sem efeito real.

### Detecção light por client_hint / manual_override

`WindowThemeDetectionMode::ClientHint` e `ManualOverride` foram criadas
mas não implementadas — caem em `Luminance` com warn. Implementação
exige integração com `xdg-decoration` (client hint) ou um campo
`override_dark: Option<bool>` no theme (manual override).

### BORDER_THICKNESS 3px vs spec 1px

A spec mostra borda de 1px. Hoje BORDER_THICKNESS=3 (necessário pro
hit-test do resize_grab). Decisão pragmática: pintar a cor com alpha
real (window_border_focused_alpha=0.7) em 3px — fica "moldura" um
pouco mais grossa que a spec, mas elegante e funcional.

**Como resolver definitivamente:** desacoplar hit-test (3px lógico)
de pintura (1px visível). Hoje os 2 são o mesmo buffer.

## H32.7 — Débitos do menu launcher

### Fonte Fraunces no username + Manrope nos rows

A spec pede Fraunces serif pro username, Manrope sans pros rows e
labels do menu. O TextRenderer atual carrega 1 fonte default
(NotoSans/DejaVu/Liberation, primeira disponível). Não suporta:
- Family selection (Fraunces vs Manrope vs system)
- Weight variants (400/500/600)
- Italic (greeting style)

**Como resolver:** estender `text::TextRenderer` pra map de `Font` por
nome + weight, com fallback chain. Empacotar Fraunces e Manrope nos
assets do Luma (~400KB combinados).

### Letter-spacing real nos sub-headers

Spec pede `column_header_letter_spacing = 2px` pros labels FAVORITOS
e LUGARES. Implementado inserindo espaços manuais entre letras
("F A V O R I T O S"). Funcional mas não é o mesmo.

**Como resolver:** quando o TextRenderer suportar `letter-spacing`,
passar 2 e remover os espaços manuais.

### Animações 120-150ms (todas)

A spec pede transição suave em vários estados:
- Search bar default ↔ focused (cor de fundo + border)
- Row hover (background)
- Session button hover (squircle aparece com fade)
- Menu open/close (animation_duration_ms=150)

Hoje tudo instantâneo. Sem sistema de animação no compositor.

**Como resolver:** mesmo plano da H32.5.7 — adicionar interpolação
`f32` 0..1 entre estados, blend de 2 buffers via alpha.

### Sombra externa do menu (`menu_launcher.shadow_*`)

`shadow_blur=12, shadow_offset_y=6, shadow_alpha=0.4` pendentes.
Mesma limitação do shadow de janela (sem filtro pós-render no GLES).

**Como resolver:** pré-renderizar texture maior que o menu com sombra
desenhada em volta. Custo ~1MB por menu (560×540 + 12px de cada lado).

### Glow externo do indicador focus + dos hover squircles

`hover_glow_blur=4`, `hover_glow_alpha=0.4` na sessão. Indicador focus
3×24 também tem glow âmbar na spec. Não implementado.

**Como resolver:** pré-renderizar variante "glow" de cada squircle
com halo gaussiano (área maior que o squircle). Trabalho moderado.

### Divider vertical entre colunas com gradient

Spec pede `gradient = true` no divider — transparente nas pontas,
0.18 alpha no meio. Hoje SolidColorBuffer 1px com alpha uniforme.

**Como resolver:** trocar SolidColorBuffer por MemoryRenderBuffer 1×h
pré-renderizado com gradient vertical (transparent→alpha→transparent).
Similar ao gradient horizontal do background da taskbar (H32.5.3).

### Estado focused da search bar (visual)

A spec quer: bg muda pra `search_bg_focused` (#3D2520) + border 1.5px
âmbar quando search está em foco do teclado. Hoje o bg é sempre
`search_bg` e não tem border.

**Como resolver:** detectar foco de teclado na search (today só ativa
quando menu_open, sem estado "blur"). Adicionar overlay solid pra
mudar bg + outline amber quando focused.

### Persistência do recently_used (~/.cache/luma/...)

Hoje recently_used é in-memory (LumaState.recently_used: HashMap).
Reset ao reiniciar o luma.

**Como resolver:** serializar pra `~/.cache/luma/recently_used.json`
(Vec<(app_name, timestamp)>) ao spawn de app + load no boot.

### "Circle" da bolinha recently-used é quadrado 3×3

A spec pede círculo. Hoje é SolidColorBuffer quadrado (visualmente
próximo a 3px mas geometricamente quadrado).

**Como resolver:** trocar por MemoryRenderBuffer dedicado com gfx::
make_dot(radius, color) — círculo cheio com antialias.

### Pílula "Super K" com glifo de Super (logo) em vez de "Super"

A spec mostra `⌘ K` (Mac-style). No Luma faz sentido usar logo do
Linux (Super) ou um caractere ⊞. Hoje texto literal "Super K" via
`shortcut_keybind.replace('+', ' ')`.

**Como resolver:** usar uma fonte que tenha o glifo, OU desenhar o
ícone (square com plus interior) em gfx.rs.

### Sessão tem 4 ícones (sem 5º Logout)

Decisão arquitetural da H32.7.9: removida `SessionAction::Logout` —
spec só tem Lock/Sleep/Restart/Power. Logout via close compositor
não é exposto na sessão visual. Quem quiser pode usar Power → reboot
+ login screen do display manager.

### Tooltip de sessão

`SessionAction::label()` continua disponível ("Bloquear", "Dormir",
etc) pra ser usado em tooltip futuro quando a infra de tooltip
(débito da H32.5.11) chegar.

### Emoji da saudação

A spec H32.7.10 sugere `☀/☕/🌙` ao final da saudação ("Bom dia,
$USER ☀"). Removidos do output porque o TextRenderer com fonte
fallback do sistema (NotoSans/DejaVu/Liberation/Adwaita) não tem
esses codepoints — renderiza como quadradinhos. Hoje só texto.

**Como resolver:** adicionar fonte com cobertura emoji (NotoEmoji
ou OpenMoji) na chain de fallback, OU desenhar os ícones em
pixel-art (igual fizemos pros window controls e session icons).

### menu.rs é candidato a refactor completo

Durante a H32.7 a topologia do menu foi REESCRITA (H32.7.8b) por
cima da arquitetura H16.5 original. O resultado funciona mas o
arquivo `src/menu.rs` (~1150 linhas) acumulou camadas:
- Estrutura H16.5 (2 colunas + user_label só direita)
- Refinamentos H32.5 (cantos arredondados, divider, etc)
- Tokens H32.7.1 + sub-structs
- Topologia spec H32.7.8b (profile bar full-width, search topo)
- Refinamentos visuais H32.7.4-7 (avatar, sub-headers, recently-used)

Alguns campos ficaram dead (SEARCH_PROMPT, user_x/user_y, BLOCK_PADDING)
porque vieram de fases anteriores. Hit-test foi atualizado várias vezes
e tem condições redundantes.

**Quando refatorar:** quando entrarmos na fase de applets independentes
(H42-H44), o menu deveria virar `crate::menu` separado em sub-módulos:
- `menu::layout` (constantes e cálculo de posições)
- `menu::profile_bar`, `menu::search_bar`, `menu::row`, `menu::session`
- `menu::render` (compõe os sub-elementos)
- `menu::hit_test` (testa em uma única fn baseada em layout)

A reescrita seria ~50% do código atual (eliminando consts mortos e
condições redundantes), com arquitetura mais legível e mais fácil de
estender (workspace switcher, applets, etc).

## H32.9 (Spotlight search) — débitos remanescentes

A etapa H32.9 entregou search funcional Spotlight-like com 4
categorias (apps, pastas, cálculo, web), fuzzy match, highlight
char-a-char, keyboard navigation, hover/focus feedback visual.
Débitos que ficaram fora do escopo:

### Empty state com 4 dicas educativas

`MenuState.empty_tips` rasteriza 4 dicas coloridas (apps/calc/web/
comandos) mas o render visual com badges + cards completos NÃO é
desenhado. Renderizar exige layout fora da grid atual (2 colunas
fixas). Decisão: deixar pra quando menu virar sub-módulos (H42-H44).

### No-results state com logo dim

`is_no_results()` retorna true quando todas categorias vazias e
title/subtitle são renderizados centralizados. Mas o logo do Luma
mini com 25% alpha (conforme spec) NÃO está sendo desenhado —
exigiria indexar o asset SVG ou desenhar o glifo do logo em pixel art.

### Clipboard pro Enter na calc

Briefing pediu Enter na linha do cálculo copiar o resultado pro
clipboard. Wayland clipboard via wl-clipboard ou wlrs-data-control
exige implementação adicional. Hoje o resultado fica só visível.

### Debounce de 80ms

Briefing pedia debounce de 80ms pro filter. Hoje filter roda
síncrono a cada frame. Funciona OK pra ~50 apps; pode reaver em
sistemas com milhares.

### Threshold do fuzzy

`theme.search.fuzzy_threshold = 0.3` está documentado mas NÃO é
aplicado. Hoje aceita qualquer Some() do skim. Pra filtrar só
matches "fortes" precisaria normalizar o score (i64 absoluto) pra
range 0-1.

### Tab pula categoria parcialmente

Tab cicla Apps↔Folders. Calc e Web não são incluídos no ciclo (não
navegáveis com setas ainda). Fix: estender `selected_hit` e
`nav_total` pra cobrir as 4 categorias.

### Action no calc

Calc renderiza resultado mas Enter sobre ele não copia nem dispara
nada. Deveria copiar pro clipboard (vide acima).

### Click handler pra web search row

`pointer.rs` não tem case pra clicar no row da web. Hoje só Enter
dispara. Adicionar `MenuHit::WebSearch` + hit_test daquela linha.

## H32.9 fixes de validação visual

Após os 10 commits originais (H32.9.1 a H32.9.10), 9 fixes
adicionais foram aplicados em resposta a feedback:

- **fix.click**: click em área neutra do menu não fecha mais
- **fix.truncate / .truncate.2**: paths longos truncam em 20 chars
  com "…" prefix
- **fix.pill / .pill.2**: pílula `⌘ K` removida do render
- **H32.9.0.1**: border âmbar permanente na search bar
- **H32.9.0.2**: cursor `|` piscando a 500ms
- **H32.9.0.3**: feedback de focus — border satura no click

## H32.10.5 — Fade do menu pode "travar" sem interação

A animação fade-in/fade-out do menu fullscreen funciona via `MenuAnimation::tick`
que avança o alpha proporcional ao delta entre frames. Mas o backend
winit só renderiza quando há eventos (mouse move, tecla, etc), então se
o usuário abre o menu e fica parado, a animação para no meio.

**Como resolver:** agendar um redraw forçado a cada ~16ms durante
`menu_anim.alpha != target` (via `data.notifier.notify_loop()` ou
similar). Quando migrar pro DRM backend (planejado pós H40) o render
loop é dirigido por vsync e o problema some sozinho.
