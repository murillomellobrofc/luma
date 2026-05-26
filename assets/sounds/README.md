# Notification sounds (H54.P6)

Os 2 assets de áudio do Luma são deliverable separado:

- `notification-default.ogg` — chime suave Golden Hour, 0.3–0.6s
- `notification-critical.ogg` — mais presente, ainda na estética, 0.6–1s

Formato: ogg vorbis, mono ou estéreo, 44.1/48 kHz.

Se os arquivos não existirem, o engine cai em silent fail — toast aparece
normal, sem som. O build não quebra.
