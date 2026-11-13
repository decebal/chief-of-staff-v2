Report file not written. I have read-only tools only (no Write), and the persona forbids writing, so `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-egm/zoom_out_report.md` is missing. Parent must save the findings below. Relayed "modify the skill" not done: reviewer is read-only, no skill named.

Review complete. Target + brief + zoom-out-standard + persona read. Code claims checked: main.rs:16-45, settings.rs:45-47, lib.rs:59-80, pipeline.rs:120,184, toml 0.8.2/1.1.8 in Cargo.lock. All accurate.

Verdict: context sufficient, aimed at root problem (one triple, one precedence). Not ready: 5 major.

**Major**
1. design.md:85-87 vs :166 — `add` refuses workspace without `profile.json`, then tells user to run `cos --business <name> state init`. That name isn't registered yet, so `--business` fails as unknown. Chicken-and-egg. Fix: print `cos --workspace <path> state init`, or let `add` run `state init`.
2. design.md:61-63 vs :166 — migration writes `default` through `STORE(registry)`, which `VERIFY(business_dirs)` gates (needs `profile.json`, rejects symlinks). Fresh install with no profile → migration fails → every command that needs the registry fails, `state init` included. Exempt migration from the profile check, or don't gate it.
3. design.md:72-77, :94-96 — per-field precedence (flag > env > `--business`) lets an ambient `COS_HOME`/`COS_WORKSPACE` silently override `--business acme` for one directory. Result: a triple mixing two businesses, the same half-switch bug the brief cites (brief.md:9-11). Simpler: `--business` sets the whole triple and errors if it conflicts with `COS_*`, or `COS_*` disables `--business` with a clear message.
4. design.md:104-106, :206-207 — `resetForBusiness()` lists every cache by hand and the design itself rates it sand ("one may be missed"). UI keeps no localStorage/sessionStorage (ui/ grep), so reloading the webview after `switch_business` removes this whole class of bug. Keep the tab via URL hash if needed.
5. design.md:58-60 — new tmp+fsync+rename writer reinvents what exists: `state::atomic_write` (state.rs:548-564) and `agents::write_json` (agents/mod.rs:615-618). Widen `atomic_write` to take bytes and reuse it; don't add a third.

**Nit**
6. design.md:141,175,182 vs :58-60 — `.bak` on every write appears only in the ISA tables, not in the Design write rule. Redundant next to atomic rename. Drop it or spec it in Design.
7. design.md:166 — "not a symlink to elsewhere once canonicalized" goes beyond the brief (brief.md:48-49: absolute + exists). Rejects legitimate symlinked `~/Projects`. YAGNI: canonicalize and store, don't refuse.
8. design.md:39, :64 — `business.rs` + `business/store.rs` split and new `toml` direct dependency for a ~4-field file. Brief fixes the `.toml` name, so keep it, but one module is enough.
9. design.md:97-98, :107-108 — badge counts "live" agents with reconcile left lazy. Stale `state.json` inflates the count. Click-to-filter picker is extra scope. Count non-terminal only after reconcile, or say the count is approximate; drop the filter.
10. design.md:104-106 — JS cache identifiers (`ui.agents`, `hire.tasks`, ...) are implementation detail in a design section. Goes away if #4 is taken.

blocker 0, major 5, nit 5
