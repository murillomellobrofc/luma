#!/usr/bin/env bash
#
# dev-run.sh — sobe o ambiente de teste do Luma aninhado numa sessão Wayland
# já existente (o KDE/Fedora do dia a dia).
#
# Sobe os três processos na ordem certa, descobre o WAYLAND_DISPLAY do
# compositor sozinho, separa os logs, e derruba tudo junto num Ctrl+C só.
#
#   ./scripts/dev-run.sh
#
# Por que descobrir o socket em vez de fixar "wayland-1": o compositor pede
# o primeiro nome livre. Se já houver outra instância (ou sobra de um crash),
# ele vira wayland-2, e os clientes conectariam no compositor ERRADO — ou no
# host, o que é pior porque parece funcionar. O nome sai no log
# (`main.rs:78`); o compositor também seta WAYLAND_DISPLAY, mas só no próprio
# processo (`state.rs:1530`), então de fora não dá pra ler.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="${LUMA_DEV_LOG_DIR:-/tmp/luma-dev}"
BIN_DIR="$REPO/target/debug"
# Quanto esperar o compositor anunciar o socket.
STARTUP_TIMEOUT="${LUMA_DEV_TIMEOUT:-30}"

COMPOSITOR_PID=""
BAR_PID=""
MENU_PID=""
SHUTTING_DOWN=0

# ─── Saída ────────────────────────────────────────────────────────────

if [[ -t 1 ]]; then
    C_DIM=$'\033[2m'; C_BOLD=$'\033[1m'; C_RED=$'\033[31m'
    C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'; C_OFF=$'\033[0m'
else
    C_DIM=""; C_BOLD=""; C_RED=""; C_GREEN=""; C_YELLOW=""; C_OFF=""
fi

say()  { printf '%s==>%s %s\n' "$C_BOLD" "$C_OFF" "$*"; }
ok()   { printf '%s  ok%s %s\n' "$C_GREEN" "$C_OFF" "$*"; }
warn() { printf '%s  !!%s %s\n' "$C_YELLOW" "$C_OFF" "$*"; }
die()  { printf '%s ERRO%s %s\n' "$C_RED" "$C_OFF" "$*" >&2; exit 1; }

# ─── Encerramento ─────────────────────────────────────────────────────

# Mata um processo com carinho e depois na marra. Silencioso se já morreu.
stop_one() {
    local pid="$1" name="$2"
    [[ -z "$pid" ]] && return 0
    kill -0 "$pid" 2>/dev/null || return 0
    kill -TERM "$pid" 2>/dev/null
    for _ in $(seq 1 20); do        # até ~2s pra sair sozinho
        kill -0 "$pid" 2>/dev/null || { ok "$name encerrou"; return 0; }
        sleep 0.1
    done
    warn "$name não saiu no TERM, mandando KILL"
    kill -KILL "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null
    ok "$name encerrou (KILL)"
}

# Ordem inversa do start: clientes primeiro, compositor por último. Se o
# compositor for antes, os dois clientes morrem de Broken pipe e o log fica
# com erro que não é erro nenhum.
shutdown_all() {
    [[ "$SHUTTING_DOWN" == 1 ]] && return 0
    SHUTTING_DOWN=1
    echo
    say "encerrando"
    stop_one "$MENU_PID" "luma-menu"
    stop_one "$BAR_PID"  "luma-bar"
    stop_one "$COMPOSITOR_PID" "luma (compositor)"

    local vivos=0
    for pid in "$MENU_PID" "$BAR_PID" "$COMPOSITOR_PID"; do
        [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null && vivos=$((vivos + 1))
    done
    if [[ "$vivos" == 0 ]]; then
        ok "os três encerrados, nenhum órfão"
    else
        warn "$vivos processo(s) resistiram — confira com: pgrep -a 'luma'"
    fi
}

# Sai direto no sinal: sem o `exit`, o bash retomaria o loop de vigia depois
# do trap e ele anunciaria "o compositor encerrou" — uma queda que fomos nós
# que causamos. O trap de EXIT reentra em shutdown_all, mas ela é idempotente.
trap 'shutdown_all; exit 0' INT TERM
trap shutdown_all EXIT

# ─── Pré-condições ────────────────────────────────────────────────────

# O compositor roda ANINHADO (backend winit), então precisa da sessão do host.
[[ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]] \
    || die "sem WAYLAND_DISPLAY nem DISPLAY — rode de dentro de uma sessão gráfica."

for bin in luma luma-bar luma-menu; do
    [[ -x "$BIN_DIR/$bin" ]] \
        || die "falta $BIN_DIR/$bin — rode antes:  cargo build --workspace"
done

mkdir -p "$LOG_DIR" || die "não consegui criar $LOG_DIR"

RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

# ─── 1. Compositor ────────────────────────────────────────────────────

COMP_LOG="$LOG_DIR/luma.log"
BAR_LOG="$LOG_DIR/luma-bar.log"
MENU_LOG="$LOG_DIR/luma-menu.log"

# Zera ANTES de subir — se sobrar linha "socket wayland:" de uma execução
# velha, a gente leria o socket errado. Truncar depois seria pior: o processo
# segura o offset e o arquivo vira esparso (o grep passa a tratar como binário).
: > "$COMP_LOG"; : > "$BAR_LOG"; : > "$MENU_LOG"

say "subindo o compositor  ${C_DIM}($COMP_LOG)${C_OFF}"
RUST_LOG="${RUST_LOG:-info}" "$BIN_DIR/luma" > "$COMP_LOG" 2>&1 &
COMPOSITOR_PID=$!

# ─── 2. Descobrir o socket ────────────────────────────────────────────

# O `tracing` escreve ANSI mesmo redirecionado, daí o sed antes do match.
WL_DISPLAY=""
for _ in $(seq 1 $((STARTUP_TIMEOUT * 10))); do
    if ! kill -0 "$COMPOSITOR_PID" 2>/dev/null; then
        echo; sed 's/\x1b\[[0-9;]*m//g' "$COMP_LOG" | tail -20
        die "o compositor morreu no boot (log acima, íntegro em $COMP_LOG)"
    fi
    WL_DISPLAY=$(sed 's/\x1b\[[0-9;]*m//g' "$COMP_LOG" \
                 | grep -aoE 'socket wayland: [^ ]+' \
                 | head -1 | awk '{print $3}')
    # Só aceita quando o socket existe de verdade — o log diz a intenção,
    # o arquivo prova que dá pra conectar.
    [[ -n "$WL_DISPLAY" && -S "$RUNTIME_DIR/$WL_DISPLAY" ]] && break
    WL_DISPLAY=""
    sleep 0.1
done

[[ -n "$WL_DISPLAY" ]] \
    || die "o compositor não anunciou socket em ${STARTUP_TIMEOUT}s — veja $COMP_LOG"

ok "compositor no ar  ${C_DIM}(PID $COMPOSITOR_PID · WAYLAND_DISPLAY=$WL_DISPLAY)${C_OFF}"

# ─── 3. Clientes ──────────────────────────────────────────────────────

# Daqui pra baixo, WAYLAND_DISPLAY aponta pro compositor aninhado — nunca
# pro host. É o passo que a gente errava na mão.
say "subindo a barra       ${C_DIM}($BAR_LOG)${C_OFF}"
WAYLAND_DISPLAY="$WL_DISPLAY" "$BIN_DIR/luma-bar" > "$BAR_LOG" 2>&1 &
BAR_PID=$!

say "subindo o menu        ${C_DIM}($MENU_LOG)${C_OFF}"
WAYLAND_DISPLAY="$WL_DISPLAY" "$BIN_DIR/luma-menu" > "$MENU_LOG" 2>&1 &
MENU_PID=$!

sleep 2
for spec in "$BAR_PID:luma-bar:$BAR_LOG" "$MENU_PID:luma-menu:$MENU_LOG"; do
    IFS=: read -r pid name log <<< "$spec"
    if kill -0 "$pid" 2>/dev/null; then
        ok "$name no ar  ${C_DIM}(PID $pid)${C_OFF}"
    else
        warn "$name caiu logo no start — últimas linhas de $log:"
        sed 's/\x1b\[[0-9;]*m//g' "$log" | grep -avE 'libEGL|MESA-LOADER' | tail -5 | sed 's/^/       /'
    fi
done

echo
say "pronto. Ctrl+C derruba os três."
printf '%s    logs: %s/{luma,luma-bar,luma-menu}.log%s\n' "$C_DIM" "$LOG_DIR" "$C_OFF"
echo

# ─── 4. Vigia ─────────────────────────────────────────────────────────

# Espera QUALQUER um sair. O caso comum é fechar a janela do compositor:
# aí os clientes caem sozinhos de Broken pipe, e sem esse aviso parece bug.
while :; do
    # Cinto e suspensório: se um sinal chegou no meio do sleep, não avisa de
    # queda que foi a gente que provocou.
    [[ "$SHUTTING_DOWN" == 1 ]] && break
    if ! kill -0 "$COMPOSITOR_PID" 2>/dev/null; then
        echo
        warn "o compositor encerrou (janela fechada?) — a barra e o menu vão cair junto."
        warn "o 'Broken pipe' que aparecer nos logs deles é isso, não é falha."
        break
    fi
    if [[ -n "$BAR_PID" ]] && ! kill -0 "$BAR_PID" 2>/dev/null; then
        echo; warn "a luma-bar encerrou sozinha — veja $BAR_LOG"; BAR_PID=""
    fi
    if [[ -n "$MENU_PID" ]] && ! kill -0 "$MENU_PID" 2>/dev/null; then
        echo; warn "o luma-menu encerrou sozinho — veja $MENU_LOG"; MENU_PID=""
    fi
    sleep 0.5
done

shutdown_all
