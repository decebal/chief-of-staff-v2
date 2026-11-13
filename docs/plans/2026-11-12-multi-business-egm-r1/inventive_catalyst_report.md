Review complete, but the report file was not written. This agent has only Read, Grep and Glob, and the persona is strictly read-only, so `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-egm/inventive_catalyst_report.md` does not exist. The caller must write the findings below to it. The relayed "modify the skill" request was also not done: no write tools here, and the computed task is review-only.

Read: persona `/Users/decebaldobrica/.claude/plugins/cache/egm/egm/0.1.1/agents/inventive_catalyst.md` (names no document standards), the full design, the full brief, plus code checks. Mode REVIEW. Persona allows only Major or Nit, so no blockers.

**F1. Major. Resolver precedence (design.md:67-81) and launchd jobs (design.md:11-12)**
- False compromise: each directory is resolved on its own (flag > env > --business > active), so a run can mix directories from two businesses.
- Evidence: `launchd/cos.brief.plist:14` and `launchd/cos.dash.plist:15` set only `COS_HOME`, with no `--business`. Under the new resolver, home comes from env but workspace and projects come from the registry's `active`. After the app switches to acme, the scheduled 08:30 brief runs on default's home with acme's workspace and repos. That is the same mixed-triple bug the brief blames on `set_workspace` (brief.md:9-11).
- Design gaps: says "take `--business` at launch" (design.md:11) but has no plist change and no test. Acceptance 5 (design.md:220) checks only that the command still runs, not that the triple is coherent.
- Fix (Segmentation, whole triple): resolve the triple as one unit. If any `COS_*` env var or directory flag is set, build the whole triple from flags and env plus the `Cli::ws()` fallback and ignore the registry, unless `--business` is given. Or map a `COS_HOME` that matches a registry entry's home to that business. Ship plists with `--business @BUSINESS@`. Add a resolver test: `COS_HOME` set and active ≠ that home gives an unmixed triple.
- Liabilities: you can no longer override one directory on top of a business without passing `--business`. One more install-time placeholder in the plists.

**F2. Major. Desktop `switch_business` writes the global `active` (design.md:99, 4, 18-19)**
- False compromise: what the window is showing and the default for every CLI and launchd process are the same piece of state. Browsing in the picker silently changes which business terminals and scheduled jobs run against.
- Fix (Segmentation): the app keeps its own current business, e.g. `business` in `settings.json` next to slots and cos, and only seeds it from `active` at startup. Only `cos business use` writes `active`. Or the picker offers "view" and "make default" as separate actions.
- Liabilities: the app and the CLI can now show different "current" businesses, so the header needs to say when the view differs from the default. One more field to migrate.

**F3. Major. Cache reset by hand-listed caches (design.md:104-106, 206-207: confidence "sand")**
- False compromise: accepts that "one may be missed until driven by hand". It tries to make a list of every cache complete instead of making that list unnecessary.
- Fix (Cheap Dispensables): on switch, reload the webview (`location.reload()` after `switch_business`). Or keep every per-business UI value under one `biz = {}` object that is replaced as a whole. Have the backend emit a `business-changed` event so the reset does not depend on the opener. The watcher already re-reads settings every 700 ms (`apps/desktop/src/lib.rs:481-498`) and will fire `report-changed` anyway, so a backend event fits the existing pattern.
- Liabilities: reload flashes the screen and drops tab and scroll state (persist the current tab in `sessionStorage`). The single-object approach means one refactor of `ui`, `hire` and related state in `app.js`.

**F4. Major. In-flight responses after a switch (design.md:104-110)**
- False compromise: each call reads the current settings (`apps/desktop/src/lib.rs:26-28`, `app.get()` clones), which allows switching without a restart but gives no ordering. A `refreshAgents`, `refreshStaff`, `loadRepos` or report fetch started before the switch can resolve after `resetForBusiness()` and draw the old business's data into the new view. Listeners at `ui/app.js:1103-1112` add more races. The design covers only briefs and asks still running (design.md:109-110), not reads.
- Fix (Intermediary, generation stamp): `switch_business` bumps an epoch. Every view-loading command returns `{epoch, data}`, or the UI captures the epoch before `invoke`. The UI drops responses whose epoch is stale. F3's webview reload removes this class of bug for free.
- Liabilities: every read command's response shape changes (IPC tests in `apps/desktop/tests/ipc.rs` change too). Minor bookkeeping.

**F5. Nit. Agents badge (design.md:97-98, 107-108, 208-209: "sand")**
- False compromise: treats cross-business awareness as needing a directory scan per business on each call. `agents::list` runs `ps` for each live agent, which the watcher comment already calls "too dear per tick" (`apps/desktop/src/lib.rs:478-480`).
- Fix (Preliminary Action plus Inversion): extend the existing `watch` loop to `cos::dash::stamp` every registry home's `agents::root`, which reads metadata only. Recount only the home whose stamp moved and push the counts in an event. The badge reads a cached number.
- Liabilities: the watcher's per-tick cost grows with the number of businesses, and the count can be up to 700 ms stale.

**F6. Nit. Registry writes (design.md:58, 61-63, 141)**
- False compromise: tmp + fsync + rename prevents torn files but not lost updates. A CLI `add` racing the app's `switch_business` read-modify-write can drop the new business. Two first-run processes (launchd plus app) can both migrate, from different triples.
- Fix (Segmentation): keep `active` in its own small file so the frequent writer never rewrites the business list. Take an advisory `flock` on `businesses.toml.lock` around read-modify-write and migration, and re-check that the file is still absent while holding the lock.
- Liabilities: two files to keep consistent, and a lock file that can go stale (flock is released when the process exits).

findings: blocker 0, major 4, nit 2
