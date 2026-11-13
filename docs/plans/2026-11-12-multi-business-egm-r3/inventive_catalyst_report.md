**Inventive Catalyst Goldfish: REVIEW of `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md`** (Stage 5)

Read: persona file, target design, brief. The persona names no document standard, so none read. Claims checked against code. Core architecture is sound: one resolver picks one layer, so the triple never mixes sources; registry writes use flock and atomic write; ⌘K comes from a menu accelerator so it works inside the frames; reload drops every JS cache at once. These resolve real contradictions and do not hide compromises. Findings below are local.

---

**1. Severity: Major**
- **Affected Scope:** Context and "Renames taken first"; the `Triple { home, workspace, projects }` struct.
- **False Compromise:** The design accepts that a home and a workspace are both raw `PathBuf`. It guards against swapping them with renames (`pipeline.rs:119-120, 183-184`, `track.rs`, `context.rs`, `main.rs:775-778`) plus one characterisation test. Names don't enforce anything. Every new call site that threads a business through can still pass `triple.home` where a workspace belongs, and the compiler won't object. The brief's own risk ("names invite routing the wrong directory") stays open for every future change.
- **Proposed Resolution:** Segmentation by type. Add newtypes `Home(PathBuf)`, `Workspace(PathBuf)`, `Projects(PathBuf)` in `src/business.rs`. Make `Triple`'s fields those types, and have the renamed functions take `&Workspace` / `&Home`. A swap becomes a compile error. The characterisation test stays as a backstop.
- **Secondary Liabilities:** Signature churn across pipeline, track, context, main and the desktop crate. `.as_path()` / `Deref` boilerplate at every fs call. Some serde work for the TOML registry (`#[serde(transparent)]`).

**2. Severity: Major**
- **Affected Scope:** Desktop badge, `businesses()`, `agents::count`; Risks "Badge cost"; Confidence "Badge: sand".
- **False Compromise:** The design accepts that picker-open latency grows with total run history across all businesses: one record read per run, and finished runs are never pruned. It treats this as something to measure rather than remove. Opening the picker is also tied to computing the counts ("computed on picker open"). So a slow count makes ⌘K itself feel slow, and the 200 ms gate may never pass on a machine with a long history.
- **Proposed Resolution:**
  - (a) Inversion. Render the picker list from the registry at once and fill in `{running, lost}` asynchronously per business. The picker no longer waits on the count.
  - (b) Preliminary Action. Let `count` skip terminal runs cheaply. Options: a per-home live index (a marker `out/agents/.live/<id>` written at spawn and removed in `finish`), or probing held `out/agents/.slots/run-*.lock` with `LOCK_NB`. A held slot means a live supervisor (`supervise.rs:55-58`), and the kernel frees the lock when the supervisor dies. Cost then scales with live runs, not history.
- **Secondary Liabilities:**
  - (a) The badge can be briefly stale or empty, and the async path needs a cancel when the picker closes.
  - (b) The marker index can drift if a supervisor dies between spawn and its first write, so `classify` stays the source of truth for marked runs. Slot probing takes the lock for a moment and can delay a queued supervisor by one `TICK`. Slot probing also can't see `starting` runs (no slot held yet) or `lost` runs, so the lost count still needs a record read, limited to marked runs.

**3. Severity: Nit**
- **Affected Scope:** Terms "Run record"; Desktop badge; Departures ("counts `running` runs only").
- **False Compromise / factual:** The status list is wrong against the code. `src/agents/mod.rs:110-124` has `Starting, Queued, Seeding, Running, Done, Failed, Killed, TimedOut, Lost`. There is no `stopped`; `Killed` and `TimedOut` are missing. Counting only `running` also hides runs that are `queued`, `starting` or `seeding` in other businesses, which still hold or wait for slots. So "badge vs. accuracy" is a self-made trade-off.
- **Proposed Resolution:** Fix the enum list. Have the badge count non-terminal (`!terminal()`, `mod.rs:127-131`) as "active", with `lost` kept separate.
- **Secondary Liabilities:** The badge wording changes ("active" instead of "running"), and Acceptance 5's text needs updating.

**4. Severity: Nit**
- **Affected Scope:** Risks "Orphaned harness in an unopened business"; `classify`/repair split.
- **False Compromise:** The design accepts that cleanup happens only when the user opens that business, so `businesses()` stays strictly read-only. That limit is partly self-imposed. The harness's stdout is piped to the supervisor (`supervise.rs:80, 100`), so it gets EPIPE once the supervisor dies. But grandchildren in the process group (builds, tools) can keep running and burning resources until someone opens that home.
- **Proposed Resolution:** Self-Service. Let `count` run the existing idempotent repair for runs that `classify` marks `Lost`, or add an explicit "clean up" action on the badge's lost entry. The other option is a lifeline: the supervisor holds a pipe, and a tiny watcher in the harness group kills the group on EOF.
- **Secondary Liabilities:** Opening the picker gains side effects (kills, record writes) in businesses the user isn't looking at, and the ISA matrix row "byte-identical, no process killed" must be rewritten. A lifeline watcher adds one process per run and needs its own reaping.

**5. Severity: Nit**
- **Affected Scope:** Desktop pin; Risks "Env pin".
- **False Compromise:** The design trades away switching entirely to honour `COS_*` at start. Spawn already strips `COS_*` from the child's env (Desktop, last bullet), and the triple lives only in app memory. So after start, the pin protects nothing except the user's original intent.
- **Proposed Resolution:** Beforehand Cushioning. The picker offers "switch anyway (ignores COS_HOME for this window)" behind a confirmation, and the header keeps saying the env was overridden.
- **Secondary Liabilities:** One more UI state to test. A user could forget the window no longer matches their shell env.

**6. Severity: Nit**
- **Affected Scope:** Departures ("`add` must name existing directories"); CLI `add`.
- **False Compromise:** Ordering is "register first, or init the profile first", and the design pushes that onto the user as a separate `cos --workspace … state init` step. That splits the brief's `cos --business x state init` assumption into two commands.
- **Proposed Resolution:** Preliminary Action. `cos business add … --init` runs `state init` on the workspace inside the same locked write, before the registry entry is committed.
- **Secondary Liabilities:** `add` now writes into the workspace as well as the registry, so the side-effect inventory and ISA rows grow. A failed init must leave the registry byte-identical.

**7. Severity: Nit**
- **Affected Scope:** Registry "Write".
- **Claim gap:** "The lock helper in `src/agents/slots.rs`" is a private `fn flock(file, nonblocking) -> bool` (`slots.rs:44-48`) with no path- or guard-returning API. `state::atomic_write` takes a `&Value` and writes JSON (`state.rs:548-564`), and its temp file is named `.state-*.tmp`.
- **Proposed Resolution:** State that both helpers get widened: `flock` becomes a public path-to-guard helper, and `atomic_write` takes bytes and a temp prefix.
- **Secondary Liabilities:** Small API surface added to `slots.rs` and `state.rs`.

---

Findings: 0 Blocker, 2 Major, 5 Nit.
