# Role library: design

Date: 2026-11-28. Brief: [2026-11-28-role-library-brief.md](2026-11-28-role-library-brief.md).
Epic: `t-b0b659`. Branch: `feat/agents-role-library`.

Review history: round 1,
[a](2026-11-28-role-library-design-review-r1-a.md) and
[b](2026-11-28-role-library-design-review-r1-b.md), both `findings`, on
`bbfbeb70…304a6f`. Round 2,
[a](2026-11-28-role-library-design-review-r2-a.md) and
[b](2026-11-28-role-library-design-review-r2-b.md), both `findings`, on
`55a0b8fb…6dec61`. Round 3,
[a](2026-11-28-role-library-design-review-r3-a.md) and
[b](2026-11-28-role-library-design-review-r3-b.md), both `findings`, on
`c99f344a…`. Round 4,
[a](2026-11-28-role-library-design-review-r4-a.md) and
[b](2026-11-28-role-library-design-review-r4-b.md), both `findings`, on
`78321b81…ba80`. After round 4 the user chose: contract changes approved only
by a person (a terminal or the window), the guard-process bound for owner
death, the `apply` signature fix, and then approval without a fifth round.
Every "Carried to Plan" item becomes a Plan task. Approved at `98d703e6…`.

After Build, the [code review](2026-11-28-role-library-code-review.md)
found the finish line reaching the prompt uncleaned, the guard's sweep
missing from the ISA, and drift between this text and what was built. This
revision records what was built: §1 caps for `output` and `scope`, §3 empty
list items and `output`, §4 step 6 (the written role must be valid; elsewhere
only an added problem refuses), §6 the guard's poll and sweep and the real
signatures, §7 the guard as the window's child, and the `RENDER(finish_line)`
and `TOOL_CALL(guard_sweep)` rows. Drift reviews:
[r5-a](2026-11-28-role-library-design-review-r5-a.md) and
[r5-b](2026-11-28-role-library-design-review-r5-b.md), both `findings` on
`bcd69825…`, answered here.

## Decision

`cos` ships a library of specialist role definitions as data
(`templates/roles/*.json`, compiled in). A role gains five optional typed
fields: `principles`, `method`, `anti_patterns`, `output_format`,
`escalation`. It also gains `from` for provenance and `retired`. One module,
`src/agents/roles.rs`, owns the library, the validator and the only write to
`context/agents.json`. The write runs under a lock with a digest check and an
atomic replace, and refuses a policy that fails validation.

Making a proposal and writing it are separate steps. `cos roles propose`
runs a headless `claude` with no tools to specialise a library entry and
prints or saves a role proposal, writing nothing to the policy. `cos roles
new` writes a role (from a library entry, a proposal file or blank) on the
user's approval. The desktop Staff tab runs `propose` as a child it owns,
shows the proposal in a form, and saves through the same write path.

Loading never drops a role. Problems are reported per role. The preamble a
hire reads is rendered from sanitised, capped fields.

## Context

- `src/agents/policy.rs`: `Role` (L56), `Policy` (L110), `load` (L137) falls
  back to the template's roles when the file has none and to defaults with a
  single `note: Option<String>` when it does not parse. It fills `fit` from
  the seed when the file has none (L149). `Terms` and `Role` carry
  `budget_usd`, `stall_secs` and `repeat` as `skip_serializing` retired keys
  (L44–49, L100–105). `preamble` (L268) puts title, mandate and the output
  rule in front of a hire's prompt. `Policy::apply` (L220) adds it (L250).
  `Policy::hire` (L182) refuses an unknown role or the head.
- `templates/context/agents.json`: the seed, holding 10 roles with one-line
  mandates, `defaults`, `repos` and the `fit` vocabulary.
- `src/agents/fit.rs`: `Vocab`. `check` matches a role's `work` against the
  task's kinds with `r.work.contains(k)` (L218) and tolerates unknown names.
  `suggest` takes the first non-head role that fits (L245), and `refusal`
  names it (L116).
- `src/agents/hire.rs`: `Plan` (L33) carries `prompt`, the task prompt.
  `cos --dry-run agents spawn` prints the plan as JSON (`src/main.rs:988`).
  The preamble is not in the plan; it is added when the request is recorded
  (`src/agents/mod.rs:513` via `Policy::apply`).
- `src/agents/ledger.rs`: rows are keyed by run, with `role` as a field
  (L39–41). `by_role` groups by it (L186), so a role id must stay stable for
  the scorecard.
- `apps/desktop/src/lib.rs`: `agents_roles` (L1253) is read-only. `named`
  (L279) is the business guard on writes such as `ritual_approve` (L1708).
  `brief_run` (L1582) runs `cos brief` as an owned child through
  `pipeline::brief_child` (`src/pipeline.rs` L686), which uses
  `proc::spawn_grouped`, `wait_until` and `kill_group`. `on_run_event`
  (L2080) stops owned servers on quit. Commands are reachable from the
  `main` window only (`apps/desktop/capabilities/default.json`).
- UI: `apps/desktop/ui/app.js` holds `renderStaff`, `roleCard` (L431–535),
  `renderRole` (L540), `ROLE_ICON` (L409) and the hire dialog's `fillRole`
  (L1152). `apps/desktop/ui/states.js` holds the state kinds (L117–129);
  failures use `unreachable`. `tests/design.rs` keeps a loader list
  (L1531–1543) that must name every function rendering a loading state.
- `src/claude.rs`: `Run`, with `Toolset::None` (L140) giving `--tools ""
  --strict-mcp-config`. No caller uses it today. `execute` (L193) opens the
  audit log `out/audit/<day>.jsonl` and passes the prompt as `-p <prompt>`
  (L121). `src/pipeline.rs` `Bins` (L38) names the `claude` binary; tests
  substitute `tests/fixtures/fake-claude.sh` (`tests/sleep.rs:16`), which
  always returns a brief-shaped `structured_output`.
- `src/state.rs`: `atomic_write_bytes` (L555) uses a temp file named
  `.state-<pid>-<nanos>.tmp`, fsync, rename and dir fsync. Its `Lock` (L597)
  is private.
- Nothing writes `context/agents.json` today. The user edits it by hand.

## Design

### 1. Role schema (policy.rs)

New optional fields on `Role`, all `#[serde(default)]`. Empty lists, `None`
options and `retired: false` are skipped when serialising
(`skip_serializing_if`). Every old file still loads. A file holding the new
fields does not load in an older `cos` (`deny_unknown_fields`): it falls back
to the template roles with a note (`policy.rs:141`) until `cos` is updated.

| Field | Type | Meaning |
|---|---|---|
| `principles` | `Vec<String>` | Standards a top practitioner holds |
| `method` | `Vec<String>` | Ordered steps the role follows |
| `anti_patterns` | `Vec<String>` | What it refuses to do |
| `output_format` | `Option<String>` | Exact shape of what it hands back |
| `escalation` | `Vec<String>` | When it stops and leaves the question for the user |
| `from` | `Option<String>` | `<library-id>@<version>` it was made from |
| `retired` | `bool` | Kept for history, not offered for hire |

**Caps** (`roles::CAPS`, one table used by the validator, the proposal
schema and the renderer):

| Field | Max items | Max chars per item / value |
|---|---|---|
| `title` | — | 60 |
| `mandate` | — | 600 |
| `serves`, `sandbox`, `judged`, `done_when` | — | 300 each |
| `principles` | 8 | 240 |
| `method` | 10 | 240 |
| `anti_patterns` | 8 | 240 |
| `escalation` | 6 | 240 |
| `output_format` | — | 1,200 |
| `output` | — | 200 |
| `scope` | 40 | 200 |
| proposal `rationale` | — | 800 |
| user `brief` to propose | — | 1,000 |

Worst case for the structured sections is 1,920 + 2,400 + 1,920 + 1,440 +
1,200 characters, plus the 600-character mandate: 9,480 characters, about
2,400 tokens per hire. A role may hold any count from 0 to the max. The
library's minimum counts (§2) apply to library entries only.

**Retired.** `Policy::hire` refuses a retired role: "the <title> is retired;
restore it with `cos roles restore <id>`". `fit::check` and `fit::refusal`
skip retired roles when choosing a suggestion, so a retired role is never
suggested.

**Preamble.** `preamble` appends after the mandate, when non-empty:
`Principles:` (bullets), `Method:` (numbered), `Never:` (bullets), `Hand
back:` (output_format), `Stop and leave it for the user when:` (bullets).
The existing output rule and the drafts-only sentence stay last. The
renderer, not the validator, is the boundary at this sink. Every value is
rendered through `roles::clean(s, cap)`, which replaces each run of
whitespace (newlines included) with one space, trims, and truncates to the
cap. Lists are cut to their max items. A hand-edited role therefore cannot
forge a section or exceed the bound, even though load does not refuse it.

### 2. The library (roles.rs, templates/roles/)

One JSON file per entry:

```json
{
  "id": "appsec-reviewer",
  "version": 1,
  "summary": "Reviews a change for exploitable weaknesses against OWASP ASVS 5.0 and CWE Top 25.",
  "sources": [{ "title": "OWASP ASVS 5.0", "url": "https://owasp.org/www-project-application-security-verification-standard/" }],
  "fit_kinds": { "security": ["security review", "threat model", "vulnerability", "appsec"] },
  "role": { "id": "appsec-reviewer", "title": "Application security reviewer", "mandate": "…", "principles": ["…"], "method": ["…"], "anti_patterns": ["…"], "output_format": "…", "escalation": ["…"], "expects": "file", "output": "SECURITY-REVIEW.md", "checks": ["test -s SECURITY-REVIEW.md"], "scope": ["SECURITY-REVIEW.md"], "work": ["security", "review"] }
}
```

`LibraryEntry { id, version: u32, summary, sources: Vec<Cite { title, url
}>, fit_kinds: BTreeMap<String, Vec<String>>, role: Role }`, with
`deny_unknown_fields`. `roles::library()` returns every entry from a `const`
list of `include_str!` paths.

First entries (12):

| id | Draws on | Expects | work |
|---|---|---|---|
| `staff-engineer` | Google eng practices, "A Philosophy of Software Design" | diff | code, test |
| `principal-reviewer` | Google code review guide | file `REVIEW.md` | review |
| `appsec-reviewer` | OWASP ASVS 5.0, CWE Top 25 | file `SECURITY-REVIEW.md` | security, review |
| `sre` | Google SRE book (SLOs, error budgets, toil); `drafts: ["live"]` | file `RELIABILITY.md` | reliability |
| `test-engineer` | Testing trophy, property-based testing | diff | test |
| `performance-engineer` | Brendan Gregg's USE method | file `PERF.md` | performance |
| `technical-writer` | Diátaxis | diff | docs |
| `release-manager` | SemVer 2.0, Keep a Changelog | diff | release |
| `accessibility-auditor` | WCAG 2.2 AA | file `A11Y.md` | accessibility, review |
| `data-analyst` | "Trustworthy Online Controlled Experiments"; `repo: false` | file `ANALYSIS.md` | analysis, research |
| `product-researcher` | Jobs to be Done; `repo: false` | file `RESEARCH.md` | research |
| `debugger` | Agans' nine rules of debugging | `none`, output `ANSWER.md` | locate |

`tests/roles.rs` holds every entry to these rules: it parses, `id` equals
`role.id`, it passes `validate`, it cites at least one source with an
`https` URL, it has at least 5 principles, 5 method steps, 4 anti-patterns,
3 escalation rules and a non-empty output format, and it is listed in the
`include_str!` list. That last rule fails when a file under
`templates/roles/` is missing from the list.

New kinds (`security`, `performance`, `accessibility`, `analysis`,
`reliability`) are added to the template's `fit.kinds`. `ROLE_ICON` gains a
Lucide icon for each of the 12 ids.

The template's seed roles stay as they are. A user upgrades one by opening
Edit and choosing "Start from a library entry", which loads that entry's
descriptive fields into the form.

### 3. Validation and load (roles.rs, policy.rs)

`roles::validate(policy: &Policy, vocab: &Vocab) -> Vec<Problem>` is
deterministic and does no I/O. `Problem { role: Option<String>, field:
String, message: String, level: Level }`, where `Level` is `Refuse` or
`Warn`.

Refuse:

- an id not matching `^[a-z][a-z0-9-]{1,39}$`, or a duplicate id;
- anything other than exactly one role with `head: true`;
- an empty `title` or `mandate`, or an empty item in any list;
- any value over its cap or list over its max items (§1);
- a `\n` or `\r` in `title`, `mandate` or any list item;
- `expects: file` without `output`;
- an `output` or `scope` entry that is absolute or holds a `..` segment;
- an empty `output`, or one holding a `\n` or `\r`.

Warn:

- a `work` value that is not a key of the vocab's `kinds`;
- a `drafts` value that is not a key of `needs`;
- a `from` that names no library entry, or a version above the entry's
  current one.

**Load never drops a role from a file that parses.** `Policy` gains
`problems: Vec<Problem>` (`#[serde(skip)]`), filled by `load` from
`validate` against the effective vocab. `load` keeps every role and its
existing fallbacks, unchanged. A file that does not parse as a whole,
including one with a key `Role` does not know (`deny_unknown_fields`,
`policy.rs:55`), behaves as today: the template roles with a `note`
(`policy.rs:141–148`). Every write refuses on it (§4 step 2), so the
user's file is never overwritten while they fix it. Per-role tolerant
parsing is out of scope. The
fallback to template roles still happens only when the file has none.
`note` stays for whole-file failures. No problem found at load blocks a
hire. The renderer's `clean` (§1) bounds what reaches the hire. The Staff
tab and `cos roles list` show each problem next to its role. A role that
loads today keeps hiring tomorrow.

**Effective vocab** is the file's `fit` when present, else the template's.
The write path and `load` use the same function, `roles::vocab(&Policy)`.

### 4. The write path (roles.rs): the only writer of agents.json

```rust
pub enum Change {
    Add(Role),                     // id must be new
    Replace { id: String, role: Role },
    Retire(String),
    Restore(String),
}
pub struct Written { pub digest: String, pub role: String, pub notes: Vec<String> }

pub fn read(home: &Path) -> Result<(Policy, String)>   // policy as load gives it, digest of the bytes
pub fn apply(home: &Path, change: Change, read_digest: &str,
             contract: ContractApproval) -> Result<Written>
pub enum ContractApproval { None, Human }   // Human: a typed `contract` on a TTY, or the desktop confirm step
pub fn contract_diff(before: &Role, after: &Role) -> Vec<FieldDiff>
```

`apply`, in order:

1. `create_dir_all(home/context)`. Take `context/agents.json.lock` with
   `create_new`, writing the pid. A `Lock` guard (copied from `state.rs`'s
   shape; that one is private) removes it on drop. If the lock is held, the
   write is refused: "another edit holds <path> (pid N, running|not
   running); if no edit is in progress, delete <path>". The lock is never
   taken over silently.
2. Read the file's bytes. If they do not parse, refuse with the parse error
   and write nothing. A broken file the user is editing is never
   overwritten. If the file is missing, start from an empty `Policy`.
3. Compare `sha256(bytes)` (`"absent"` for a missing file) with
   `read_digest`. On a mismatch, refuse: "agents.json changed since you read
   it (read <a>, now <b>); reload".
4. Apply the change to the parsed `Policy`:
   - If the file had no `roles`, materialise the template's roles first, so
     the change does not drop them. `notes` then says: "agents.json now
     holds the template's 10 roles; later template changes to them will not
     reach this home". `fit` and `defaults` are not materialised. A file
     without `fit` keeps taking the template's at load.
   - `Replace` keeps `id` and `head` as they were. `Replace`, `Retire` and
     `Restore` of the head are refused.
   - When the written role's `from` names a library entry, its `fit_kinds`
     missing from the file's own `fit.kinds` are added. Only when the file
     has a `fit`; an existing kind is never changed.
   - The retired keys (`budget_usd`, `stall_secs`, `repeat`) are cleared on
     every role and repo. `notes` names how many were dropped.
5. Contract check. For `Add`, `before` is the library entry named by the
   role's `from` (an empty `Role` when there is none). For `Replace`,
   `before` is the current role. `Retire` and `Restore` change no contract.
   If `contract_diff(before, after)` is not empty and `contract` is not
   `Human`, refuse: "contract changes need a person's approval: <fields>".
6. `validate` with `roles::vocab`, before and after the change. The role an
   Add or Replace writes must carry no `Refuse` problem at all, whatever the
   file held before, so a replacement cannot keep a bad value or swap it for
   another. Elsewhere only a `Refuse` problem the change adds refuses, so a
   hand-broken role can still be retired or restored, and another role's
   problem blocks nothing. Refusals are returned; `Warn` problems on the
   changed role go into `notes`.
7. Serialise pretty to `out`. Require `serialise(parse(out)) == out`, a
   byte-equal round trip of what is about to be written. This holds with
   legacy keys, because they were cleared in step 4.
8. Copy the current bytes to `context/agents.json.bak`, overwriting it.
   This is one level of undo. Then call `state::atomic_write_bytes(path,
   out)`; its temp file is named `.state-*.tmp` in `context/`.
9. Return the new digest and the notes.

### 5. Proposing (roles.rs, pipeline.rs)

```rust
pub struct SpecialiseContext {
    business: String,          // ≤ 80 chars
    goals: Vec<String>,        // ≤ 20, each cut to 200 chars
    repos: Vec<RepoScope>,     // ≤ 30: { name, scope: Vec<String> }
    staff: Vec<(String, String)>, // (id, title) of every non-retired role
    brief: Option<String>,     // ≤ 1,000 chars; longer is refused, not cut
}
pub fn specialise_context(policy: &Policy, state: Option<&Value>, business: &str,
                          brief: Option<&str>) -> Result<SpecialiseContext>
pub fn specialise_prompt(entry: &LibraryEntry, ctx: &SpecialiseContext) -> String
```

`specialise_context` is pure. Goal titles come from `state["goals"][*]
["title"]` (callers pass `state::read(workspace)`). Repo names and `scope`
come from `policy.repos`. Each repo's `checks` are not sent, because a check
can carry a token or a host, and the proposal cannot set checks anyway.
Nothing else reaches the prompt: no mail, calendar, tickets, briefs, facts,
deliverables or journal.

`Config::propose_run(prompt) -> Run` uses agent `recruiter` (new
`.claude/agents/recruiter.md`: specialises role definitions and writes
nothing), model `sonnet`, `Toolset::None`, a $1.00 budget, a 180 s deadline
and `json_schema = roles::proposal_schema()`. `Toolset::None` has no caller
today. Acceptance 9 is a live run that proves it returns
`structured_output`. There is no fallback toolset. If `Toolset::None`
cannot return structured output, `propose` fails with that reason and the
design returns to Spec. It does not quietly run with tools or MCP servers.

`Proposal` is a closed type (`deny_unknown_fields`, and `additionalProperties:
false` in the schema). It holds only `title`, `mandate`, `serves`, `sandbox`,
`judged`, `done_when`, `principles`, `method`, `anti_patterns`,
`output_format`, `escalation` and `rationale`, each within §1's caps.
Deserialising a proposal that carries any other key (`checks`, `scope`,
`harness`, …) fails.

`roles::merge(entry, proposal, id: Option<&str>) -> Role` takes every
contract field (`expects`, `output`, `checks`, `scope`, `harness`, `model`,
`repo`, `deadline_secs`, `work`, `drafts`) from the library entry and the
descriptive ones from the proposal. `id` is the caller's `--id`, else the
entry's. `from` is set to `id@version`.

The result, `ProposalOut { role: Role, rationale: String, diff:
Vec<FieldDiff> }`, is printed or saved. Nothing is written to the policy.

### 6. CLI (src/main.rs → `cos roles`)

| Command | Does | Writes |
|---|---|---|
| `cos roles list [--library] [--retired] [--json]` | Staff (or library) with id, title, from, status and problems | nothing |
| `cos roles show <id> [--library] [--json]` | One role or entry in full. `--json` prints the role, ready to edit | nothing |
| `cos roles propose <library-id> [--id <id>] [--brief "…"] [--out <file>] [--json]` | Runs the recruiter, then prints the rationale and the field diff and saves the role JSON to `--out`. `cos --dry-run roles propose …` prints the exact prompt and sends nothing | the `--out` file only |
| `cos roles new <library-id> [--id <id>] [--yes]` | Adds the entry's role as is | agents.json |
| `cos roles new --file <role.json> [--yes]` | Adds the role in the file, e.g. a proposal | agents.json |
| `cos roles new --blank <id> --title "…" --mandate "…" [--yes]` | Adds a minimal role with `expects: diff` | agents.json |
| `cos roles edit <id> --file <role.json> [--yes]` | Replaces the role, printing the field diff first | agents.json |
| `cos roles retire <id> [--yes]` / `cos roles restore <id> [--yes]` | Flips `retired` | agents.json |

**`--out`** is opened with `create_new`, so an existing file is never
overwritten. It refuses any path that resolves (after `canonicalize` of its
parent) inside `COS_HOME/context/`, `COS_HOME/out/` or the workspace's
`.chief-of-staff/`, and any symlinked parent. The proposal can reach
`agents.json` only through `cos roles new --file`, and so only through
`apply`.

**Contract changes need their own approval.** `roles::contract_diff(before,
after)` compares the contract fields (`checks`, `scope`, `harness`, `model`,
`repo`, `expects`, `output`, `deadline_secs`, `work`, `drafts`). `before`
is the library entry named by the role's `from` for `new`, the current role
for `edit`, and empty for `new --blank` and for `new --file` without a known
`from`. When the diff is not empty, the command prints a "Contract changes"
block listing each changed field, old and new. It then writes only after a
typed `contract` on a terminal (stdin and stderr both a TTY). No flag stands
in for it, so an agent cannot approve a contract change. `--yes` without a
terminal refuses: "contract changes need a person at a terminal: <fields>".
The CLI passes `ContractApproval::Human` to `apply` only after that typed
answer. `roles_save` takes `allow_contract: bool`, which the dialog sets
only from a separate confirm step that lists the same block. The window's
Save is a person's click, which is the same boundary as `ritual_approve`.
`checks` run under `sh -c` when a hire ends (`src/agents/judge.rs:292`), so
no path reaches them without a person's second approval.

Every writing command prints what it will write (the role and, for edit, the
diff), then writes on `--yes` or after a typed `yes` on a terminal. Without
a terminal and without `--yes`, it refuses. Every write reads `(policy,
digest)` first and passes the digest to `apply`. A proposal is therefore
always printed by one command and approved by a later one. No single
invocation generates text and writes it.

**Who owns `claude`.** `claude` runs inside the group of a `cos proc-guard`
process (below), which the owner spawns with `proc::spawn_grouped`
(`src/proc.rs:210`). The group is killed only by the thread that spawned
the guard, or by the guard itself at its wall-clock limit. New: `proc::stream_lines_until(cmd, deadline, stop:
&AtomicBool, on_line)` is `stream_lines_on` with one more check per poll
(`POLL` = 1 s, `src/proc.rs:19`). When `stop` is set, that same thread calls
`kill_and_reap` and returns `Cancelled`. `claude::execute_until(program,
install, audit, run, guard, stop)` passes it through; `guard` names the cos
binary to run `claude` under `cos proc-guard`, and `claude::execute` runs
unguarded with a flag that is never set. `roles::propose(cfg, entry, ctx, id,
guard, stop)` is the one function both surfaces call; `id` is the staff id
the role gets. The invariant: the only code that signals `claude` is the
thread that spawned it, at its deadline or on its stop flag. No process id
crosses a thread.

On the CLI, `cos roles propose` installs SIGINT and SIGTERM handlers (`libc::
sigaction`, already a dependency) that set a static `AtomicBool`, and passes
it as `stop`. Ctrl-C therefore kills and reaps `claude` within one poll
before `cos` exits.

**When the owner dies without running any code** (SIGKILL, a crash, a
force-quit), no handler runs. A wall-clock bound that does not depend on the
owner comes from a guard process. The owner does not spawn `claude`
directly. It spawns `cos proc-guard --secs 180 -- <claude argv>` (a new
hidden subcommand) with `spawn_grouped`, so the guard leads the group. The
guard:

- calls `setpgid(0, 0)` first, so it leads a new group whoever started it;
- spawns `claude` with `process_group` left unset, so `claude` is in the
  guard's group and inherits the guard's stdout and stderr (the owner's
  pipes);
- polls `try_wait` every 50 ms for at most `--secs`, and on the limit kills
  and reaps `claude` (`Child::kill`, `wait`);
- then, or when `claude` exits, runs the sweep (`TOOL_CALL(guard_sweep)`
  below): SIGTERM to its group with itself ignoring it, up to 300 ms of
  grace, then SIGKILL to every remaining member the kernel lists for the
  group except itself; and exits with `claude`'s status (124 on timeout).

The guard sleeps between polls and runs no model, so it cannot stall the
way `claude` can. The owner's paths are unchanged: deadline, stop flag and
quit call `kill_and_reap` on the guard's group, which holds `claude` too.
If the owner is SIGKILLed, the guard still ends the group at 180 s. The
pipe also closes, so a `claude` that writes dies sooner on `SIGPIPE`. The
remaining gap is the guard itself being SIGKILLed along with the owner,
which is the PPID-1 sweep's job (process-ownership rule). The test matrix
proves the 180 s bound with a stand-in deadline of 2 s and a fake that
never writes.

### 7. Desktop

The commands are registered for the `main` window, the same boundary
`ritual_approve` has. Each command that names a business checks it with
`named(&s, &business)` and works on `Current.home`, never on a path from
the UI.

| Command | Does |
|---|---|
| `agents_roles` | Gains `digest`, `problems` per role, `retired` and `library` (entry id, version, title, summary, sources) |
| `roles_library_entry(id)` | One `LibraryEntry` |
| `roles_propose_prompt(business, id, brief)` | The exact text Propose would send (`specialise_prompt`), shown before running |
| `roles_propose(business, id, new_id, brief)` | Shaped like `brief_run` (`lib.rs:1582`): returns at once, and runs `roles::propose` in-process on a thread it spawns, against `Current`'s home, workspace and projects. One at a time. The thread owns the `claude` child (§6) and emits `role-proposal` with `ProposalOut` or the failure when it ends |
| `roles_propose_cancel()` | Sets the running proposal's stop flag. The owning thread kills and reaps `claude` within one poll and emits `role-proposal` with `cancelled` |
| `roles_save(business, change, digest)` | `named`, then `roles::apply(&s.home, change, &digest)`. Returns `{ digest, notes }` or the problems |

The state is `Proposing(Mutex<Option<Running { stop: Arc<AtomicBool>,
done: Receiver<()> }>>)`. `on_run_event` (quit) sets the flag and waits on
`done` for at most 3 s, which is one poll plus `kill_and_reap`. The window
never holds or signals a process id. Its thread spawns `cos proc-guard`
(the cos binary the window found), which spawns `claude`; with no cos
binary, `roles_propose` refuses rather than run `claude` unguarded.

`COS_CLAUDE` is a new environment override for the `claude` binary, read the
way `herdr::hc()` reads `COS_HC` (`src/herdr.rs:31`). It is used by the CLI
and desktop tests and is listed in the side-effect inventory.

UI (`app.js`, `index.html`; tokens only; every state through `stateView`,
with failures as `unreachable`):

- **Staff pane header:** a "New role" button opens the role dialog at
  step 1. Problems from `agents_roles` show as a warning line on each
  affected card.
- **Role dialog**, three steps in one `<dialog>`:
  1. *Start from*: library cards (title, summary, and sources as links that
     open in the default browser through the existing navigation hook) plus
     "Blank".
  2. *Specialise* (optional): an id field, a brief textarea, and a
     disclosure "What is sent" showing `roles_propose_prompt`'s text. A
     "Propose for <business>" button runs it. While it runs, the dialog
     shows `stateView("reading")` with Cancel (`roles_propose_cancel`).
     After 6 s it shows the slow state, and on failure `unreachable` with
     Try again. "Skip" takes the entry as is.
  3. *Review and save*: a form over the typed fields, with lists as
     textareas, one item per line. Each field the proposal changed against
     the entry carries a "changed" badge, and the rationale shows above the
     form. Contract fields are read-only behind an "Edit contract"
     disclosure. Problems returned by `roles_save` render next to their
     field, and its notes show after saving. Save is the approval.
- **Role pane:** sections for Principles, Method, Never, Hand back and
  Escalates when, when non-empty. Actions are Edit (the dialog at step 3
  with the role, plus "Start from a library entry") and Retire (confirm).
  The head shows neither.
- **Retired** roles sit in a collapsed group under the grid, each with
  Restore. The hire dialog's role select leaves them out.
- **Digest conflict:** "The staff changed since you opened this. Reload and
  re-apply?" with Reload. The form's content is kept.
- The dialog's loader functions are added to `tests/design.rs`'s loader
  list.

### 8. Errors

| Case | Behaviour |
|---|---|
| agents.json does not parse | No write. The message names the file and the parse error |
| Lock held | No write. The message names the lock path, its pid and whether that pid runs |
| Digest mismatch | No write: reload prompt |
| Refuse problems | No write. Problems are returned per field |
| Propose run fails, times out or is cancelled | No proposal, with the reason. The entry can still be used as is |
| Proposal fails its schema, or carries an unknown key | Treated as a failed run. Nothing is merged |
| Hire names a retired role | Refused with a restore hint |
| Contract changed without a person's approval | No write. The changed contract fields are named |
| `--out` exists, is protected or has a symlinked parent | Nothing written; the reason is named |
| Role with problems at load | Loaded and hireable. Problems shown. Preamble rendered through `clean` |

## Semantic ISA

### Scope

- **Agent:** the recruiter proposes a specialised role definition. The write
  path persists roles the user approved, and the hire preamble hands a
  role's text to a hired agent.
- **Environment:** `context/agents.json` and its `.bak` and `.lock`; the
  `claude` CLI (network to Anthropic, and the local audit log); the desktop
  window; a hired agent's worktree.
- **Harms, ranked:** (1) model-authored text becomes a check command, a scope
  path or a harness; (2) a write corrupts or clobbers the user's
  agents.json; (3) business data leaves the machine beyond what proposing
  needs; (4) role text steers a hire outside its sandbox or forges its
  preamble; (5) a `claude` child outlives its owner.

### Side-effect inventory

| Capability | System | Reversible? | Who is affected if it misfires |
|---|---|---|---|
| Write `context/agents.json` | local file (gitignored) | One level, through `.bak` | The user: every later hire |
| Write `context/agents.json.bak` | local file | Overwritten each write | The user's undo |
| Create/remove `context/agents.json.lock` | local file | Yes; a stale one is named, never taken over | Concurrent edits |
| Propose run (`claude -p`) | Anthropic API | Sending cannot be undone | Goal titles, repo names and scopes, staff titles and the brief leave the machine |
| `-p <prompt>` in the child's argv | local process table | Gone when the child exits (≤ 180 s) | Local users who can run `ps` |
| Audit log append (`out/audit/<day>.jsonl`) | local file (gitignored) | By hand | The stream, including the proposal text, kept locally |
| `--out <file>` from `cos roles propose` | local file the user names | By hand | Only that path |
| Role text in a hire's preamble | sent to the hire's model provider on each spawn; acts in the worktree | Sending cannot be undone; worktree changes are drafts | That hire's diff; role text leaves the machine with each hire |
| Library source links | default browser | Yes | Opens a public URL |
| Create `context/` under `COS_HOME` (`apply` step 1) | local directory | `rmdir` by hand | None; it holds only cos's own files |
| Spawn `cos proc-guard`, which spawns `claude` (the program named by `COS_CLAUDE`, else `claude` on `PATH`) in its group | local processes | Ends at the owner's deadline, on the stop flag, at exit, or at the guard's 180 s even if the owner is SIGKILLed | Whoever sets `COS_CLAUDE` in the environment chooses what runs with the prompt; the same trust as `COS_HC` for Herdr |
| Kill and reap the `claude` group | local processes | n/a | Only the group the calling thread spawned |
| SIGINT/SIGTERM handlers in `cos roles propose` | the cos process | Process lifetime | Only that invocation |

### Instruction set

| Instruction | Core | Governance property | Sink? | Replaces capability |
|---|---|---|---|---|
| `LOAD(library_entry)` | Memory | Deterministic, compiled-in | no | — |
| `LOAD(policy, digest)` | Memory | Deterministic I/O | no | file read |
| `LOAD(goal_titles)` | Memory | Allowlisted field of state.json | no | state.json read |
| `VERIFY(specialise_context)` | Normative | Deterministic checkpoint | no | — |
| `TOOL_CALL(claude.propose)` | Execution | Deterministic action, network | **yes** | claude -p |
| `GENERATE(proposal)` | Cognitive | Probabilistic output | no | — |
| `VERIFY(proposal_schema)` | Normative | Deterministic checkpoint | no | — |
| `VERIFY(contract_from_library)` | Normative | Deterministic checkpoint (`merge`) | no | — |
| `STORE(proposal_file)` | Execution | Deterministic, user-named path | **yes** | file write |
| `VERIFY(user_approval)` | Normative | Human checkpoint | no | — |
| `VERIFY(policy_valid)` | Normative | Deterministic checkpoint | no | — |
| `STORE(agents.json)` | Execution | Deterministic action | **yes** | file write |
| `VERIFY(render_clean)` | Normative | Deterministic checkpoint | no | — |
| `RENDER(preamble)` | Execution | Deterministic, to hired agent | **yes** | hire prompt |
| `RENDER(finish_line)` | Execution | Deterministic, to hired agent | **yes** | hire prompt (`hire::plan`) |
| `TOOL_CALL(kill_and_reap)` | Execution | Deterministic, the calling thread's own child only | **yes** | proc::kill_and_reap |
| `TOOL_CALL(guard_sweep)` | Execution | Deterministic, the guard's own group only | **yes** | proc::guard |

### Bindings

`Role` below means the serde shape of `policy::Role` (§1), with
`deny_unknown_fields`.

```json
{ "instruction": "TOOL_CALL(claude.propose)",
  "input_schema": { "type": "object", "additionalProperties": false,
    "required": ["entry_id", "business", "goals", "repos", "staff"],
    "properties": {
      "entry_id": { "type": "string", "pattern": "^[a-z][a-z0-9-]{1,39}$" },
      "business": { "type": "string", "maxLength": 80 },
      "goals": { "type": "array", "maxItems": 20, "items": { "type": "string", "maxLength": 200 } },
      "repos": { "type": "array", "maxItems": 30, "items": { "type": "object", "additionalProperties": false,
        "required": ["name", "scope"], "properties": {
          "name": { "type": "string", "maxLength": 100 },
          "scope": { "type": "array", "items": { "type": "string" } } } } },
      "staff": { "type": "array", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } },
      "brief": { "type": "string", "maxLength": 1000 } } },
  "output_schema": "roles::proposal_schema(): closed object of title, mandate, serves, sandbox, judged, done_when, principles[], method[], anti_patterns[], output_format, escalation[], rationale, each within the §1 caps" }

{ "instruction": "STORE(agents.json)",
  "input_schema": { "type": "object", "additionalProperties": false,
    "required": ["change", "read_digest"],
    "properties": {
      "change": { "oneOf": [
        { "type": "object", "additionalProperties": false, "required": ["add"], "properties": { "add": { "$comment": "Role" } } },
        { "type": "object", "additionalProperties": false, "required": ["replace"], "properties": { "replace": { "type": "object", "additionalProperties": false, "required": ["id", "role"], "properties": { "id": { "type": "string" }, "role": { "$comment": "Role" } } } } },
        { "type": "object", "additionalProperties": false, "required": ["retire"], "properties": { "retire": { "type": "string" } } },
        { "type": "object", "additionalProperties": false, "required": ["restore"], "properties": { "restore": { "type": "string" } } } ] },
      "read_digest": { "type": "string", "pattern": "^([0-9a-f]{64}|absent)$" } } },
  "output_schema": { "type": "object", "additionalProperties": false, "required": ["digest", "role", "notes"],
    "properties": { "digest": { "type": "string" }, "role": { "type": "string" }, "notes": { "type": "array", "items": { "type": "string" } } } } }

{ "instruction": "RENDER(preamble)",
  "input_schema": "a Role as loaded (never dropped, possibly with problems)",
  "output_schema": { "type": "string", "maxLength": 12000 } }

{ "instruction": "TOOL_CALL(kill_and_reap)",
  "input_schema": { "type": "object", "additionalProperties": false, "required": ["stop"],
    "properties": { "stop": { "type": "boolean", "$comment": "the flag; the child handle never leaves the spawning thread" } } },
  "output_schema": { "type": "null" } }
```

### Taint

- **Sources:** goal titles from state.json (often copied from mail subjects
  or tickets), repo names and scopes, the user's `brief`, `GENERATE(proposal)`,
  a hand-edited agents.json, and the `--file` role JSON.
- **Propagation:** the proposal is tainted because its inputs and its
  generator are. In a merged role, the descriptive fields carry the
  proposal's taint. Contract fields come from the compiled-in library entry
  and are clean, unless the user edits them, which is the user's own input
  and approved at Save. A stored role keeps its taint through the STORE →
  LOAD round trip. Every role text is treated as tainted at RENDER.
- **Sinks:** `TOOL_CALL(claude.propose)`, `STORE(proposal_file)`,
  `STORE(agents.json)`, `RENDER(preamble)`, `TOOL_CALL(kill_and_reap)`.

| Sink | Accepted input must be cleared by | Check |
|---|---|---|
| `TOOL_CALL(claude.propose)` | `VERIFY(specialise_context)` (`roles::specialise_context`) | Built only from goal titles (≤ 20, ≤ 200 chars), repo names and scopes (no checks), staff ids and titles, and a brief (≤ 1,000 chars, refused above). The type has no field for anything else. The run uses `Toolset::None` (`--tools "" --strict-mcp-config`) and has no fallback. The exact text is shown in "What is sent" and by `--dry-run` |
| `STORE(proposal_file)` | `VERIFY(proposal_schema)`, `VERIFY(contract_from_library)` and `VERIFY(out_path)` | `Proposal` deserialises with `deny_unknown_fields` and caps. `merge` takes every contract field from the entry. `--out` is `create_new`, outside `context/`, `out/` and `.chief-of-staff/`, with no symlinked parent |
| `STORE(agents.json)` | `VERIFY(user_approval)`, `VERIFY(contract_approval)`, then `VERIFY(policy_valid)` in `roles::apply` | Approval is CLI `--yes` or a typed `yes` in an invocation that generated nothing, or desktop Save after the form was shown. A non-empty `contract_diff` also needs `ContractApproval::Human`: a typed `contract` on a terminal (no flag stands in), or `allow_contract: true` from the dialog's separate confirm step. `apply` step 5 refuses otherwise. `apply` then requires: lock held, digest equal, parse OK, head unchanged, no `Refuse` problem on the role an Add or Replace writes, no `Refuse` problem the change adds elsewhere, byte-equal round trip |
| `RENDER(preamble)` | `VERIFY(render_clean)` (`roles::clean`) | Whitespace collapsed (no newline survives), each value cut to its cap, lists cut to max items. Drafts-only sentence last. Worktree sandbox unchanged |
| `RENDER(finish_line)` | `VERIFY(render_clean)` (`roles::clean`) | A role's `done_when` and `output` reach the prompt only through the outward finish line in `hire::plan`: the original finish line cleaned and cut to `CAPS.text` (300), the drafted one (which embeds the original's subject and the output path) to twice that (600) |
| `TOOL_CALL(kill_and_reap)` | Spawning-thread ownership | On a proposal's path the kill runs inside `stream_lines_until` (stop flag, deadline) on the guard `Child` that call spawned and has not yet reaped; callers can only set `stop`, and no pid is stored outside that call. `proc::wait_until` and `Drain::finish` (existing helpers shared by every `proc` spawn) also signal the group; `Drain::finish`'s `kill_group` after the reap reaches only members still alive, because a group id stays reserved while any member lives and is a no-op (ESRCH) once none does |
| `TOOL_CALL(guard_sweep)` | `VERIFY(own_group)` (`setpgid(0, 0)` at the guard's start) | The guard leads a new group, so its spawner is outside it. On its limit it kills and reaps its own direct child (`Child::kill`, `wait` on the `Child` it spawned). At the end, either way, it sends SIGTERM to its group (ignoring it itself), waits up to 300 ms, then SIGKILLs every remaining member the kernel lists for that group (`proc_listpids`, `PROC_PGRP_ONLY`) except itself |

### Violations

| Violation | Evidence recorded | Recovery |
|---|---|---|
| Proposal fails schema or carries an unknown key | Error and field path in the audit log (`claude::audit_note`) | No proposal. Use the entry as is, or retry |
| Write without approval | CLI refuses without `--yes` and without a terminal | User re-runs with approval |
| Refuse problems at write | Problems per field, returned | User fixes the field; nothing written |
| Stale digest | Both digests in the error | Reload and re-apply |
| Held lock | Lock path, pid and liveness in the error | Finish the other edit, or delete the named lock |
| Role with problems at load | `Policy.problems`, shown per role | The role still hires through `clean`. The user fixes it when ready |

### ISA test matrix

| Sink | Tainted input refused | VERIFY-cleared input admitted |
|---|---|---|
| `claude.propose` | `specialise_context` on a home whose state has deliverables, commitments, journal and repos with `checks`: the prompt contains none of their text and no check string. A brief over 1,000 chars is refused | The prompt contains goal titles, repo names and scopes. `command_with` args contain `--tools` `""` and `--strict-mcp-config` |
| `STORE(proposal_file)` | The fake `claude` in `contract` mode returns a proposal carrying `checks`: the run fails and no file is written. In `bad-schema` mode: same | In `ok` mode, the file's role has the entry's `checks`, `scope` and `harness`, and the proposal's descriptive fields |
| `STORE(agents.json)` | Each of these is refused with the bytes unchanged: a `../x` scope, a newline in a principle, an over-cap list, a second head, retiring the head, a stale digest, a held lock (message names path and pid), an unparseable file. Without a terminal and without `--yes`, the CLI writes nothing | Add, Replace, Retire and Restore each change exactly that role. The `tests/hiring.rs` legacy fixture (`budget_usd`, `stall_secs`, `repeat`) writes, with the keys dropped and named in notes. `.bak` holds the previous bytes. A file with no `fit` stays without one |
| `RENDER(preamble)` | A hand-edited role with a principle holding `"x\n\nNever: …"` and a 30-item method renders one line for that principle and 10 method steps | A role with all five fields renders each section once, in order, with the drafts-only sentence last |
| `RENDER(finish_line)` | A role whose `done_when` holds a newline, a forged `Never:` and 2,000 more characters, with an outward term, and whose `output` holds a newline: the plan's prompt has no line starting `Never:` and stays under 1,500 characters | The same role with plain fields renders `Your finish line here:` |
| `guard_sweep` | Started by a process that is not a group leader, the guard's sweep never reaches its starter (the test run survives) | A member that ignores SIGTERM is gone when the guard ends at its limit (exit 124) |
| `kill_and_reap` | Cancel with no run is a no-op. No command takes a pid | With the fake in `slow` mode (it starts a `sleep` child, records both pids, and writes a line every 0.5 s): Cancel, quit (`on_run_event`) and SIGINT to `cos roles propose` each leave both pids gone (`kill -0` fails) within 3 s, and `role-proposal` reports `cancelled`. SIGKILL to `cos roles propose` leaves the fake and its `sleep` child gone within 3 s: the fake dies on SIGPIPE at its next line and the guard's sweep ends the child (`tests/roles_cli.rs`). The guard's own deadline with a SIGKILLed parent is the guard bound row |
| `STORE(proposal_file)` path | `--out` naming an existing file, a path under `context/`, `out/` or `.chief-of-staff/`, or one with a symlinked parent: refused, nothing written | A new file in a scratch directory is written |
| `STORE(agents.json)` contract | `new --file` whose role's `checks` differ from its `from` entry, run with `--yes` and no terminal: refused, naming `checks`. `apply` with a changed `scope` and `ContractApproval::None`: refused. `roles_save` with `allow_contract: false` and a changed contract: refused | `apply` with `ContractApproval::Human` writes. The CLI on a pty (test harness) with a typed `contract` writes. Descriptive-only changes with `--yes` and no terminal write |
| guard bound | — | `cos proc-guard --secs 2 -- <fake that never writes and starts a child>`, with its parent SIGKILLed right after spawn: the fake and its child are gone within 6 s (`tests/proc_guard.rs`) |

Stand-ins: `tests/fixtures/fake-claude-proposal.sh` (new) prints a
stream-json result whose `structured_output` depends on `FAKE_PROPOSAL=ok |
bad-schema | contract | slow`. Library tests pass it as `Bins.claude`; CLI
and desktop tests set `COS_CLAUDE`. There is no live model in the suite.

## Alternatives

- **Markdown playbook field:** rejected by the user in favour of typed fields
  that validate and diff.
- **Library fetched from a URL at runtime:** a network read and a supply
  path for prompt text into every hire. Compiled-in data is reviewed in a
  PR.
- **Validating at load and dropping invalid roles:** would stop roles that
  hire today (an unknown `work` name is tolerated by `fit` now). Rejected
  for report-at-load and sanitise-at-render.
- **Propose and write in one command (`new --specialise --yes`):** would
  approve text nobody read. Rejected.
- **Writing through `state::commit`:** that path is for `state.json` and its
  revision and transition rules. A digest check gives the same stale-write
  protection without a schema change.
- **Letting the proposal suggest checks:** puts model text into a shell
  command. Not in this change.
- **Exposing roles over MCP:** not asked for. The HTTP server stays
  read-only and stdio gains nothing here.

## Security and privacy

- **Read:** agents.json, the compiled-in library, and goal titles from
  state.json.
- **Stored locally:** agents.json, `.bak` and `.lock` under `context/`
  (gitignored), the propose run's stream in `out/audit/` (gitignored), and a
  proposal file only where the user names one.
- **Leaves the machine on Propose:** the library entry, the business name,
  goal titles, repo names and scopes, staff ids and titles, and the user's
  brief, sent to Anthropic through `claude`. No repo checks are sent. No
  tool calls are possible. The user sees the exact text first.
- **Leaves the machine on each hire:** the role's rendered preamble, as part
  of the hire's prompt to its harness, as today, now longer.
- Library fixtures name no customer.

## Testing

- **Static:** `tests/roles.rs` holds the library rules (§2).
  `tests/design.rs` gets the dialog's loaders added to its loader list
  (L1531).
- **Unit-level, in `tests/roles.rs`:** `validate` per rule and level;
  `clean`; `merge` never takes a contract field and honours `--id`;
  `preamble` order and bound; `specialise_context` exclusions; `apply`
  refusals and admits from the ISA matrix on temp homes, including the
  legacy fixture; `fit::check` never suggests a retired role.
- **Integration:** `tests/roles_cli.rs` drives `cos roles` end to end on a
  temp `COS_HOME` with `COS_CLAUDE` set to the fake: list and show the
  library, propose to `--out`, `new --file`, `new <id>`, edit, retire,
  restore, the no-terminal refusal, and a hire refused for a retired role.
  Then `cos --dry-run agents spawn --role <new>` shows the structured
  preamble in the plan.
- **Desktop:** `apps/desktop/tests/roles_ipc.rs` covers `roles_save` (add,
  conflict, refused problems, a foreign business name writes nothing),
  `roles_propose` with the fake (ok, cancel, quit, no cos binary), Cancel
  with no run, and `roles_propose_prompt`. `apps/desktop/tests/roles_page.rs` (new, added to
  `task test-e2e` in `Taskfile.yml`) drives the preview
  (`apps/desktop/examples/preview.rs`) in a browser: New role → library →
  Skip → Save → card appears → Retire → in Retired group.
- **WCAG:** `examples/wcag.rs` adds the role dialog's three steps.

## Confidence

| Piece | Tier | Moves it up |
|---|---|---|
| Schema, validation, write path | rock | — |
| Preamble rendering and bound | rock | Caps are one table; the bound is tested |
| CLI | rock | — |
| Propose run on `Toolset::None` | sand | Never run before. Acceptance 9 (live) makes it rock. If it fails, the design returns to Spec |
| Library content (12 entries) | sand | Each entry cites its source. Review reads each against it. Minimum counts gate the shape |
| Desktop dialog and owned child | sand | The e2e pass, the cancel test and the WCAG run make it rock |

## Acceptance

1. `cos roles list --library` prints 12 entries, each with a cited source.
2. `cos roles new appsec-reviewer --yes` on a home with no agents.json
   writes one holding the 10 template roles and `appsec-reviewer` with
   `from: "appsec-reviewer@1"`, and prints the materialisation note. `cos
   roles list` shows it.
3. With `COS_CLAUDE` set to the fake in `ok` mode, `cos roles propose sre
   --brief "…" --out /tmp/x/sre.json` prints the rationale and a diff and
   writes only that file. agents.json is unchanged. The file's role has
   `checks` equal to the library entry's. `cos roles new --file
   /tmp/x/sre.json --yes` then adds it.
4. With a hand-edited agents.json that does not parse, `cos roles retire
   reviewer --yes` refuses, and the file bytes are unchanged.
5. `cos roles retire reviewer --yes`. Then `cos agents spawn --role reviewer
   …` is refused with the restore hint, and a fit check on a review task
   does not suggest the reviewer. `cos roles restore reviewer --yes` makes it
   hireable again. `cos agents scorecard` still lists the earlier rows under
   the role id `reviewer`.
6. `cos --dry-run agents spawn --role appsec-reviewer …` prints a plan whose
   new `preamble` field (computed with `policy::preamble`) has Principles,
   Method, Never, Hand back and Stop-and-leave sections, in that order, with
   the drafts-only sentence last.
7. In the desktop app, on the demo home: Staff → New role → "Application
   security reviewer" → Skip → Save. The card appears. Its pane shows the
   five sections. Retire moves it to Retired, and it is gone from the hire
   dialog's select. Propose with the fake in `slow` mode, then Cancel while
   it runs: the pid the fake recorded is gone within 3 s. The same holds for
   quitting the app mid-run and for Ctrl-C on `cos roles propose`.
8. A home whose agents.json carries legacy `budget_usd` keys accepts `cos
   roles new principal-reviewer --yes`, and the notes name the dropped keys.
9. Live, once, run by the author in the PR with the cost reported: `cos
   roles propose performance-engineer --brief "Rust services" --out …`
   against real `claude` returns a valid proposal under $1.00. If it fails on
   `Toolset::None`, the PR stays draft and the design goes back to Spec.
10. `task test` and `task test-e2e` pass.
