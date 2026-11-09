#!/bin/sh
# Stands in for chronis (`cn`) in the current directory: `list` prints
# .chronis/tasks.json, writes are appended to .chronis/calls.log, and a repo
# with .chronis/fail makes every call fail.
if [ -f .chronis/fail ]; then
  echo "store is locked" >&2
  exit 1
fi
case "$1" in
  list) cat .chronis/tasks.json ;;
  *) echo "$*" >> .chronis/calls.log ;;
esac
