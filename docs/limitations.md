# Limitações conhecidas

Limitações do backend atual (winit + GLES 2.0) que ainda não temos como
contornar, junto com a estratégia de fallback adotada.

---

## Blur do wallpaper no menu fullscreen (H32.10.2)

**Pretendido pelo spec:** menu híbrido deveria desfocar (gaussian blur
~30px) o wallpaper e janelas atrás antes de aplicar o backdrop Coffee
escuro alpha 0.94, igual ao Spotlight do macOS.

**Realidade:** o backend winit+GLES 2.0 do Smithay não expõe shader de
blur. Aplicar um blur multi-pass exigiria:

1. Render offscreen do que estiver atrás
2. Shader de gaussian blur horizontal + vertical (2 passes)
3. Composição final com alpha

Fora do escopo desta etapa.

**Fallback adotado:** overlay sólido Coffee `#1F1611` alpha 0.94 cobrindo
o output inteiro. O alpha alto deixa o wallpaper levemente visível mas
sem desfoque — visualmente próximo do efeito desejado, sem a difusão
fotográfica.

**Quando reativar:** quando migrarmos pro backend DRM (planejado pós
H40) com renderer GLES 3.0 / shader pipeline customizado, o blur entra
sem renegociar a arquitetura.
