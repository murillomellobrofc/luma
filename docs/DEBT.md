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
