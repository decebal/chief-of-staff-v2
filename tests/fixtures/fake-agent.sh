#!/bin/sh
# A stand-in agent for runs under fake-hc.sh with FAKE_HC_EXEC=1. Its first
# argument is the hire's prompt, as cos puts it on Claude's command line:
# FAIL exits 1; REVIEW writes REVIEW.md; TESTPLAN writes TEST-PLAN.md with its
# evidence list; READONLY changes nothing else; otherwise it writes
# made-by-agent.txt.
prompt="${1:-}"
case "$prompt" in *FAIL*) echo "it could not do it"; exit 1 ;; esac
case "$prompt" in *REVIEW*) echo "a review" > REVIEW.md ;; esac
case "$prompt" in *TESTPLAN*) printf '# Test plan\n\n## Evidence\n\n- logs\n' > TEST-PLAN.md ;; esac
case "$prompt" in *READONLY*) ;; *) echo "made by the agent" > made-by-agent.txt ;; esac
echo "done: $COS_RUN_ID"
