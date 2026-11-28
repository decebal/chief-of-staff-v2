# Role library: design

Date: 2026-11-28. Brief: [2026-11-28-role-library-brief.md](2026-11-28-role-library-brief.md).
Epic: `t-b0b659`. Branch: `feat/agents-role-library`.

## Decision

`cos` ships a library of specialist role definitions as data
(`templates/roles/*.json`, compiled in). A role gains five optional typed
fields: `principles`, `method`, `anti_patterns`, `output_format`,
`escalation`. It also gains `from` for provenance and `retired`. One module,
`src/agents/roles.rs`, owns the library, the validator and the only write to
`context/agents.json`. The write runs under a lock with a digest check and an
atomic replace, and refuses any policy that fails validation. A headless run
with no tools specialises a library entry to the business and returns a
proposal limited to descriptive fields. The proposal is written only when the
user approves it. The CLI (`cos roles …`) and the desktop Staff tab (New
role, Edit, Retire, Restore) both call that module. Every hire's preamble
carries the role's structured fields.

## Context

- `src/agents/policy.rs`: `Role` (L56), `Policy` (L110), `load` (L137) falls
  back to the template's roles when the file has none and to defaults with a
  `note` when it does not parse. `preamble` (L268) puts title, mandate and
  the output rule in front of every hire's prompt. `Policy::hire` (L182)
  refuses an unknown role or the head.
- `templates/context/agents.json`: the seed. 10 roles with one-line mandates,
  `defaults`, `repos`, and the `fit` vocabulary (`kinds`, `needs`,
  `outward_done`, `ops`).
- `src/agents/fit.rs`: `Vocab`. A role's `work` names `fit.kinds`, and its
  `drafts` names `fit.needs`.
- `src/agents/hire.rs` `plan` and `contract` read roles through `Policy`.
  `src/agents/ledger.rs` keys verdict rows by role id, so ids must stay
  stable.
- `apps/desktop/src/lib.rs` `agents_roles` (L1253) is read-only. The UI is
  in `apps/desktop/ui/app.js`: `renderStaff` and `roleCard` (L431–535),
  `renderRole` (L540), and the hire dialog's `fillRole` (L1152).
- `src/claude.rs` `Run` with `Toolset::None` and `json_schema` already
  supports a no-tools structured run, and `proc` owns the child.
- `src/state.rs` `atomic_write` (L548): temp file, fsync, rename, fsync dir.
- Nothing writes `context/agents.json` today. The user edits it by hand.

## Design

### 1. Role schema (policy.rs)

New optional fields on `Role`, all `#[serde(default)]`. An old file still
loads.

| Field | Type | Meaning | Cap |
|---|---|---|---|
| `principles` | `Vec<String>` | Standards a top practitioner holds ("every finding cites file:line") | 3–10 items when present, ≤ 300 chars each |
| `method` | `Vec<String>` | Ordered steps the role follows | ≤ 12, ≤ 300 |
| `anti_patterns` | `Vec<String>` | What it refuses to do | ≤ 10, ≤ 300 |
| `output_format` | `Option<String>` | Exact shape of what it hands back | ≤ 1500 |
| `escalation` | `Vec<String>` | When it stops and leaves the question for the user | ≤ 8, ≤ 300 |
| `from` | `Option<String>` | `<library-id>@<version>` it was made from | — |
| `retired` | `bool` | Kept for history, not offered for hire | — |

`Policy::hire` refuses a retired role: "the <title> is retired; restore it
with `cos roles restore <id>`". `preamble` appends, after the mandate and
when present: `Principles:` (bullets), `Method:` (numbered), `Never:`
(bullets), `Hand back:` (output_format), `Stop and leave it for the user
when:` (bullets). The existing output rule and the drafts-only sentence stay
last. The caps bound the preamble at about 1,500 words in the worst case.

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

`LibraryEntry { id, version: u32, summary, sources: Vec<Cite>, fit_kinds:
BTreeMap<String, Vec<String>>, role: Role }`, `deny_unknown_fields`.
`roles::library()` returns every entry. It is built from a `const` list of
`include_str!` paths, and `tests/roles.rs` fails when a file under
`templates/roles/` is missing from the list.

First entries (12), each with at least 5 principles, 5 method steps, 4
anti-patterns, an output format, 3 escalation rules and at least one cited
public source:

| id | Draws on | Expects |
|---|---|---|
| `staff-engineer` | Google eng practices, "A Philosophy of Software Design" | diff |
| `principal-reviewer` | Google code review guide | file `REVIEW.md` |
| `appsec-reviewer` | OWASP ASVS 5.0, CWE Top 25 | file `SECURITY-REVIEW.md` |
| `sre` | Google SRE book (SLOs, error budgets, toil); drafts for `live` | file `RELIABILITY.md` |
| `test-engineer` | Testing trophy, property-based testing | diff |
| `performance-engineer` | Brendan Gregg's USE method | file `PERF.md` |
| `technical-writer` | Diátaxis | diff |
| `release-manager` | SemVer 2.0, Keep a Changelog | diff |
| `accessibility-auditor` | WCAG 2.2 AA | file `A11Y.md` |
| `data-analyst` | "Trustworthy Online Controlled Experiments"; no repo | file `ANALYSIS.md` |
| `product-researcher` | Jobs to be Done; no repo | file `RESEARCH.md` |
| `debugger` | Agans' nine rules of debugging | nothing (answer file) |

New `fit.kinds` used by these entries (`security`, `performance`,
`accessibility`, `analysis`, `reliability`) go into the template's `fit` as
well. When a home's own `fit` lacks one of them, adopting the entry adds the
entry's `fit_kinds` to the written file. It never changes a kind that
already exists.

The template's seed roles stay as they are. A user upgrades one by opening
Edit and choosing "Start from a library entry", which loads that entry's
descriptive fields into the form.

### 3. Validation (roles.rs)

`roles::validate(policy: &Policy, vocab: &Vocab) -> Result<(), Vec<Problem>>`,
deterministic, no I/O. `Problem { role: Option<String>, field, message }`.

- Ids match `^[a-z][a-z0-9-]{1,39}$` and are unique.
- Exactly one role has `head: true`.
- `title` is 1–60 chars, `mandate` 1–600. Each list stays inside its caps,
  and no list item is empty.
- `expects: file` requires `output`. `output` and every `scope` entry are
  relative paths with no `..` segment and no leading `/`.
- Every `work` value is a key of the vocab's `kinds`, and every `drafts`
  value a key of its `needs`.
- `from`, when set, names a library entry and a version ≤ the entry's
  current version.

### 4. The write path (roles.rs): the only writer of agents.json

```rust
pub enum Change {
    Add(Role),                 // id must be new
    Replace { id: String, role: Role },
    Retire(String),
    Restore(String),
}
pub struct Written { pub digest: String, pub role: String }

pub fn apply(home: &Path, change: Change, read_digest: &str,
             fit_kinds: &BTreeMap<String, Vec<String>>) -> Result<Written>
```

In order:

1. Take `context/agents.json.lock` with `create_new`. A held lock is refused
   ("another edit is in progress"), never stolen, and removed on drop. This
   is the same shape as `state::commit`.
2. Read the file's bytes. If they do not parse, refuse with the parse error
   and write nothing. A broken file the user is editing is never
   overwritten. If the file is missing, start from the template's
   `defaults`, `roles` and `fit`.
3. Compare `sha256(bytes)` (`"absent"` for a missing file) with
   `read_digest`, the digest the caller saw when it read. On a mismatch,
   refuse: "agents.json changed since you read it; reload".
4. Apply the change to the parsed `Policy`. If the file had no `roles`,
   materialise the template's roles first so the change does not drop them.
   `Replace` and `Retire` of the head are refused, and `Replace` may not
   change `id` or `head`. Missing `fit_kinds` are merged in.
5. `validate` with the effective vocab. Any problem refuses the write and
   returns the problems.
6. Serialise pretty, parse the result back, and require it to equal the
   policy (round-trip). Then `state::atomic_write_bytes`.
7. Return the new digest.

`roles::read(home) -> (Policy, digest)` is what every caller reads first.

### 5. Specialising (roles.rs + pipeline)

`roles::specialise_prompt(entry: &LibraryEntry, ctx: &SpecialiseContext) ->
String`. `SpecialiseContext { business: String, goals: Vec<String>, repos:
Vec<RepoTerms>, staff: Vec<(String, String)>, brief: Option<String> }` holds:

- goal titles from the workspace's `state.json` (titles only),
- repo directory names with their policy `checks` and `scope`,
- the current staff's ids and titles (to avoid duplicates),
- an optional free-text brief from the user ("focus on our Rust services"),
  at most 1,000 chars.

Nothing else goes in: no mail, calendar, tickets, briefs or facts.

`Config::specialise_run(prompt) -> Run` has agent `recruiter` (new
`.claude/agents/recruiter.md`: specialises role definitions, writes nothing),
model `sonnet`, `Toolset::None`, budget $1.00, deadline 180 s, and
`json_schema = roles::proposal_schema()`.

`Proposal` is closed (`additionalProperties: false`) and holds only
descriptive fields: `title`, `mandate`, `serves`, `sandbox`, `judged`,
`done_when`, `principles`, `method`, `anti_patterns`, `output_format`,
`escalation`, and `rationale` (shown to the user, never stored).

`roles::merge(entry, proposal) -> Role` takes every contract field (`id`,
`expects`, `output`, `checks`, `scope`, `harness`, `model`, `repo`,
`deadline_secs`, `work`, `drafts`) from the library entry and the
descriptive ones from the proposal. It sets `from = id@version`. The model
cannot set a check command, a scope path or a harness, because the schema
has no field for them.

The proposal is returned to the caller as `{ role, rationale, diff }`. `diff`
lists each changed field against the library entry. Nothing is written.

### 6. CLI (src/main.rs → `cos roles`)

| Command | Does |
|---|---|
| `cos roles list [--library] [--retired] [--json]` | Staff (or library) with id, title, from and status |
| `cos roles show <id> [--library] [--json]` | One role or entry in full. `--json` prints the role for editing |
| `cos roles new <library-id> [--id <id>] [--specialise] [--brief "…"] [--yes]` | Builds the role from the entry, optionally specialised, then prints the role and the diff. Writes on `--yes`, or after a typed `yes` on a terminal. Refuses without a terminal and without `--yes` |
| `cos roles new --blank <id> --title "…" --mandate "…" [--yes]` | A minimal role with `expects: diff` |
| `cos roles edit <id> --file <role.json> [--yes]` | Replaces the role with the file's role (validated). Prints the field diff first |
| `cos roles retire <id> [--yes]` / `cos roles restore <id>` | Flips `retired` |

Every write reads `(policy, digest)` first and passes the digest to `apply`.
The confirm prompt is the same shape as `cos ritual approve`.

### 7. Desktop

Tauri commands, each in a `blocking` thread like `agents_roles`:

- `agents_roles` gains `digest`, `retired` per entry, and `library` (entry
  id, version, title, summary, sources).
- `roles_library_entry(id) -> LibraryEntry`.
- `roles_specialise(business, id, brief) -> { role, rationale, diff }`. The
  `claude` child runs under `proc` with the run's deadline. `named(&s,
  &business)` guards it the way `ritual_approve` is guarded.
- `roles_save(business, change, digest) -> { digest }`. Here `change` is the
  serialised `Change`, routed to `roles::apply`.

UI (`app.js`, `index.html`; tokens only, every state through `stateView`):

- **Staff pane header:** a "New role" button opens the role dialog at the
  library step.
- **Role dialog**, three steps in one `<dialog>`:
  1. *Start from*: library cards (title, summary, sources as links that open
     in the browser) plus "Blank".
  2. *Specialise* (optional): a brief textarea and a "Specialise for
     <business>" button. While the run is going, a `stateView("reading")`
     and Cancel. After 6 s a slow state appears, and on error an error state
     with Try again. "Skip" takes the entry as is.
  3. *Review and save*: a form over the typed fields. Lists are textareas,
     one item per line. Every field the proposal changed against the entry
     carries a "changed" badge, and the rationale shows above the form.
     Contract fields are shown read-only with an "Edit contract" disclosure
     that makes them editable. Validation problems from `roles_save` render
     next to their field. Save writes and closes.
- **Role pane:** sections for Principles, Method, Never, Hand back and
  Escalates when, when present. Actions are Edit (the same dialog at step 3
  with the current role, plus "Start from a library entry") and Retire
  (confirm). The head shows neither.
- **Retired roles** sit in a collapsed "Retired" group under the grid, each
  with Restore. The hire dialog's role select leaves them out.
- A digest conflict shows "The staff changed since you opened this. Reload
  and re-apply?" with Reload. The form's content is kept in the dialog.

### 8. Errors

| Case | Behaviour |
|---|---|
| agents.json does not parse | No write. The message names the file and the parse error |
| Lock held | No write: "another edit is in progress" |
| Digest mismatch | No write: reload prompt |
| Validation problems | No write. Problems are returned per field |
| Specialise run fails or times out | No proposal, with the reason. The entry can still be used as is |
| Proposal fails its schema | Treated as a failed run. Nothing is merged |
| Hire names a retired role | Refused with a restore hint |

## Semantic ISA

### Scope

- **Agent:** the recruiter brain proposes a specialised role definition.
  The write path persists roles the user approved, and the hire preamble
  hands a role's text to a hired agent.
- **Environment:** `context/agents.json` (local file), the `claude` CLI
  (network to Anthropic), and a hired agent's worktree (through the
  preamble).
- **Harms, ranked:** (1) a model-authored check command or scope runs or
  widens what a hire may touch; (2) a write corrupts or clobbers the user's
  agents.json; (3) business data leaves the machine beyond what specialising
  needs; (4) a hired agent is steered outside its sandbox by role text.

### Side-effect inventory

| Capability | System | Reversible? | Who is affected if it misfires |
|---|---|---|---|
| Write `context/agents.json` | local file | Yes, but only by hand. The file is gitignored and not versioned, so `apply` keeps `agents.json.bak` (previous bytes) before each replace | The user: every later hire |
| Specialise run (`claude -p`) | Anthropic API | Sending can't be undone | The user's goal titles and repo names leave the machine |
| Role text in a hire's preamble | hired agent in its worktree | Yes (worktree, drafts only) | That hire's diff |
| `agents.json.lock` create/remove | local file | Yes | Concurrent edits |

### Instruction set

| Instruction | Core | Governance property | Sink? | Replaces capability |
|---|---|---|---|---|
| `LOAD(library_entry)` | Memory | Deterministic I/O, compiled-in data | no | — |
| `LOAD(policy, digest)` | Memory | Deterministic I/O | no | file read |
| `LOAD(specialise_context)` | Memory | Allowlisted fields only | no | state.json read |
| `TOOL_CALL(claude.specialise)` | Execution | Deterministic action, network | **yes** | claude -p |
| `GENERATE(proposal)` | Cognitive | Probabilistic output | no | — |
| `VERIFY(proposal_schema)` | Normative | Deterministic checkpoint | no | — |
| `VERIFY(user_approval)` | Normative | Human checkpoint | no | — |
| `VERIFY(policy_valid)` | Normative | Deterministic checkpoint | no | — |
| `STORE(agents.json)` | Execution | Deterministic action | **yes** | file write |
| `RENDER(preamble)` | Execution | Deterministic, to hired agent | **yes** | hire prompt |

### Bindings

```json
{ "instruction": "TOOL_CALL(claude.specialise)",
  "input_schema": { "type": "object", "additionalProperties": false,
    "required": ["entry_id", "business", "goals", "repos", "staff"],
    "properties": {
      "entry_id": { "type": "string", "pattern": "^[a-z][a-z0-9-]{1,39}$" },
      "business": { "type": "string", "maxLength": 80 },
      "goals": { "type": "array", "maxItems": 20, "items": { "type": "string", "maxLength": 200 } },
      "repos": { "type": "array", "maxItems": 30, "items": { "type": "object", "additionalProperties": false,
        "required": ["name"], "properties": { "name": { "type": "string" },
          "checks": { "type": "array", "items": { "type": "string" } },
          "scope": { "type": "array", "items": { "type": "string" } } } } },
      "staff": { "type": "array", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } },
      "brief": { "type": "string", "maxLength": 1000 } } },
  "output_schema": "roles::proposal_schema(): closed object of title, mandate, serves, sandbox, judged, done_when, principles[], method[], anti_patterns[], output_format, escalation[], rationale, each with the §3 caps" }

{ "instruction": "STORE(agents.json)",
  "input_schema": { "type": "object", "additionalProperties": false,
    "required": ["change", "read_digest"],
    "properties": {
      "change": { "oneOf": [
        { "type": "object", "required": ["add"], "properties": { "add": { "$ref": "Role" } }, "additionalProperties": false },
        { "type": "object", "required": ["replace"], "properties": { "replace": { "type": "object", "required": ["id", "role"] } }, "additionalProperties": false },
        { "type": "object", "required": ["retire"], "properties": { "retire": { "type": "string" } }, "additionalProperties": false },
        { "type": "object", "required": ["restore"], "properties": { "restore": { "type": "string" } }, "additionalProperties": false } ] },
      "read_digest": { "type": "string", "pattern": "^([0-9a-f]{64}|absent)$" } } },
  "output_schema": { "type": "object", "required": ["digest", "role"], "additionalProperties": false,
    "properties": { "digest": { "type": "string" }, "role": { "type": "string" } } } }

{ "instruction": "RENDER(preamble)",
  "input_schema": "a Role from a Policy that passed roles::validate at load (load-time validation added: an invalid role is dropped from the hireable set with a note)",
  "output_schema": { "type": "string" } }
```

### Taint

- **Sources:** `GENERATE(proposal)` (model output), the user's `brief` text,
  a hand-edited `agents.json`, and the `--file` role JSON.
- **Propagation:** a merged role is tainted in its descriptive fields when
  the proposal is. Contract fields come from the compiled-in library entry
  and are clean. A stored role keeps its taint through the STORE → LOAD
  round-trip. We treat every role text as tainted at RENDER, and its clean
  contract fields (checks, scope) as the boundary.
- **Sinks:** `TOOL_CALL(claude.specialise)`, `STORE(agents.json)`,
  `RENDER(preamble)`.

| Sink | Accepted input must be cleared by | Check |
|---|---|---|
| `TOOL_CALL(claude.specialise)` | `VERIFY(specialise_context)` in `roles::specialise_context(home, workspace)` | Built only from goal titles, repo names with policy checks and scope, staff ids and titles, and a brief of at most 1,000 chars. The type has no field for anything else. Run with `Toolset::None` and `--strict-mcp-config` |
| `STORE(agents.json)` | `VERIFY(user_approval)` and then `VERIFY(policy_valid)` in `roles::apply` | Approval: CLI `--yes` or a typed `yes`, desktop Save from the main window. Then lock held, digest equal, parse OK, head unchanged, `roles::validate` empty, round-trip equal |
| `RENDER(preamble)` | `VERIFY(policy_valid)` at load and `VERIFY(contract_from_library)` at merge | Load drops invalid roles. `merge` takes checks, scope, harness, model, repo, expects, output and work from the entry (or from the user's own edit), never from a proposal. The preamble's last sentence keeps the drafts-only boundary, and the worktree sandbox is unchanged |

### Violations

| Violation | Evidence recorded | Recovery |
|---|---|---|
| Proposal fails schema | Field and value in the run's audit line (`claude::audit_note`) | No proposal; the user may use the entry as is or retry |
| Proposal carries a contract field | Impossible by schema (`additionalProperties: false`), and `merge` ignores it if one gets through | Schema failure path |
| Write without approval | CLI refuses without `--yes` and without a terminal | User re-runs with approval |
| Policy invalid at write | Problems per field, returned | User fixes the field; nothing written |
| Stale digest | Both digests in the error | Reload and re-apply |
| Invalid role at load | `Policy::note` names role and problem | Role not hireable until fixed |

### ISA test matrix

| Sink | Tainted input refused | VERIFY-cleared input admitted |
|---|---|---|
| `claude.specialise` | A `SpecialiseContext` cannot hold mail/calendar fields (type). A test asserts the prompt from a home whose state has deliverables, commitments and journal contains none of their text, and that a brief over 1,000 chars is refused | Prompt contains the goal titles and repo names; `command_with` args contain `--tools "" --strict-mcp-config` |
| `STORE(agents.json)` | Each is refused and leaves bytes unchanged: a proposal JSON carrying `checks`, a role with `../x` scope, a second head, retiring the head, a stale digest, a held lock, an unparseable file | A valid Add, Replace, Retire and Restore each change exactly that role. Digest returned; `.bak` holds the previous bytes |
| `RENDER(preamble)` | A role with an over-cap list in agents.json is not hireable (`Policy::hire` refuses with the note) | A role with all five fields renders each section once, in order, the drafts-only sentence last |

Driven by stand-ins: library tests set `Bins { claude:
tests/fixtures/fake-claude.sh }` as `tests/sleep.rs` does. CLI and desktop
tests set `COS_CLAUDE`, which `Bins::default` reads the way `COS_HC` names a
stand-in Herdr (new). There is no live model in the suite.

## Alternatives

- **Markdown playbook field:** rejected by the user in favour of typed fields
  that validate and diff.
- **Library fetched from a URL at runtime:** a network read and a supply
  path for prompt text into every hire. Compiled-in data is reviewed in a PR.
- **Writing agents.json through `state::commit`:** that path is for
  `state.json`, with its revision and transition rules. agents.json has no
  revision field, so a digest check gives the same stale-write protection
  without a schema change.
- **Letting the proposal suggest checks:** useful, but it puts model text
  into a shell command. If wanted, a suggested check could later be shown
  as a hint the user types in themselves. Not in this change.
- **Exposing roles over MCP:** not asked for. The HTTP server stays
  read-only and stdio gains nothing here.

## Security and privacy

- **Read:** agents.json, the compiled-in library, and goal titles from
  state.json.
- **Stored:** agents.json and agents.json.bak under `context/` (gitignored).
- **Leaves the machine:** on Specialise only, the library entry, the
  business name, goal titles, repo names with their checks and scope, staff
  titles and the user's brief, sent to Anthropic through `claude`. No tool
  calls are possible.
- No secrets are involved. Library fixtures name no customer.

## Testing

- **Static:** `tests/roles.rs` loads every library file, which must parse,
  validate, meet the minimum counts, cite a source and be listed in the
  `include_str!` list. `tests/design.rs` gains nothing new: the existing
  gates cover the dialog's states.
- **Unit-level, in `tests/roles.rs`:** `validate` per rule; `merge` never
  takes a contract field; `preamble` order; `apply` refusals and admits from
  the ISA matrix on temp homes.
- **Integration:** `tests/roles_cli.rs` drives `cos roles` end to end on a
  temp `COS_HOME` with a fake `claude`: new from library, specialised, edit,
  retire, restore, hire refused for retired. Then `cos --dry-run agents
  spawn --role <new>` shows the structured preamble in the plan.
- **Desktop:** `apps/desktop/tests/ipc.rs` covers `roles_save` (add, conflict,
  invalid) and `roles_specialise` with a stand-in `claude`.
  `task test-e2e` runs a browser pass over `examples/preview.rs`: New role →
  library → skip → save → role appears → Retire → in Retired group.
- **WCAG:** `examples/wcag.rs` adds the role dialog's three steps.

## Confidence

| Piece | Tier | Moves it up |
|---|---|---|
| Schema + validation + write path | rock | — |
| Preamble rendering | rock | — |
| CLI | rock | — |
| Specialise run | rock | Same machinery as `ask`, no tools |
| Library content (12 entries) | sand | Each entry cites its source; a reviewer reads each against it; minimum counts gate the shape |
| Desktop dialog | sand | Three-step dialog is new UI; the e2e pass and WCAG run make it rock |
| Load-time validation dropping invalid roles | sand | Could hide a role a user hand-edited. The `note` must name it, and a test asserts the note |

## Acceptance

1. `cos roles list --library` prints 12 entries, each with a cited source.
2. `cos roles new appsec-reviewer --yes` on a home with no agents.json
   writes one containing the 10 template roles and `appsec-reviewer` with
   `from: "appsec-reviewer@1"`. `cos roles list` shows it.
3. `cos roles new sre --specialise --brief "…"` with the fake `claude`
   prints a diff of changed descriptive fields and writes nothing without
   `--yes`. With `--yes` it writes, and the role's `checks` equal the
   library entry's.
4. A hand-edited agents.json that does not parse: `cos roles retire
   reviewer --yes` refuses, and the file bytes are unchanged.
5. `cos roles retire reviewer --yes`, then `cos agents spawn --role reviewer
   …` is refused with the restore hint. `cos roles restore reviewer` makes
   it hireable again, and the ledger's earlier reviewer rows still show
   "Reviewer".
6. `cos --dry-run agents spawn --role appsec-reviewer …` prints the plan,
   which gains a `preamble` field (`hire::Plan`, new; today dry-run prints
   the plan without the prompt). That preamble has Principles, Method, Never, Hand back and Stop-and-leave
   sections, in that order, with the drafts-only sentence last.
7. In the desktop app: Staff → New role → pick "Application security
   reviewer" → Skip → Save. The card appears. Its pane shows the five
   sections. Retire moves it to Retired, and it is gone from the hire
   dialog's select.
8. `task test` passes.
