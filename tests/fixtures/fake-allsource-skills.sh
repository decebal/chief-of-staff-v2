#!/bin/sh
# A stand-in for allsource-skills, copied into each test's own directory. It
# reads its behaviour from the files beside it and logs every call to
# calls.log. Never the real tool.
#
#   digest        the 64-hex digest it reports (default: all zeros)
#   skills        the names a role reads, and the export install writes
#   roles_mode    auto (fresh when the export exists, else stale), stale,
#                 garbage, path, nonhex, fail, sleep
#   install_mode  ok (default), fail, sleep
D=$(dirname "$0")
DIG=$(cat "$D/digest" 2>/dev/null || echo 0000000000000000000000000000000000000000000000000000000000000000)
echo "$*" >> "$D/calls.log"

cmd="$1"
shift
repo=""
role=""
while [ $# -gt 0 ]; do
    case "$1" in
    --repo) repo="$2"; shift ;;
    --json) ;;
    *) role="$1" ;;
    esac
    shift
done

names() {
    list=""
    sep=""
    for s in $(cat "$D/skills" 2>/dev/null); do
        list="$list$sep\"$s\""
        sep=","
    done
    printf '%s' "$list"
}

case "$cmd" in
roles)
    mode=$(cat "$D/roles_mode" 2>/dev/null || echo auto)
    case "$mode" in
    sleep) sleep 30 ;;
    fail) echo "roles: the store is unreadable" >&2; exit 2 ;;
    garbage) echo "not json"; exit 0 ;;
    path) printf '{"version":1,"state":"fresh","digest":"%s","role":"%s","known":true,"skills":[],"path":"/etc"}\n' "$DIG" "$role"; exit 0 ;;
    nonhex) printf '{"version":1,"state":"fresh","digest":"ABC","role":"%s","known":true,"skills":[]}\n' "$role"; exit 0 ;;
    stale) printf '{"version":1,"state":"stale","digest":"%s"}\n' "$DIG"; exit 3 ;;
    esac
    if [ -d "$repo/.prime/skills/$DIG/export/claude" ]; then
        printf '{"version":1,"state":"fresh","digest":"%s","role":"%s","known":true,"skills":[%s]}\n' "$DIG" "$role" "$(names)"
        exit 0
    fi
    printf '{"version":1,"state":"stale","digest":"%s"}\n' "$DIG"
    exit 3
    ;;
install)
    mode=$(cat "$D/install_mode" 2>/dev/null || echo ok)
    case "$mode" in
    fail) echo "install: the store could not be written" >&2; exit 2 ;;
    sleep) sleep 30 ;;
    esac
    export_dir="$repo/.prime/skills/$DIG/export/claude"
    mkdir -p "$export_dir"
    for s in $(cat "$D/skills" 2>/dev/null); do
        mkdir -p "$export_dir/$s"
        printf -- '---\ndescription: %s, built from the repo\n---\n\nBody.\n' "$s" > "$export_dir/$s/SKILL.md"
    done
    exit 0
    ;;
*)
    echo "unknown command $cmd" >&2
    exit 2
    ;;
esac
