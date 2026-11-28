---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: findings
---

1. blocking Semantic ISA / Side-effect inventory: writes to `state.json` (new `skills_bundled`, §4 step 3) and to `spec.json` (new `skills`, §3) are missing from E1-E6. Evidence: design line 179 (`Record the bundle in state.json`); `src/agents/mod.rs:1228` `write_state` writes `state.json`; `src/agents/mod.rs:580` `write_json(&d.join("spec.json"), &spec)`. Fix: add rows for both writes (file, trigger, reversibility: removed by `cos agents clean`, `mod.rs:1224`), and a test-matrix or acceptance row that checks both contain the skill set.

2. blocking Semantic ISA / Side-effect inventory: VERIFY-U says "`host` resolves only to loopback addresses", which implies a DNS lookup for any non-literal host (for example `localhost`, `example.com` in the S1 row). That lookup is a network call not in E1-E6. Evidence: design line 263-ish VERIFY-U text (§ VERIFY before every sink) and S1 row line 299 (`http://example.com/mcp` must "open no socket"). Fix: either accept IP literals only (`IpAddr` parse, no DNS, which matches §1 "refuses any host that is not a loopback address"), and state that, or add E7 "DNS lookup" with its trigger and a test row.

3. blocking §4 Delivery step 2: `cp -Rc <src> out/agents/<id>/skills/skills/<name>` cannot run as written. `cp` does not create the destination parent, and step 1 only creates `skills/.claude-plugin/`, so `skills/skills/` is absent. Evidence: `src/agents/seed.rs:148-150` `clone_cmd` is `cp -Rc from to` with no mkdir; design line 172-173. Fix: step 1 creates `skills/skills/` as well; step 2 also states that an existing destination is removed first (a relaunch would otherwise nest `<name>/<name>`).

4. blocking Semantic ISA / VERIFY-B (S3): `cp -Rc` on macOS does not follow symlinks inside the source tree (`-R` without `-L`), so a symlink inside a skill directory is copied as a symlink that still points at its original target outside the bundle. The design's `~/.agents/skills` entries are symlinks into project checkouts (design line 51), and VERIFY-B checks only the top-level `SKILL.md` digest. Evidence: design line 51 and 174; `src/agents/seed.rs:148-150`. Fix: pass `-L` (or reject any symlink under the source with a named warning) in the bundle command, and add an S3 row: a skill dir holding `scripts/x -> /etc/hosts` must not produce a symlink in the bundle.

5. should §4 `harness::argv`: `--plugin-dir` is "when anything was bundled", but `argv(spec: &Spec)` (`src/agents/harness.rs:16`) receives only `Spec`, and `launch` calls it as `harness::argv(&spec)` (`src/agents/mod.rs:624`). The bundled-or-not fact lives in `State.skills_bundled`. Fix: state the new signature, e.g. `argv(spec, bundled: bool)`, or move `skills_bundled` onto `Spec` before `spec.json` is written.

6. should §2 Terms / §5 config: `context/agents.json` gains top-level `skills.roots` and `skills.unreviewed`, but `Policy` is `#[serde(deny_unknown_fields)]` (`src/agents/policy.rs:109`), so the existing loader refuses the file once the key is present. The design does not add the fields to `Policy`. Fix: name the `Policy` struct change (a `skills` struct with `roots` and `unreviewed`, both optional) and a test that the policy still loads with and without the key.

7. should §2 Read: fold order is ambiguous. "fold to the graph, drop and warn on any fact failing VERIFY-R" reads as drop before fold, which lets an older valid fact win. The Violation table (line 290) says the skill "reads as unreviewed", which requires dropping after fold. Fix: pick one order and state it; add a test row where the newest fact is invalid and an older valid one exists.

8. should §1 vs VERIFY-U: §1 says the client "refuses any host that is not a loopback address" (literal check), VERIFY-U says "host resolves only to loopback" (resolution check). Brief line 67 says "refuses any host but 127.0.0.1 / ::1". Fix: make one rule (literal IP only, see finding 2) and use it in both places.

9. should §2 Write: `prime_add_edge` needs `source` and `target` as entity ids in the form `node:{type}:{id}` (tool schema of `mcp__prime__prime_add_edge`: "Source node entity_id (format: node:{type}:{id})"). The design does not say how the new node's id is obtained from `prime_add_node`, so "an edge failure is a warning" covers a step that may never be wired. Fix: state where the entity id comes from (the `prime_add_node` result) and add an acceptance row for it, or drop edges from the design since they are "never read back".

10. should §3 / Acceptance 1-2: `--dry-run` today must call nothing (`CLAUDE.md` "`cos --dry-run brief` print the prompt, call nothing"). With this design `plan()` makes a Prime call and writes `out/skills/graph.json` (E1, E3), and the dialog's `agents_fit` does so on every debounced check. The design never says whether `--dry-run` is exempt. Fix: state it. Either dry-run reads only the cache and says so in the plan, or the exception is named in the Semantic ISA section.

11. should §4 / Acceptance 4: the recorded digest is the sha256 of `SKILL.md` only (`VERIFY-B`, S3 row). Scripts and references beside it are copied unhashed, but the run record reads as if the whole bundle is pinned ("each skill's sha256"). Fix: define the digest as `SKILL.md` only in §1 Terms and in Acceptance 4, or hash the whole tree.

12. should Scope gap: a repo's own `.claude/skills` (in the worktree) loads in every hire on that repo. It is outside the catalog and outside `hidden`, and the design says `skillOverrides` covers "user or project skill" (line 57-59) but never uses it for project skills. The brief's problem statement names repo-kept skills (brief line 19-20). Fix: state whether repo skills are in scope; if not, say so as a plan warning, as done for plugins.

13. should §5 / Acceptance 7-8: behaviour when Prime is down for a write (`cos skills set`, `curate`, `skills_set`) is not specified: fail with exit 1 (the VERIFY-W row says only validation failures), or queue. Fix: add a violation row, "Prime unreachable on write: exit 1 naming the URL, nothing local written", and an acceptance row for it.

14. should §4 idempotency: `cp -Rc` into an existing destination nests the source directory. Retried `launch` runs `prepare` again (`src/agents/mod.rs:608`). Fix: remove the destination before copy (covered with finding 3).

15. nit §Context: stale line refs. `cos agents spawn` is `src/main.rs:921` (`AgentCmd::Spawn`), not 987 (`main.rs:921` per `rg`); `spin_off` is `src/mcp.rs:242`, not 295. Evidence: rg output above. Fix: update lines 26-27.

16. nit §4 Codex: "`workspace-write` lets it read outside the worktree" is not verified against any source in the repo (`src/agents/harness.rs:28` only passes the flag). Fix: mark as assumption A2b or drop the sentence.

17. nit Assumptions A4 and Context: "754 nodes", "37 `execution_role` snapshots", "`prime_search` returns every node of a type in one response" were not checked by me (Prime is out of scope for this review). Only the tool schema was checked: `prime_search` takes `type` only, and its description says it "find[s] all nodes of a given type". Fix: keep, but mark as the author's measurement, not re-verified.

Summary: blocking 4, should 10, nit 3. Verified OK: `hire.rs:33` Plan, `hire.rs:63` plan, `hire.rs:181` request, `harness.rs:13` CLAUDE_SANDBOX, `harness.rs:16` argv, `mod.rs:509` record_run, `mod.rs:602` launch, `mod.rs:683` prepare, `mod.rs:1187` clean, `judge.rs:246-254` untracked count, `app.js:1215-1217` 150 ms debounce, `seed.rs:22` 300 s step deadline, `proc.rs:105` run owns and reaps, `tests/design.rs:1456` and `:1872` gate names, `apps/desktop/src/lib.rs:1382/1433` plan and agents_fit, `Cargo.toml:33` serde_json.
