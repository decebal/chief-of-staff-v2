#!/bin/sh
# Stands in for `claude -p <prompt> --output-format stream-json` in a brain
# run (cwd = the cos home), driven by ./fake-mode, appending one line per
# invocation to ./invocations:
#   ok             a result with a minimal brief as structured_output
#   once:<msg>     the first call fails with <msg>, later calls succeed
#   always:<msg>   every call fails with <msg>
#   budget         a budget stop
# When ./caffeinate.pid is expected (./fake-hold), the run waits for it and
# writes ./held if that process is alive while the run is.
if [ -f fake-mode ]; then
  mode=$(cat fake-mode)
  echo run >> invocations
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
  esac
  echo '{"type":"result","subtype":"success","is_error":false,"result":"Answered.","structured_output":{"headline":"Fake brief"},"total_cost_usd":0.5,"num_turns":2}'
  exit 0
fi
echo "fake-claude: no ./fake-mode here" >&2
exit 2
