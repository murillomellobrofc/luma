# Legacy cosmic app (pré-migração iced M1)

`app_cosmic.rs` é o `AppModel` `cosmic::Application` original do luma-config
(sidebar + páginas Widgets/Wallpaper/Preset + dialog AddWidget + toasts +
single-instance). **Não compila** (não está no module tree, e o app dropou
o libcosmic). Guardado de propósito: o código de render das páginas serve
de referência quando a grade (2b.2) e a navegação (2c) forem construídas
em iced. Histórico completo também está no git antes da tag
`backup/pre-migracao-iced`.
