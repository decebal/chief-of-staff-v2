#!/bin/sh
# Stands in for `claude -p <prompt> --output-format stream-json`. The prompt
# picks the behaviour: SLEEP blocks (its sleeper's pid lands in ./sleeper.pid),
# FAIL ends in an error result, LOOP repeats its first call five more times
# and then succeeds, anything else edits a file and succeeds.
prompt="$2"
echo '{"type":"system","subtype":"init","session_id":"sess-1"}'
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}'
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
      echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}'
    done
    ;;
esac
echo hello > made-by-agent.txt
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"All done."}]}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"Changed one file.","total_cost_usd":0.12,"num_turns":3}'
