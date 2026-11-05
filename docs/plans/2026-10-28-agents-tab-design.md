# Agents tab: a staff the chief of staff runs, and a design pack for it

Date: 2026-10-28. Status: built (eight roles, `expects` judging, a pre-hire fit check, Basecoat + Geist tokens, design gates in `tests/design.rs`). Web sources retrieved
2026-10-28.

The ask: the desktop Agents tab should show roles, the staff the chief of
staff is in charge of, and look closer to a real agent console. This doc
records what the tab shows today, proposes a role model built from what `cos`
already has, compares shadcn/ui, Vercel Geist and their vanilla ports for a
no-build Tauri webview, and sketches the redesign.

Source labels: `file:line` is read in this repo this session; a URL is a page
fetched on 2026-10-28; "inferred" is a conclusion from those; "not checked"
means nobody looked.

## 1. Current state

### The UI is vanilla, no build step

- Three files: `apps/desktop/ui/index.html` (109 lines), `app.js` (293),
  `style.css` (98) (observed, `wc -l`).
- `tauri.conf.json:7` sets `frontendDist: "ui"`; there is no
  `package.json`, bundler config or `node_modules` under `apps/desktop/`
  (observed, `find -maxdepth 2`). The page loads `style.css` and `app.js`
  directly (`index.html:7,107`) and reaches Rust through
  `window.__TAURI__.core.invoke` (`app.js:3`, `withGlobalTauri: true` at
  `tauri.conf.json:10`).
- Every backend value reaches the DOM through `textContent` or a property,
  never `innerHTML` (`app.js:1-2`, the `el()` helper at `app.js:11-15`).

### CSP and capabilities

- `tauri.conf.json:22`: `default-src 'self'; script-src 'self'; style-src
  'self' 'unsafe-inline'; img-src 'self' data:; frame-src cosreport:
  http://cosreport.localhost; connect-src ipc: http://ipc.localhost`.
- No `font-src`, so fonts fall back to `default-src 'self'`: a woff2 file
  under `ui/` loads, a remote font does not (inferred from the CSP
  fallback rules).
- `capabilities/default.json:6`: `core:default` only, main window only.
- Tauri appends nonces and hashes to the CSP for bundled assets at compile
  time, and a browser ignores `'unsafe-inline'` once a hash or nonce is in
  the same directive ([Tauri CSP](https://v2.tauri.app/security/csp/);
  the `'unsafe-inline'` interaction is reported in
  [GeoLibre #3212](https://github.com/opengeos/GeoLibre/pull/3212), read via
  a search summary, not the PR itself). `index.html` has no inline
  `<style>` today (observed), so this does not bite yet; it would if one were
  added.

### What the tab shows

| Area | Shows | Anchor |
|---|---|---|
| Spawn form | harness (Claude Code / Codex), repo, worktree toggle, title, prompt, copies, deadline, budget, model | `index.html:48-71` |
| From the latest run | agent prompts from the brief's actions or the week's goals, each with spawn / edit | `index.html:73-78`, `app.js:265-287` |
| Agents table | id, harness, status, title, tool calls, cost, latest line | `index.html:84`, `app.js:123-144` |
| Detail | status, workdir, repo, source, seeded, build slot, session, started, ended, resume command; error; seed log; activity tail; stop / clean | `index.html:90-103`, `app.js:155-197` |
| Header | `running/slots` and queued count | `app.js:31-37` |

### IPC commands

`paths`, `set_workspace`, `agents_list`, `agents_spawn`, `agents_stop`,
`agents_clean`, `agents_tail`, `agent_tasks`, `repos`, `docs_save`
(`lib.rs:349-360`). `agents_list` returns `{spec, state}` rows with the full
`Spec` and `State` serialized (`lib.rs:151-167`, `State` derives `Serialize`
at `src/agents/mod.rs:146`).

### The agent model in `src/agents/`

| Piece | What it holds | Anchor |
|---|---|---|
| `Spec` | id, harness, model, repo, base commit, workdir, title, source, prompt, deadline, budget, slots, contract, limits | `mod.rs:73-100` |
| `Status` | starting, queued, seeding, running, done, failed, killed, timed_out, lost | `mod.rs:102-118` |
| `State` | cost, tokens, tool calls, session, last line, summary, error, seeded, shimmed, verdict, live flags, flagged kinds | `mod.rs:145-182` |
| `Contract` | `done_when`, `checks`, `scope` | `judge.rs:21-33` |
| `Verdict` | outcome (accepted / needs_you / rejected), reasons, changed files, out_of_scope, per-check exit and tail, overclaimed | `judge.rs:35-76` |
| judge | rejected if not `done` or a check fails; needs_you if no checks, out of scope, or no diff against base; else accepted. 300 s budget for the whole verdict | `judge.rs:16, 117-152` |
| watch | `stalled` after `stall_secs` (default 600) of silence, `looping` after `repeat` (default 5) identical calls; flags, never stops | `watch.rs:18-25, 82-104` |
| policy | `context/agents.json`: `defaults` and per-repo `Terms` (checks, scope, deadline, budget, stall, repeat); `deny_unknown_fields` on both structs | `policy.rs:14-45` |
| ledger | one JSON line per judged agent in `out/agents/ledger.jsonl`, survives `clean`; row has `kind` = first word of `source` | `ledger.rs:17-69` |
| scorecard | grouped by harness, model, repo: runs, accepted, needs_you, rejected, overclaimed, flagged, median cost, median secs | `ledger.rs:101-156` |
| slots | run slots = `COS_AGENT_SLOTS` or cores / 4; one machine-wide build slot via a `cargo` shim | `slots.rs:18-33` |
| harness | Claude: `acceptEdits`, Seatbelt sandbox, strict network allowlist, no unsandboxed retry, `--max-budget-usd`. Codex: `exec --sandbox workspace-write` | `harness.rs:14-47` |
| handoff | the kit's nine-part handoff from a deliverable; `Receiver::Agent` says "sandboxed worktree, no network, stop before anything outward-facing" | `src/handoff.rs:13-19, 182-200` |

The template policy ships `defaults` (deadline 1800, stall 600, repeat 5) and
one repo entry with `checks: ["task test"]`, `scope: ["src/", "tests/"]`
(`templates/context/agents.json:1-13`).

### Data that exists but the tab does not show

Already in every `agents_list` row, never rendered (observed: `app.js`
reads none of these fields):

- the verdict: outcome, reasons, changed files, out-of-scope files, each
  check's command, exit and tail, `overclaimed`;
- live `flags` (stalled / looping with detail) and the `flagged` history;
- the contract: `done_when`, checks, scope;
- `limits`, `deadline_secs`, `budget_usd`, `model`, `base`, `tokens`,
  `summary`, `exit`.

Not reachable from the desktop at all:

- the scorecard and ledger: no IPC command reads them (observed, `grep
  scorecard|ledger apps/desktop` empty), while `cos dash` → `/agents` renders
  a scorecard table (`src/agents/view.rs:177, 267`) and MCP `agents` returns
  it (`src/mcp.rs:222-223`);
- the hiring policy in `context/agents.json`;
- spawn from a tracked deliverable: CLI only (`src/main.rs:145-147,
  349-375`); `agent_tasks` knows only `brief` and `goals` (`mod.rs:63-70`);
- re-judge: `agents::rejudge` (`mod.rs:426`) and `cos agents judge`
  (`main.rs:184`), no IPC;
- a contract typed in the window: `SpawnArgs` accepts `contract`
  (`lib.rs:186-188`) but the form never sends one (`app.js:238-250`), so
  every desktop spawn takes checks and scope from the policy alone.

Two small defects found while reading (not fixed here, outside this doc's
scope):

- `main.rs:365` reads a deliverable's `done_when`, but the kit's state uses
  `doneWhen` (`handoff.rs:176`; `assets/demo-state.json` has 1 `doneWhen`, 0
  `done_when`, observed with `grep -c`). Inferred: `--deliverable` spawns
  never carry the finish line into the contract. Not run.
- `main.rs:401` says `--from brief|review`; the enum is `brief|goals`
  (`mod.rs:67-70`).

## 2. Roles: the staff the chief of staff runs

### Constraints from the starter kit

These are the authority (`CLAUDE.md`, "The starter kit is the
authority"), and every role below inherits them:

- Brains are read-only; they recommend, the user commits (`CLAUDE.md`,
  "What the brains read"; `docs/design.md:86-110`).
- Drafts only: no sending, posting, scheduling, assigning, merging,
  deploying, spending or access changes (`handoff.rs:185-190`;
  `references/delegation.md:14`).
- A handoff transfers context, not authority (`delegation.md:3`;
  `handoff.rs:1-4`). A handoff for a completed item is refused
  (`handoff.rs:44-47`).
- Report back with an exact status: draft-ready, reviewed, implemented,
  deployed or accepted, and never change the source task yourself
  (`delegation.md:15`; `handoff.rs:202-208`).
- Agents work only in their own worktree; shell commands get no network
  (`docs/design.md:167-172`; `harness.rs:3-6`).

### The head: chief of staff

One brain exists in this repo: `.claude/agents/tactician.md`, described as
"The user's chief of staff" (`tactician.md:2-3`), run by `cos brief` and
`cos ask` (`CLAUDE.md` agent table). It has no file tools; context is inlined
(`tactician.md:19-21`; `docs/design.md:91-97`). A separate `strategist`
existed and is now removed at install (`Taskfile.yml:46` deletes
`~/.claude/agents/strategist.md`); inferred from `tactician.md:3`, which
covers "am I on track" and "what should I stop doing", that the strategic
horizon was folded into the tactician.

The chief of staff hires; it does not do the work. In `cos` terms it already
writes an `agent_prompt` per action and per week goal (`docs/design.md:152`,
`examples/demo.rs:218-279`), and every tracked deliverable has a handoff.
The proposal names who each prompt goes to.

### Proposed roles

"Exists" means the mechanism is in code today; "new" means it is not.

| Role | Mandate | Serves | Where it works, what it may touch | Contract (done when) | Judged by | Status |
|---|---|---|---|---|---|---|
| Chief of staff | the brief: steer per goal, at most three actions, handoffs | every goal | no file tools, approved connectors only | the brief validates against `reports.rs` schema | the user | exists (`tactician.md`) |
| Builder | implement a deliverable in one repo | deliverables with a code finish line | own detached worktree; scope from policy, e.g. `src/`, `tests/` | repo checks exit 0, diff in scope, non-empty | judge as today: checks + scope + diff | exists as the default agent; role name new |
| Reviewer | review one PR, draft comments with file and line | deliverables that unblock a goal (demo `D-1`, `demo.rs:218`) | worktree of the PR branch; writes only a review file | findings with file and line plus a verdict, in a named file | today: always `needs_you` ("no changes against its base commit", `judge.rs:144-146`); needs a file-output contract | prompts exist; judging new |
| Researcher | gather and chart, produce a one-page draft | goal-level questions (`demo.rs:226, 275`) | scratch directory, no repo (`app.js` repo `""`) | a draft file exists and is non-empty | today: `base` is `None`, so no diff check; with no checks it is `needs_you` | exists as scratch agent; contract new |
| Investigator | locate code, map a module, answer "where is X" | prep for a builder or reviewer | worktree, read-only by contract (`scope: []` plus a check that `git diff` is empty) | an answer file with `file:line` anchors | new check: diff empty, answer file present | new |
| Docs keeper | keep repo docs current with what shipped | deliverables whose finish line is a doc; the Docs room (`src/docs.rs`) | worktree, scope `docs/` and `README.md` | docs build or link check exits 0, diff inside `docs/` | judge as today | new role over existing mechanics |
| Release clerk | draft release notes, changelog, the checklist for a release | release deliverables | worktree, scope `CHANGELOG.md`, `docs/` | draft present; never tags, pushes or publishes | judge as today; outward steps stay with the user | new; "release/ops" reduced to drafts by the kit rules |
| Tester | draft the test; the user runs it | deliverables whose finish line is a test result | worktree, no network; writes `TEST-PLAN.md` only | `TEST-PLAN.md` holds the plan, a runnable script and an evidence checklist | `expects: file` plus `test -s` and an evidence grep; the user records the result on the run | built |

Before any spawn, the fit check (`src/agents/fit.rs`) matches the task's
words against the `fit` vocabulary in `context/agents.json` and answers
`fit`, `misfit` (naming the role that fits), `needs_you` (a need no sandbox
meets: network, a live system, unless the role `drafts` for it, as the tester
does) or `run_yourself` (an operation on `cos` itself).

A release or ops role that deploys, merges or tags conflicts with the kit's
draft-only rule (`handoff.rs:185-190`) and with the sandbox (`harness.rs:3-6`).
The table keeps the role and cuts its authority to drafts. Changing that is
the user's decision, not a design default.

### Where roles live: data, not binary

A role is hiring terms plus a prompt preamble, which is what `context/agents.json`
already holds per repo (`policy.rs:1-4` calls it "data, so changing how agents
are hired for a repo is an edit, not a rebuild"). Proposal:

```json
{
  "defaults": { "deadline_secs": 1800, "stall_secs": 600, "repeat": 5 },
  "roles": {
    "builder":  { "harness": "claude", "expects": "diff",
                  "mandate": "Implement the deliverable in this repo." },
    "reviewer": { "harness": "claude", "expects": "file", "output": "REVIEW.md",
                  "mandate": "Review the PR. Write findings with file:line to REVIEW.md.",
                  "checks": ["test -s REVIEW.md"] },
    "researcher": { "harness": "claude", "expects": "file", "output": "DRAFT.md",
                    "budget_usd": 2 }
  },
  "repos": { "example-repo": { "checks": ["task test"], "scope": ["src/", "tests/"] } }
}
```

What that needs in code (all new):

1. `Policy.roles: BTreeMap<String, Role>`; `deny_unknown_fields`
   (`policy.rs:15,34`) means today's binary rejects the key and hires on
   defaults with a note (`policy.rs:56-59`), so the schema change and the
   template change land together.
2. Precedence: request > repo > role > defaults, extending `Policy::terms`
   (`policy.rs:64-88`).
3. `Spec.role: Option<String>`, `ledger::Row.role`, and `scorecard` grouped by
   role first (`ledger.rs:118-129`). Rows written before the change have no
   role; the UI derives one from `kind` (`ledger.rs:55-59`: `deliverable`,
   `brief`, `goals`, `desktop`, `prompt`) and labels it "unassigned".
4. `Contract.expects: diff | file | none`. The judge's "no changes against its
   base commit" reason (`judge.rs:144-146`) applies only to `diff`; `file`
   requires the output path to exist and be non-empty; `none` requires an
   empty diff (investigator). This keeps the verdict computed from what the
   agent left, never its summary (`judge.rs:1-4`).

### Scorecard per role, from numbers that exist

Computable today from `ledger::Score` (`ledger.rs:103-115`) once grouped by
role: runs; acceptance rate = accepted / runs; needs-you rate; rejected;
overclaimed count (claimed success, a check failed); flagged count (stalled or
looping at least once); median cost; median seconds. Per role and harness/model
this answers "which hire does this job best" without a model call. Nothing in
it reads agent testimony (`ledger.rs:1-4`).

## 3. Design pack research

### shadcn/ui

- "Not a component library. It is how you build your component library":
  components are copied into your project through a CLI and a flat-file
  schema ([ui.shadcn.com/docs](https://ui.shadcn.com/docs), observed).
- MIT, about 126.8k stars; repo topics list react, radix-ui, base-ui,
  react-aria, tailwindcss ([github.com/shadcn-ui/ui](https://github.com/shadcn-ui/ui),
  observed).
- Theming is CSS variables under `:root` and `.dark`: `background`,
  `foreground`, `card`, `popover`, `primary`, `secondary`, `muted`,
  `accent`, `destructive`, `border`, `input`, `ring`, `chart-1..5`,
  `sidebar-*`, `radius`, each surface paired with `-foreground`; default
  values in `oklch()`; Tailwind maps them to utilities
  ([ui.shadcn.com/docs/theming](https://ui.shadcn.com/docs/theming),
  observed).
- Blocks include `dashboard-01` (sidebar, charts, data table) and several
  sidebars, all TSX for React frameworks
  ([ui.shadcn.com/blocks](https://ui.shadcn.com/blocks), observed).
- Fit here: the components need React and a Tailwind build (inferred from the
  above). This app has neither and a `script-src 'self'` CSP. The tokens
  are plain CSS and copy over as-is.

### Basecoat (vanilla port of shadcn/ui)

- "All of the shadcn/ui magic, none of the React"
  ([basecoatui.com](https://basecoatui.com/), observed). MIT, about 4.4k
  stars, vanilla JS, no framework runtime
  ([github.com/hunvreus/basecoat](https://github.com/hunvreus/basecoat),
  observed). npm `basecoat-css` 1.1.0, MIT (observed, registry
  `/basecoat-css/latest`).
- Prebuilt standalone CSS that "doesn't need a Tailwind build step", with
  named styles (vega, nova, maia, lyra, mira, luma, sera, rhea); most
  components CSS-only; JS per component loaded with `defer`, "no inline script
  is required" ([basecoatui.com/installation](https://basecoatui.com/installation/),
  observed).
- Components include Alert, Alert Dialog, Avatar, Badge, Card, Dialog, Empty,
  Item, Kbd, Progress, Scroll Area, Sidebar, Skeleton, Spinner, Table, Tabs,
  Toast, Tooltip, and form controls
  ([basecoatui.com/components/table](https://basecoatui.com/components/table/)
  sidebar, observed). Table is `class="table"` on a plain `<table>`.
- Measured, 1.1.0 from jsDelivr: `basecoat.cdn.min.css` 236,766 bytes
  (22,940 gzip -9); `js/all.min.js` 44,191 bytes (observed, `curl` and
  `gzip | wc -c`).
- The CSS defines the same token names as shadcn (`--background`,
  `--card`, `--primary`, `--sidebar-*` …) and only `data:` URLs, no remote
  `@import` (observed, `grep` on the file). Dark mode is a `.dark` class
  only; no `prefers-color-scheme` rule (observed). Today's `style.css:15`
  switches on `prefers-color-scheme`, so a 5-line `matchMedia` listener in
  `app.js` would toggle `.dark` (inferred).
- `all.min.js` contains no `eval` or `new Function` (observed, `grep -c` = 0)
  and uses `innerHTML` 4 times and `style.setProperty` once (observed). What
  those `innerHTML` calls receive was not audited. Using the CSS alone and
  keeping `app.js`'s `textContent` rule avoids the question.

### Vercel Geist

- The design system at [vercel.com/geist](https://vercel.com/geist/introduction)
  documents components as "Building blocks for React applications, available
  via `@vercel/geistcn`" (observed). That package is not on the public npm
  registry: `registry.npmjs.org/@vercel%2fgeistcn` returns 404 (observed,
  2026-10-28). Inferred: internal to Vercel; the docs are a reference, not an
  installable kit.
- `@geist-ui/*` is a community project, "not associated with Vercel", last
  `@geist-ui/react` release 2.2.5 five years ago (search summary of
  [npm @geist-ui/react](https://www.npmjs.com/package/@geist-ui/react), not
  fetched directly).
- Colours: 10 scales (backgrounds, gray, gray-alpha, blue, red, amber, green,
  teal, purple, pink), steps 100–1000 as `--ds-{scale}-{step}`. Use map:
  100 default background, 200 hover, 300 active; 400 border, 500 border hover,
  600 border active; 700 high-contrast background, 800 its hover; 900
  secondary text, 1000 primary text. P3 colours on supported displays
  ([vercel.com/geist/colors](https://vercel.com/geist/colors), observed). The
  page shows the naming and the step roles; per-step values were not copied.
- Geist font: SIL OFL 1.1; Sans and Mono in 9 weights, Pixel in 5 variants;
  OTF, WOFF2 and variable files
  ([vercel.com/font](https://vercel.com/font),
  [github.com/vercel/geist-font](https://github.com/vercel/geist-font),
  observed). npm `geist` 1.8.2 ships `dist/fonts/geist-sans/Geist-Variable.woff2`
  69,560 bytes and `geist-mono/GeistMono-Variable.woff2` 71,104 bytes
  (observed, jsDelivr file list). Two variable files cover every weight.

### Other close fits (brief)

- shadcn `dashboard-01` is the nearest published layout for a staff console
  (sidebar + KPI cards + data table), React only (observed, blocks page). Its
  layout can be copied in plain HTML with Basecoat classes (inferred).
- Linear-style and v0 dashboards: not researched this session.

### Comparison

| Option | Vanilla, no build | CSP (`script-src 'self'`, self-hosted) | Cost on disk | Dark / light | Accessibility | Licence | Tier |
|---|---|---|---|---|---|---|---|
| shadcn/ui components | no: React + Tailwind build | would need a bundler; Tailwind browser CDN is a remote script, blocked | not measured | `.dark` tokens | Radix / Base UI primitives (repo topics) | MIT | water: needs a framework decision first |
| shadcn tokens only, hand-written CSS | yes | yes | ~2 KB of variables (inferred) | `.dark` or media query | ours to build | MIT | rock |
| Basecoat CSS (+ optional JS) | yes, prebuilt file | yes, vendored under `ui/vendor/`, `defer` scripts | 236,766 B CSS, 44,191 B JS | `.dark` class | ARIA in its markup patterns (not audited) | MIT | rock |
| Geist components | no: React, package not public | n/a | n/a | yes | not checked | not stated | water: not installable |
| Geist font + colour steps | yes: two woff2 + CSS variables | yes, `font-src` falls back to `'self'` | 140,664 B fonts | Geist scales have light and dark values (docs page toggles both, not inspected per step) | high-contrast scale per the docs | OFL 1.1 (font) | rock |

### Recommendation

Vendor Basecoat's prebuilt CSS (one style, default `vega` or the darker
`nova`) under `apps/desktop/ui/vendor/`, self-host Geist Sans and Geist Mono
variable woff2, and keep `app.js` as is: no React, no Tailwind, no bundler,
no CSP change. Basecoat gives the shadcn look and its token names, and it
ships the components the redesign needs (sidebar, card, table, tabs, badge,
dialog, empty, progress, tooltip) as CSS classes on plain elements. Geist
supplies the type; its colour steps are optional and map onto the shadcn
tokens if the user prefers Vercel's palette.

Start CSS-only. Add Basecoat's per-component JS (tabs, toast, tooltip) only
where native HTML (`<dialog>`, `<details>`, radio-driven tabs) does not
cover it, after reading what its `innerHTML` calls receive.

What would change this:

- A decision to put the desktop UI on React for other reasons: then shadcn/ui
  proper, with Vite and a CSP review.
- The user wanting the report iframe (`src/report.html`) and the Docs room
  (`src/docs.html`) in the same look: those are separate pages on
  `cosreport://`; the same vendored CSS would have to be inlined into them,
  which changes the trade (not measured).
- Basecoat's CSS needing patches: then copy the shadcn tokens and write the
  ~15 classes the tab uses (the "tokens only" row).

## 4. Agents tab redesign

### Information architecture

```
Agents
├── Staff (default view)
│   ├── Chief of staff card: last brief, prompts handed out, open deliverables
│   └── Role cards: mandate, hired agents now, acceptance rate, median cost
├── Role detail
│   ├── Mandate and contract (expects, checks, scope, deadline, budget)
│   ├── Scorecard by harness/model for this role
│   ├── Live runs (table as today, filtered to the role)
│   ├── Recent verdicts (outcome, reasons, out-of-scope files, failed check tail)
│   └── Waiting work: deliverables and brief prompts routed to this role
├── Run detail (today's detail pane, plus verdict, flags, contract)
└── Hire (spawn by role): role → deliverable or prompt → contract preview → spawn
```

### Wireframe 1: Staff

```
┌ Chief of Staff ─ [Today] [Agents (3)] [Docs] ─────── workspace ~/ws  2/3 running ┐
│ ┌ Staff ┐ ┌ Runs ┐ ┌ Scorecard ┐                                     [+ Hire] │
│                                                                                │
│ ┌─ Chief of staff ───────────────────────────────────────────────────────────┐ │
│ │ tactician · brief 2026-10-28 · 3 actions · 3 prompts · 4 open deliverables │ │
│ └────────────────────────────────────────────────────────────────────────────┘ │
│ ┌─ Builder ─────────────┐ ┌─ Reviewer ────────────┐ ┌─ Researcher ──────────┐  │
│ │ Implement deliverables │ │ Review PRs, draft     │ │ One-page drafts       │  │
│ │ ● 1 running  ○ 1 queued│ │ ● 1 running           │ │ idle                  │  │
│ │ 12 runs · 75% accepted │ │ 5 runs · 0 accepted*  │ │ 3 runs · needs you 3  │  │
│ │ median $1.80 · 14 min  │ │ median $0.90 · 6 min  │ │ median $0.60 · 4 min  │  │
│ │ ⚑ 1 stalled            │ │ * judged as no-diff   │ │                       │  │
│ └────────────────────────┘ └───────────────────────┘ └───────────────────────┘  │
│ ┌─ Investigator ─┐ ┌─ Docs keeper ─┐ ┌─ Release clerk ─┐ ┌─ Unassigned ─┐       │
│ │ not hired yet  │ │ 2 runs        │ │ drafts only     │ │ 7 older runs │       │
│ └────────────────┘ └───────────────┘ └─────────────────┘ └──────────────┘       │
└────────────────────────────────────────────────────────────────────────────────┘
```

Numbers in the wireframe are placeholders, not data. Status sits on each
card beside its own role, never as a tab-wide banner. "not hired yet" is the
empty state: the role exists in policy and no agent ran under it.

### Wireframe 2: Role detail (Reviewer)

```
┌ ← Staff   Reviewer ─────────────────────────────────────────────── [Hire] ┐
│ Mandate  Review one PR; findings with file:line in REVIEW.md               │
│ Contract expects file REVIEW.md · check `test -s REVIEW.md` · scope none   │
│          deadline 1800 s · budget $5 · stall 600 s · repeat 5              │
│ ┌ Scorecard ─────────────────────────────────────────────────────────────┐ │
│ │ harness  model   repo         runs  acc  you  rej  over  flag  $med  s │ │
│ │ claude   sonnet  api-gateway     4    3    1    0     0     0  0.90 360│ │
│ │ codex    -       mobile          1    0    1    0     0     1     -  540│ │
│ └────────────────────────────────────────────────────────────────────────┘ │
│ Live           id        status   title                 tools  cost  latest│
│                a-7f2c    running  Review api-gateway#412  31  $0.44 tool Rd│
│ Recent verdicts                                                            │
│   a-61d0  accepted   1 check passed · REVIEW.md 2.1 KB                     │
│   a-5e9a  needs you  no checks in the contract: nothing proves it is done  │
│ Waiting   D-1 Review gateway 3.4 PR (G-1)            [hire for this]       │
└────────────────────────────────────────────────────────────────────────────┘
```

### Wireframe 3: Hire (dialog)

```
┌ Hire ───────────────────────────────────────────────────────┐
│ Role        [Reviewer ▾]                                    │
│ Work        (•) Deliverable [D-1 Review gateway 3.4 PR ▾]   │
│             ( ) Brief prompt [2 · Review mobile#288 ▾]      │
│             ( ) Own prompt                                  │
│ Harness     [Claude Code ▾]   Model [role default]          │
│ Repo        [api-gateway ▾]   [x] own worktree, seeded      │
│ Contract    done when  findings with file and line + verdict│
│             checks     test -s REVIEW.md          [edit]    │
│             scope      (none)                     [edit]    │
│ Limits      deadline 1800 s · budget $5 · copies 1          │
│ Permission  Sandboxed worktree, no network. Drafts only.    │
│             This hire grants nothing outside the worktree.  │
│                                   [Cancel]  [Hire]          │
└─────────────────────────────────────────────────────────────┘
```

The contract block is filled from policy and shown before spawning, so the
user sees what the agent will be judged on. "Permission" quotes
`handoff.rs:193-197` and is not editable.

### Mapping to IPC

| Screen element | Source today | New |
|---|---|---|
| Live runs, status, cost, latest | `agents_list` | filter by `spec.role` |
| Verdict, flags, contract in run detail | already in `agents_list` rows | render only |
| Scorecard | `ledger::read` + `ledger::scorecard` (`ledger.rs:87, 118`) | `agents_scorecard` command; `role` in the group key |
| Role cards, contract preview | `policy::load` (`policy.rs:51`) | `roles` key in `agents.json`; `agents_roles` command returning roles, merged terms, and `Policy::note` |
| Waiting deliverables | `state::find` / open deliverables (MCP `state` tool, `mcp.rs:162-174`) | `deliverables_open` command |
| Hire from deliverable | `handoff::deliverable(.., Receiver::Agent)` + CLI path `main.rs:349-375` | `SpawnArgs.deliverable`, `SpawnArgs.role`; read `doneWhen` |
| Contract typed in the form | `SpawnArgs.contract` (`lib.rs:186-188`) | send it from the form |
| Re-judge button | `agents::rejudge` (`mod.rs:426`) | `agents_rejudge` command |
| Chief of staff card | latest saved brief (`store.rs`) | `brief_summary` command (counts only) |

New `Spec`/`Row` fields: `role`. New `Contract` field: `expects` (+ `output`).
Everything else exists.

### Applying the tokens

- Replace the `:root` block in `style.css:1-27` with Basecoat's tokens (it
  ships them); map today's `--ok` / `--warn` / `--bad` onto Geist
  `green-900` / `amber-900` / `red-900` or keep them as three extra variables,
  since shadcn has only `destructive`.
- `font-family: "Geist", system-ui` and `"Geist Mono"` for ids, paths,
  `pre` and numeric table cells; `@font-face` pointing at `ui/fonts/*.woff2`.
- Status pills (`style.css:80-85`) become Basecoat `badge` variants; role
  cards are `card`; the roster/role split uses `sidebar` or `tabs`; tables
  keep `<table class="table">`.
- Keep every value through `textContent`; no inline `<style>` in
  `index.html` (see the CSP note in section 1).
- Screenshots for the PR come from `task screenshots` on the demo home only
  (`CLAUDE.md`, Layout).

### Confidence

| Slice | Tier | What would move it up |
|---|---|---|
| Re-skin: Basecoat CSS + Geist fonts, render verdict/flags/contract | rock | nothing; all data is in `agents_list` |
| Scorecard IPC and tab | rock | nothing; `ledger` functions exist |
| Roles in `agents.json`, `Spec.role`, scorecard by role | sand | the user confirming the role list and precedence |
| `expects: file / none` in the judge | sand | one reviewer and one researcher run on the demo home to see what they leave |
| Hire from deliverable in the window | rock once the `doneWhen` read is fixed | the fix |

## 5. Open questions for the user

1. Which roles do you want on day one? The table has eight; the minimum that
   matches today's prompts is builder, reviewer, researcher.
2. Release and ops: drafts only (notes, checklists), as the kit requires, or
   do you want an explicit, separately approved execution scope for one
   role? The second changes the kit's rules, not just this tab.
3. Palette: Basecoat's neutral shadcn tokens, or Geist's colour steps on top?
4. Should the Today report and the Docs room adopt the same look, or only the
   window shell and the Agents tab?
5. Reviewer prompts in the demo run `gh pr checkout` (`demo.rs:218`) while
   the Claude sandbox refuses network for shell commands
   (`harness.rs:14-16`). Inferred that those prompts fail inside a `cos`
   agent; not run. Should the reviewer's worktree be cut from the PR branch
   by `cos` before the agent starts?
