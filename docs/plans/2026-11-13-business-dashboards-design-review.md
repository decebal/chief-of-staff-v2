---
design: docs/plans/2026-11-13-business-dashboards-design.md
design-sha256: 392f535177abe0d4c7f63a98c754b175cf70a70c4d5a94139a8fee16b685c3a4
verdict: findings
---

# Design review: business sidebar and per-business dashboards

## Outcome

The design was reviewed by EGM in 13 rounds. No round came back clean.

- Rounds 2 to 13 each used five or six fresh-context reviewers: design, comprehension, structural, style, codebase-details, and from round 6 implementation-readiness.
- Blocker and major findings per round:
  - rounds 2 to 5: 35, 32, 27, 15;
  - rounds 6 to 9: 25, 27, 11, 15;
  - rounds 10 to 13: 18, 9, 14, 8.
- Every finding from round 13 (8 major, 19 nit) was fixed in the design. None was contested, declined or escalated.
- The fixed revision, the digest above, has not been reviewed since. The verdict is therefore `findings`, not `clean`.

The owner stopped the loop after round 13 (2026-11-15). The reason: findings had narrowed to ordering races between quit, spawn and probe. The ISA test matrix and the Build tests check those more reliably than further document review does.

The owner also declined to re-hire `ritual-goldfish` on this digest. The run on it (`20261115-084512-9084`) hit its $5 budget before writing a review.

## Review history

| Review | Digest | Verdict | File |
|---|---|---|---|
| EGM stages 5-7 | `a853bd18…` (superseded) | findings | `docs/archive/plans/2026-11-13-business-dashboards-egm-r1/goldfish_review_report.md` |
| Hired ritual-goldfish `20261113-225320-f142` | `a853bd18…` (superseded) | findings: 0 blocking, 2 should, 2 nit, all applied in round 5 follow-up | agent output `REVIEW.md` |
| EGM rounds 2-13 | successive revisions | findings | `docs/archive/plans/2026-11-13-business-dashboards-egm-r1/round_2_report.md` … `round_13_report.md` |

## Owner decisions taken during review

1. After a crash, the next launch stops the leftover server instead of reclaiming it.
2. A per-port `flock` replaces the recorded app pid. The lock is released when the child is reaped.
3. The header badge is removed. `BusinessRow` carries `unread` and `scheduled`.
4. After the server's leader is reaped, the process group gets one SIGKILL while `group_alive`. The quit path does this, and so does launch cleanup while it holds the port lock.
5. Launch cleanup stops only a leftover whose executable equals a binary declared for that port in `dashboards.toml`.
6. The Detailed Implementation Plan states contracts only. Internal types, derives and field layouts belong to Build.

## Carried into Build

1. should §4 lifecycle: the quit/spawn/probe ordering rules last changed in round 13 and have not been reviewed since. Evidence: `round_13_report.md`, rows on the spawn recorded after `stop_owned` returned and on quitting with a probing slot. Fix: each ISA matrix lifecycle row becomes a test in `apps/desktop/tests/servers.rs` before the supervisor code merges.
2. should Open Question Q1: the go/no-go spike for the loopback iframe, `__TAURI__` visibility, the `cosreport` POST Origin and the gazette frame headers runs before any embedding code. Evidence: design § Open Questions. Fix: run the spike first in Build and record the result in the design.
