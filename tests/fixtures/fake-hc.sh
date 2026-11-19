#!/bin/sh
# A stand-in for herdr-core's `hc`, driven by the directory in $FAKE_HC_STATE.
# Never touches a real core. Each terminal is terms/<address>/, one file per
# fact (name, cwd, labels, argv as NUL-separated words, state, status, kind,
# turn, exit_code, reason, first_message as JSON, screen, sent). Every call is
# appended to calls.log, one line, words separated by " | ".
#
#   down              server status fails, as with no core running
#   run_fails         `hc run` fails to spawn
#   first_message     JSON copied onto each new terminal's first_message
#   $FAKE_HC_EXEC=1   `hc run` runs the program to its end, in its cwd: exit 0
#                     leaves the terminal live, status done, turn 1 (an agent
#                     that finished its turn); any other exit finishes it
set -u
S="${FAKE_HC_STATE:?FAKE_HC_STATE is not set}"
mkdir -p "$S/terms"
{
    sep=""
    for a in "$@"; do
        printf '%s%s' "$sep" "$(printf '%s' "$a" | tr '\n' ' ')"
        sep=" | "
    done
    printf '\n'
} >> "$S/calls.log"

str() {
    printf '"%s"' "$(printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | tr '\n' ' ')"
}

opt() {
    if [ -s "$1" ]; then str "$(cat "$1")"; else printf 'null'; fi
}

num() {
    if [ -s "$1" ]; then cat "$1"; else printf '%s' "$2"; fi
}

term_json() {
    d="$S/terms/$1"
    printf '{"address":"%s","name":%s,"cwd":%s,"labels":{' "$1" "$(opt "$d/name")" "$(opt "$d/cwd")"
    first=1
    if [ -f "$d/labels" ]; then
        while IFS= read -r kv; do
            [ -z "$kv" ] && continue
            [ $first = 1 ] || printf ','
            first=0
            printf '%s:%s' "$(str "${kv%%=*}")" "$(str "${kv#*=}")"
        done < "$d/labels"
    fi
    printf '},"state":"%s","status":' "$(cat "$d/state")"
    if [ -s "$d/status" ]; then
        printf '{"app":"fake","state":"%s"' "$(cat "$d/status")"
        [ -s "$d/kind" ] && printf ',"kind":"%s"' "$(cat "$d/kind")"
        printf '}'
    else
        printf 'null'
    fi
    printf ',"turn":%s,"exit_code":%s,"reason":%s' "$(num "$d/turn" 0)" "$(num "$d/exit_code" null)" "$(opt "$d/reason")"
    if [ -s "$d/first_message" ]; then printf ',"first_message":%s' "$(cat "$d/first_message")"; fi
    printf ',"argv":[],"created_ms":0,"screen":"exact","cols":200,"rows":120}\n'
}

err() {
    printf '{"error":{"kind":"%s","message":"%s","hint":""}}\n' "$1" "$2" >&2
    exit "$3"
}

find_term() {
    if [ -d "$S/terms/$1" ]; then printf '%s' "$1"; return 0; fi
    for d in "$S"/terms/*; do
        [ -d "$d" ] || continue
        if [ "$(cat "$d/name" 2>/dev/null)" = "$1" ] && [ "$(cat "$d/state")" = live ]; then
            basename "$d"
            return 0
        fi
    done
    return 1
}

cmd="${1:-}"
[ $# -gt 0 ] && shift
if [ -f "$S/down" ] && [ "$cmd $*" != "server start" ]; then
    err unavailable "no herdr-core is running" 1
fi
case "$cmd" in
server)
    case "${1:-}" in
    status)
        live=0
        for d in "$S"/terms/*; do
            [ -d "$d" ] && [ "$(cat "$d/state")" = live ] && live=$((live + 1))
        done
        printf '{"version":"0.1.21-fake","live":%s}\n' "$live"
        ;;
    start) rm -f "$S/down" ;;
    *) err invalid_params "server what?" 2 ;;
    esac
    ;;
run)
    name="" cwd="$PWD"
    labels=""
    while [ $# -gt 0 ]; do
        case "$1" in
        --) shift; break ;;
        --name) name="$2"; shift 2 ;;
        --cwd) cwd="$2"; shift 2 ;;
        --label) labels="$labels$2
"; shift 2 ;;
        --*) shift ;;
        *) break ;;
        esac
    done
    [ -f "$S/run_fails" ] && err spawn_failed "the program could not start" 1
    if [ -n "$name" ] && find_term "$name" > /dev/null; then err name_taken "$name is taken" 1; fi
    n=$(cat "$S/next" 2>/dev/null || echo 100)
    echo $((n + 1)) > "$S/next"
    a="t$n"
    d="$S/terms/$a"
    mkdir -p "$d"
    printf '%s' "$name" > "$d/name"
    printf '%s' "$cwd" > "$d/cwd"
    printf '%s' "$labels" > "$d/labels"
    printf '%s\0' "$@" > "$d/argv"
    echo live > "$d/state"
    echo working > "$d/status"
    echo 0 > "$d/turn"
    [ -f "$S/first_message" ] && cp "$S/first_message" "$d/first_message"
    if [ "${FAKE_HC_EXEC:-}" = 1 ]; then
        if (cd "$cwd" && "$@") > "$d/screen" 2>&1 < /dev/null; then
            echo done > "$d/status"
            echo 1 > "$d/turn"
        else
            echo $? > "$d/exit_code"
            echo finished > "$d/state"
            echo exited > "$d/reason"
            : > "$d/status"
        fi
    fi
    term_json "$a"
    ;;
get)
    a=$(find_term "$1") || err not_found "no terminal $1" 3
    term_json "$a"
    ;;
ls)
    printf '{"terminals":['
    first=1
    for d in "$S"/terms/*; do
        [ -d "$d" ] || continue
        [ $first = 1 ] || printf ','
        first=0
        printf '{"address":"%s","state":"%s","status":"%s","turn":%s}' "$(basename "$d")" "$(cat "$d/state")" "$(cat "$d/status")" "$(num "$d/turn" 0)"
    done
    printf ']}\n'
    ;;
send)
    a=$(find_term "$1") || err not_found "no terminal $1" 3
    d="$S/terms/$a"
    [ "$(cat "$d/state")" = live ] || err finished "it has finished" 11
    [ "$(cat "$d/status")" = blocked ] && err blocked "it is waiting on a person" 9
    printf '%s\0' "$2" >> "$d/sent"
    echo working > "$d/status"
    ;;
interrupt)
    a=$(find_term "$1") || err not_found "no terminal $1" 3
    d="$S/terms/$a"
    [ "$(cat "$d/state")" = live ] || err finished "it has finished" 11
    echo idle > "$d/status"
    ;;
kill)
    a=$(find_term "$1") || err not_found "no terminal $1" 3
    d="$S/terms/$a"
    [ "$(cat "$d/state")" = live ] || err finished "it has finished" 11
    echo finished > "$d/state"
    echo killed > "$d/reason"
    : > "$d/status"
    : > "$d/kind"
    ;;
rm)
    a=$(find_term "$1") || err not_found "no terminal $1" 3
    [ "$(cat "$S/terms/$a/state")" = live ] && err live "it is still running" 1
    rm -rf "${S:?}/terms/$a"
    ;;
read)
    a=$(find_term "$1") || err not_found "no terminal $1" 3
    if [ -s "$S/terms/$a/screen" ]; then cat "$S/terms/$a/screen"; else printf 'the screen of %s\n' "$(cat "$S/terms/$a/name")"; fi
    ;;
*)
    err invalid_params "fake hc does not know $cmd" 2
    ;;
esac
