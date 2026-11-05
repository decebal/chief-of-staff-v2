#!/bin/sh
# Stands in for `claude -p <prompt> --output-format stream-json`. The prompt
# picks the behaviour: SLEEP blocks (its sleeper's pid lands in ./sleeper.pid),
# FAIL ends in an error result, LOOP repeats its first call five more times
# and then succeeds, anything else edits a file and succeeds. REVIEW also
# writes REVIEW.md; TESTPLAN writes TEST-PLAN.md; READONLY skips the edit.
#
# A brain run (cwd = the cos home) is driven by ./fake-mode instead, and
# appends one line per invocation to ./invocations:
#   ok             a result with a minimal brief as structured_output
#   once:<msg>     the first call fails with <msg>, later calls succeed
#   always:<msg>   every call fails with <msg>
#   budget         a budget stop
# When ./caffeinate.pid is expected (./fake-hold), the run waits for it and
# writes ./held if that process is alive while the run is.
prompt="$2"
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
echo '{"type":"system","subtype":"init","session_id":"sess-1"}'
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"git status --short"}}]}}'
echo '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"clean"}]}}'
case "$prompt" in
  *SLEEP*)
    /bin/sleep 60 &
    echo $! > sleeper.pid
    wait
    ;;
  *FAIL*)
    echo '{"type":"result","subtype":"error_max_budget_usd","is_error":true,"errors":["budget spent"],"total_cost_usd":0.5}'
    exit 1
    ;;
  *LOOP*)
    for _ in 1 2 3 4 5; do
      echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"git status --short"}}]}}'
    done
    ;;
esac
case "$prompt" in *REVIEW*) echo "README.md:1 says demo" > REVIEW.md ;; esac
case "$prompt" in
  *TESTPLAN*)
    printf '# Test plan\n\nRun: ./smoke.sh against staging.\n\n## Evidence\n\n- [ ] the run log\n' > TEST-PLAN.md
    echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Write","input":{"file_path":"TEST-PLAN.md","content":"..."}}]}}'
    ;;
esac
case "$prompt" in *READONLY*) ;; *) echo hello > made-by-agent.txt ;; esac
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"All done."}]}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"Changed one file.","total_cost_usd":0.12,"num_turns":3}'
