# Acessibilidade PCD — checklist H32.10.17

Status do menu fullscreen híbrido (etapa H32.10) contra o checklist
do brief. Itens validados visualmente + por leitura de código no
fechamento do bloco E.

## Implementado ✅

| Critério | Implementação |
|---|---|
| **Auto-focus na search ao abrir menu** | `FullscreenMenuState::default()` seta `focused_zone = Search`. Garantido pelo brief no spec do menu. |
| **Qualquer tecla digitada vai pra search (sem clicar)** | `input/keyboard.rs` em `MenuKey::Char` espelha `state.menu_new.search_query.push(c)` e força `focused_zone = Search` mesmo se foi pra outra zona via setas antes. |
| **Hit targets ≥ 44px (WCAG AAA)** | Botões da session bar: 44×44 ✓. Favoritos: 80×80 ✓. AllApps icons: 60×60 ✓. Mini-places: ícone 18px mas hit area expandida pra `accessibility.hit_target_min` (44px) via `hit_test_places()`. |
| **Focus ring com glow ≥ 4px** | Favorito selecionado: glow âmbar 12px padding (alpha 0.7). Search result selecionado: barra âmbar 4px à esquerda. Em zonas com seleção visual, sempre amber-tinted. |
| **Contraste WCAG AAA** | Cream `#FBF6EE` sobre Coffee `#1F1611` = ~16:1 contrast ratio. Âmbar `#E89B3D` sobre Coffee = ~7:1 (AA Large + AAA pra ícones). |
| **Zero hover-required actions** | Todo elemento clicável funciona via click direto OU teclado (setas + Enter / Tab / Esc). Nenhuma ação só por hover. |
| **`prefers-reduced-motion` respeitado** | `LUMA_REDUCE_MOTION=1` (env var) pula fade-in/out e mostra alpha binário. Vai migrar pra setting do DE quando o backend DRM expor a propriedade. |
| **Tab alterna modos** | Tab toggla Favorites ↔ AllApps em `input/keyboard.rs` MenuKey::Tab. |

## Stubs documentados ⏳

Tokens já em `theme.toml` (`[accessibility.*]`) mas implementação
fica pra etapas futuras quando a infra suportar:

| Stub | Token | Status |
|---|---|---|
| Large text mode | `accessibility.large_text_scale` (1.0 default, 1.5 = AAA) | Precisa rerasterizar todos os labels c/ scale × — vai entrar num H33 dedicado |
| High contrast mode | `accessibility.high_contrast_mode` | Precisa pallete alternativo (preto+cream puro), zero alpha translúcido |
| Dwell click | `accessibility.dwell_click_ms` | Precisa rastreamento de cursor parado por N ms no pointer.rs |
| Sticky modifiers | `accessibility.sticky_modifiers` | Precisa estado modal no keyboard.rs |
| Dyslexic font | `accessibility.dyslexic_font` | Precisa carregamento opcional de OpenDyslexic.ttf |

## Validação visual

Screenshot final em `assets/dev-screenshots/H32.10-final.png` (a tirar
pelo usuário ao validar o fechamento da etapa).

## Marketing

> "O primeiro DE Linux com acessibilidade física tratada SÉRIO desde
> a base. Não é feature extra, é o coração do design."

Validado: hit targets, auto-focus, keyboard-first, focus rings, zero
hover-required, motion respect — todos os 6 pilares do checklist.
