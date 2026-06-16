# Legacy: TitleBar (M2, revertido)

`title_bar.rs` é a barra de título Golden Hour custom do M2 (CSD: dots
coral/honey/sage + título + drag). **Revertida**: o Luma decora as
janelas dos apps dele **server-side** (header oficial desenhado pelo
compositor), então os apps rodam frameless e o libluma não expõe barra.

Guardado de propósito (não compila — fora do module tree). Se um dia a
estratégia mudar (apps com CSD próprio), o componente está aqui pronto.
O token `chrome` (#1B1714) que ele usa segue no luma-tokens.
