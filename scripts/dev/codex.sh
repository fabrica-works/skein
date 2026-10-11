# shellcheck shell=bash
# The Codex app-server daemon, as plan reaches it (development.md, sections
# 6, 7.4 and 7.6). Sourced by plan, not run.
#
#   codex_open             connect to the daemon and initialize
#   codex_call METHOD JSON print the result of one request; fail, printing
#                          its error, if the daemon refuses it
#   codex_close            disconnect
#
# The daemon's control socket speaks WebSocket; `codex app-server proxy`
# carries its bytes over stdio. This client sends text frames masked with a
# zero key, which leaves the payload as is, answers pings, and skips the
# notifications that arrive between a request and its result.

codex_id=0

codex_open() {
    coproc CODEX_PROXY { exec codex app-server proxy 2>/dev/null; }
    exec 3>&"${CODEX_PROXY[1]}" 4<&"${CODEX_PROXY[0]}"
    printf 'GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: %s\r\nSec-WebSocket-Version: 13\r\n\r\n' \
        "$(head -c 16 /dev/urandom | base64)" >&3
    local status line
    IFS= read -r -t 10 status <&4 || { echo "codex: the daemon does not answer (codex app-server daemon start)" >&2; return 1; }
    [[ $status == "HTTP/1.1 101"* ]] || { echo "codex: the daemon refused the connection: $status" >&2; return 1; }
    while IFS= read -r -t 10 line <&4 && [[ $line != $'\r' ]]; do :; done
    codex_call initialize '{"clientInfo":{"name":"plan","version":"0"}}' >/dev/null
    ws_send '{"method":"initialized"}'
}

codex_close() {
    exec 3>&- 4<&-
}

codex_call() {
    local method=$1 params=$2 msg
    codex_id=$((codex_id + 1))
    ws_send "$(jq -cn --argjson id "$codex_id" --arg m "$method" --argjson p "$params" \
        '{id: $id, method: $m, params: $p}')"
    while msg=$(ws_recv); do
        jq -e --argjson id "$codex_id" '.id == $id' <<<"$msg" >/dev/null 2>&1 || continue
        if jq -e 'has("error")' <<<"$msg" >/dev/null; then
            jq -c '.error' <<<"$msg" >&2
            return 1
        fi
        jq -c '.result' <<<"$msg"
        return 0
    done
    echo "codex: the daemon closed the connection during $method" >&2
    return 1
}

# The frame's header: FIN and opcode, the masked length, a zero mask.
# shellcheck disable=SC2059 # each format is the byte it writes
ws_header() {
    local op=$1 len=$2 shift
    printf "\\x$(printf %02x "$op")"
    if ((len < 126)); then
        printf "\\x$(printf %02x $((128 | len)))"
    elif ((len < 65536)); then
        printf '\xfe'
        printf "\\x$(printf %02x $((len >> 8)))\\x$(printf %02x $((len & 255)))"
    else
        printf '\xff'
        for shift in 56 48 40 32 24 16 8 0; do
            printf "\\x$(printf %02x $(((len >> shift) & 255)))"
        done
    fi
    printf '\x00\x00\x00\x00'
}

ws_send() {
    local text=$1 op=${2:-129} len
    len=$(printf '%s' "$text" | wc -c)
    { ws_header "$op" "$len"; printf '%s' "$text"; } >&3
}

ws_bytes() { dd bs="$1" count=1 iflag=fullblock status=none <&4; }

ws_number() {
    local n=0 x
    for x in $(ws_bytes "$1" | od -An -tu1); do n=$((n * 256 + x)); done
    printf '%s' "$n"
}

# Prints the next text message on one line; answers pings; fails when the
# daemon closes the connection.
ws_recv() {
    local b0 b1 op len payload
    while :; do
        read -r b0 b1 < <(ws_bytes 2 | od -An -tu1) || return 1
        [[ -n ${b1:-} ]] || return 1
        op=$((b0 & 15))
        len=$((b1 & 127))
        if ((len == 126)); then
            len=$(ws_number 2)
        elif ((len == 127)); then
            len=$(ws_number 8)
        fi
        payload=
        ((len == 0)) || payload=$(ws_bytes "$len")
        case $op in
        1) printf '%s\n' "$payload"; return 0 ;;
        8) return 1 ;;
        9) ws_send "$payload" 138 ;;
        *) ;;
        esac
    done
}
