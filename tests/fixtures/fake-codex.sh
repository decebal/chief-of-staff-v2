#!/bin/sh
# Stands in for `codex exec --json … -- <prompt>`: one turn that runs a
# command, edits a file and answers.
echo '{"type":"thread.started","thread_id":"thread-9"}'
echo '{"type":"turn.started"}'
echo '{"type":"item.started","item":{"id":"i1","type":"command_execution","command":"bash -lc ls","status":"in_progress"}}'
echo hello > made-by-agent.txt
echo '{"type":"item.completed","item":{"id":"i2","type":"file_change","changes":[{"path":"made-by-agent.txt","kind":"add"}],"status":"completed"}}'
echo '{"type":"item.completed","item":{"id":"i3","type":"agent_message","text":"Added the file."}}'
echo '{"type":"turn.completed","usage":{"input_tokens":1200,"cached_input_tokens":0,"output_tokens":300}}'
