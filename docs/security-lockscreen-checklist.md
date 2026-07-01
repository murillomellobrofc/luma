# Checklist de verificação — lock screen & segurança (H58)

Verificação **manual** das correções de segurança que não são cobríveis
por unit test (dependem do compositor rodando + FFI real de PAM /
libcanberra / D-Bus). Rodar numa sessão real (ou aninhada) antes de
confiar no lock screen.

Os unit tests automáticos das funções puras ficam em
`compositor/luma/src/auth.rs` (`cargo test -p luma auth::`).

## 1. Bypass por Esc (CRÍTICO — regressão)

- [ ] Bloquear via hotkey → apertar **Esc** na capa → **permanece bloqueado**.
- [ ] Bloquear via menu de sessão → **Esc** → permanece bloqueado.
- [ ] Bloquear via modal de confirmação → **Esc** → permanece bloqueado.
- [ ] Revelar o painel (qualquer tecla) → **Esc** → volta pra capa, **sem** destravar.
- [ ] Segunda vez: capa → Esc → Esc → **continua bloqueado** (o antigo bug destravava aqui).

## 2. Autenticação PAM

- [ ] Senha correta → destrava.
- [ ] Senha errada → mostra "senha incorreta", continua bloqueado.
- [ ] Campo vazio + Enter → nada acontece (não dispara PAM).
- [ ] Rodar com `$USER` desanexado do env (`env -u USER ...`) → ainda
      autentica o usuário real (via `getpwuid`), **não** tenta `root`.

## 3. Verificação de conta (`pam_acct_mgmt`)

- [ ] Bloquear a conta (`sudo passwd -l $USER` ou expirar via
      `sudo chage -E 0 $USER`) → senha **válida** → **não** destrava.
- [ ] Reverter (`sudo passwd -u $USER` / `sudo chage -E -1 $USER`) →
      senha válida → destrava normalmente.

## 4. ext-session-lock (cliente Wayland não limpa lock PAM)

- [ ] Com o lock interno ativo (hotkey/menu), um cliente que cria e
      destrói um `ext_session_lock` **não** destrava a sessão
      (log: `session lock: unlock ignorado — lock PAM ativo`).
- [ ] Lock iniciado só pelo protocolo (sem PAM) ainda pode ser
      destravado pelo próprio cliente.

## 5. Timeout do auth em andamento (DoS)

- [ ] Se o módulo PAM travar (simular com módulo lento), após ~30s o
      estado "Verificando…" reseta e mostra "tempo esgotado, tente de
      novo" — o usuário legítimo consegue tentar de novo.
- [ ] `Cancel` (Esc no painel) durante "Verificando…" limpa o estado.

## 6. Notificações — decode de imagem (DoS)

- [ ] Enviar `Notify` com `image-data` de dimensões enormes
      (ex.: `w=h=25000`) → notificação é **descartada**, sem panic nem
      pico de memória (requer `LUMA_TAKE_NOTIFICATIONS=1`).
- [ ] Ícone de notificação válido (≤1024×1024) → renderiza normal.

## 7. Ícones de tray (SNI) — limites + tmp seguro

- [ ] App SNI com ícone-bomba (imagem com dimensões declaradas enormes)
      → ícone é descartado, compositor não sofre OOM.
- [ ] PNGs de tray são escritos em `$XDG_RUNTIME_DIR/luma-tray/`
      (0700), não em `/tmp/luma-tray`.
- [ ] Um symlink pré-plantado no caminho do PNG não é seguido (o save
      remove a entrada antes de escrever).

## 8. Som de notificação (data race)

- [ ] Rajada de notificações com som → sem crash/UB (acesso ao
      `ca_context` serializado por `ctx_lock`).
