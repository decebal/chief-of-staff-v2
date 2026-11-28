#!/bin/sh
# Stands in for `claude -p <prompt> --output-format stream-json` in a brain
# run (cwd = the cos home, or $FAKE_CLAUDE_DIR when set), driven by
# ./fake-mode, appending one line per invocation to ./invocations:
#   ok             a result with a minimal brief as structured_output
#   once:<msg>     the first call fails with <msg>, later calls succeed
#   always:<msg>   every call fails with <msg>
#   budget         a budget stop
#   linger         as ok, leaving a member of its group running after it exits
#   hang           never answers
# linger and hang write the group id ($$) to ./claude.pgid. Every call
# writes its argv to ./last-args and its COS_* environment to ./child-env.
# When ./caffeinate.pid is expected (./fake-hold), the run waits for it and
# writes ./held if that process is alive while the run is.
if [ -n "$FAKE_CLAUDE_DIR" ]; then
  cd "$FAKE_CLAUDE_DIR" || exit 2
fi
if [ -f fake-mode ]; then
  mode=$(cat fake-mode)
  echo run >> invocations
  printf '%s\n' "$*" > last-args
  env | grep '^COS_' > child-env
  if [ -f fake-hold ]; then
    i=0
    while [ ! -s caffeinate.pid ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
    kill -0 "$(cat caffeinate.pid)" 2>/dev/null && echo yes > held
  fi
  n=$(wc -l < invocations | tr -d ' ')
  echo '{"type":"system","subtype":"init","session_id":"sess-1"}'
  fail() {
    printf '{"type":"result","subtype":"success","is_error":true,"result":"%s","total_cost_usd":0.25}\n' "$1"
    exit 1
  }
  case "$mode" in
    budget)
      echo '{"type":"result","subtype":"error_max_budget_usd","is_error":true,"errors":["Reached maximum budget ($3)"],"total_cost_usd":3.0}'
      exit 1
      ;;
    once:*) [ "$n" = 1 ] && fail "${mode#once:}" ;;
    always:*) fail "${mode#always:}" ;;
    linger)
      sleep 60 </dev/null >/dev/null 2>&1 &
      echo $$ > claude.pgid
      ;;
    hang)
      echo $$ > claude.pgid
      exec sleep 60
      ;;
  esac
  echo '{"type":"result","subtype":"success","is_error":false,"result":"Answered.","structured_output":{"headline":"Fake brief"},"total_cost_usd":0.5,"num_turns":2}'
  exit 0
fi
echo "fake-claude: no ./fake-mode here" >&2
exit 2
