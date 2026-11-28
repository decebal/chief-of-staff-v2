---
design: docs/plans/2026-11-25-mcp-client-bridge-design.md
design-sha256: c1a65677958bcc6f2773c25fe0f1d96b0933bd363442c0b02c8a3b0c711c0476
verdict: findings
---

## Earlier findings

Round-3 New findings (from `docs/archive/plans/2026-11-25-mcp-client-bridge-design-review-b-r3.md`):

1. closed: own group killed before final write. Self-cleanup kills every group in the set except the run's own, waits, writes the final state, then exits (design 108-112). Reaping writes only after the kills (design 153-159). Owner-death row exits 76 after the write (design 695). Code: `proc::kill_group` is `kill(-pgid, SIGKILL)` (`src/proc.rs:238-245`), so the design's "never the run's own group" rule is what keeps the run alive through the write.

2. closed: `run_stop` refused for non-daemon owners and the app's `RunChild`. The panel shows Stop only for a live run owned by `daemon`, or by `app` with the window holding its `RunChild` in managed state keyed by run id (design 399-403). `run_stop` refuses other owners with `not stoppable here: started by <owner>` (design 413-416). ISA admits and refuses per owner (design 690). Code: `Brief(Mutex<BriefRun>)` holds no child (`apps/desktop/src/lib.rs:1454-1468`), so the managed-state map is new and is named. Detail left open, see Carried.

3. closed for SIGKILL of the daemon: the daemon's own children (judge `sh -c` and `git -C` through `proc::run`, `src/agents/judge.rs:263`, `:301`; `src/agents/mod.rs:1075`, `:1125`) enter the in-process group set through `spawn_grouped` (`src/proc.rs:166-170`). The set is mirrored to `daemon.groups.json` and swept on restart under the identity check (design 196-198). ISA daemon-death row has the judge-style `sh` child (design 696). Not closed for SIGTERM / `bootout`, see Carried.

4. closed: run child dying by signal with owner alive. Reaping covers "normal, error, signal, SIGKILL" and writes `failed` with `signal <s>` (design 153-159). ISA run-child-death row: SIGKILL the stand-in run child, owner alive, grandchild group gone, `failed` (`signal 9`) (design 693). Code: `claude` runs through `stream_lines` → `spawn_grouped` (`src/claude.rs:198`, `src/proc.rs:130-137`), so the grandchild is in a group the owner can name.

## New findings

1. blocking §The invariant (design 142-143), §Run process 1-2 (design 82-91), §Taking the brief lock (167-173), §A step 2 (189-193), ISA run-child death and owner death (693, 695), Acceptance 7 (773-777).
   A `self`-owned `cos ask` has no reaper, no watchdog and no sweep. Design 60-62 and 270 give `cos ask` in a terminal owner `self`. `COS_OWNER` is set only for spawns by daemon, app or mcp (design 235-237), so a self-owned ask has no watchdog. The brief-lock sweep covers only `brief` (ask takes no lock, design 85-86). The daemon sweep covers only owner `daemon` (design 189-190). Readers never write (design 175-180). A `kill -9` of a terminal or CLI `cos ask` therefore leaves: the record `running` forever (readers show `lost`), and its `claude` group unkilled. The group ends only when claude next writes to its closed stdout pipe, or never if the child is hung (a check, a `git` wait). The design says "every path below holds it" (design 142-143) and Acceptance 7 says "kill -9 of the run process itself" with no owner qualifier. Evidence is the design text and the code paths above; I did not run it.
   Fix: every `cos brief` and `cos ask` start, and daemon start, sweeps that business's non-final records whose `pid`/`start` is dead (identity check): kill their groups, write `lost` with reason `previous run died` under the record lock, then do its own work. Add an ISA row: SIGKILL a self-owned `ask` stand-in; the next `ask` kills its group and the record reads `lost`. Or narrow the invariant and Acceptance 7 in writing to owners `daemon`, `app`, `mcp`. The first is the recommendation.

## Carried to Plan

- §A step 6 (design 208-210): daemon SIGTERM / `bootout` stops runs only. The daemon's own judge groups stay unowned until the next start. After `cos daemon uninstall` there is no next start, so a hung check runs on with no reaper. Add: SIGTERM also kills every group in the daemon's set. Item 3 is closed only for SIGKILL.
- §Reaping (design 155-157): "waits up to 2 s for `group_alive` to turn false". Say what happens if it stays true (write final anyway? log?). A zombie member of a live group also keeps `group_alive` true until reaped.
- Writers race on the same record. Lock sweep (`lost`, reason `previous run died`, design 167-172) and daemon Reaping (`failed`, `signal N`, design 153-159) can both act on one dead run. "Final states never change" picks the first writer, not a defined one. Name the winner or accept either.
- §Taking the brief lock (design 170): "kills that record's groups". Reaping also kills the run's own group (`pid`, design 155). Align.
- §Run process 1 (design 85-86) says `ask` takes no lock; §Clients (design 270) says `cos brief / cos ask` "take the brief lock". Fix the wording.
- §A step 2 (design 196-198): the gap between a daemon child's `spawn` and the `daemon.groups.json` mirror is not stated (the run gap is, design 98-100). State it. Also state that the sweep rewrites or clears `daemon.groups.json` after killing.
- `start` brief while the lock is held: the daemon replies with "the run id it holds" (design 220). The child writes that id after it takes the lock (design 84-86), so a start in between has no id to return. Say what the reply is then.
- App `run_stop` for an app-owned run: the `RunChild` is needed by two parties, the `brief_run` wait thread and the managed-state map (design 400-402, 413-416). Name the shared handle, and the Stop sequence (SIGTERM, 5 s, SIGKILL, Reaping) that the app runs on it. `RunChild::kill()` (design 227) is one step, not that sequence.
- ISA: no VERIFY row for the app's `run_stop` path (design 660 covers the daemon only). Add one for owner `app`.
- Acceptance 7 (design 773-777) tests the run's groups only. The daemon's judge groups after `bootout` (see first Carried item) have no acceptance line.
- Records with a dead owner and a live group: `Taking the brief lock` and daemon sweep both key on identity; a record whose `groups` is empty and whose own group is alive is covered only by Reaping. State that the own-group kill runs in every sweep, not only Reaping.

Counts: earlier 4: closed 4 (2 with Carried detail). New blocking 1. Carried 10.
