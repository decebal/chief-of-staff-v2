#!/bin/sh
# Stands in for chronis (`cn`) over a store that changes, in the current
# directory: `list` prints .chronis/tasks.json; `task edit <id>
# --append-description <line>` and `approve <id>` rewrite it. Holds one task,
# written by the test on one line with `"approved":null`.
s=.chronis/tasks.json
case "$1" in
  list) cat "$s"; exit 0 ;;
  task)
    sed "s|\"description\":\"\\([^\"]*\\)\"|\"description\":\"\\1\\\\n$5\"|" "$s" > "$s.new" || exit 1
    mv "$s.new" "$s" ;;
  approve)
    sed 's/"approved":null/"approved":true/' "$s" > "$s.new" || exit 1
    mv "$s.new" "$s" ;;
  *) echo "unexpected: $*" >&2; exit 2 ;;
esac
echo "$*" >> .chronis/calls.log
