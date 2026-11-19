#!/bin/sh
# A stand-in for the Herdr window's `herdr-app`, beside fake-hc.sh: every call
# goes to $FAKE_HC_STATE/app.log. With $FAKE_HC_STATE/app_closed present the
# window is not open and every `do` fails.
set -u
S="${FAKE_HC_STATE:?FAKE_HC_STATE is not set}"
mkdir -p "$S"
printf '%s\n' "$*" >> "$S/app.log"
case "${1:-}" in
do)
    if [ -f "$S/app_closed" ]; then
        printf '{ "error": "no Herdr window is open" }\n'
        exit 1
    fi
    printf '{ "ok": true }\n'
    ;;
commands) printf '[]\n' ;;
*) exit 2 ;;
esac
