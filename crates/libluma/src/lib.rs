//! libluma — design system Golden Hour dos apps Luma.
//!
//! Lê os tokens de `luma-theme` (fonte da verdade compartilhada com
//! o compositor) e expõe pros apps libcosmic:
//!
//! - O theme cosmic Golden Hour pronto pra `Settings::theme()`.
//! - Helpers de cor (struct `GoldenHour`) com a paleta nos tipos
//!   `Srgba`/`Color` esperados pelos widgets.
//! - (Futuro) Componentes visuais compartilhados — section header,
//!   icon box, card, sidebar item, mini-modal, toast. Cada um migra
//!   do `luma-config` quando um segundo app validar o padrão.
//!
//! O compositor Smithay NÃO consome libluma — ele renderiza no braço
//! lendo o `luma_theme` direto.
