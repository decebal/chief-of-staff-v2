# Business dashboards: Q1 spike record

Run 2026-11-16 for US-001 of the [plan](2026-11-15-business-dashboards-plan.md), against design Open Question Q1 of the [design](2026-11-13-business-dashboards-design.md#open-questions). This file holds the result because an edit to the design voids its approval (plan, Open questions).

Result: iframe embedding holds for the control room. Checks 1 and 2 pass, check 3 is recorded, and check 4 fails for the gazette, so the gazette opens in the browser (Alternative F) and the control room stays embedded.

## Setup

- App: branch `feat/business-dashboards` at `bb0e6af`, built and installed with `task desktop-app` (debug build, ad-hoc signed, `~/Applications/Chief of Staff.app`), launched with `open -a`.
- Spike changes, all reverted before this commit and the app rebuilt and reinstalled from the clean tree afterwards:
  - `apps/desktop/tauri.conf.json`: `frame-src` gained `http://127.0.0.1:*`, the entry design §5 adds.
  - `apps/desktop/ui/spike.js` plus a `<script>` tag in `ui/index.html`: a fixed overlay in the main window with three iframes (`http://127.0.0.1:4777/`, `http://127.0.0.1:4790/probe.html`, `http://127.0.0.1:8790/`) and an on-page log of each frame's `load` event and every `securitypolicyviolation` and `error` event in the parent.
  - `apps/desktop/src/lib.rs` `cosreport_response`: one `eprintln!` of method, path and `Origin` for every request; stderr captured with `open --stderr`.
- Control room: `portfolio-control-room serve 127.0.0.1:4777`, started by hand for the spike and stopped after it.
- Probe page: a throwaway std-only Rust listener on `127.0.0.1:4790` serving `/probe.html`, which evaluates the checks in the frame and reports each value back to the listener by same-origin `POST /report?<key>=<value>`; the listener logs every request line with its `Origin`, `Referer` and `Sec-Fetch-*` headers.
- No devtools are attached to the webview, so "console output" is the on-page log the parent wrote, read from a capture of the app window only.

## Control-room binary

Built with `cargo build --release --manifest-path ~/Projects/founder-mode/portfolio-control-room/control-room/Cargo.toml` (Setup Runbook), finished in 26.91s.

```
-rwxrwxr-x@ 1 decebaldobrica  staff  1431216 16 Nov 12:44 /Users/decebaldobrica/Projects/founder-mode/portfolio-control-room/target/release/portfolio-control-room
```

`curl -s http://127.0.0.1:4777/health` answered `{"status":"ok"}`. Its `GET /` headers carry no `X-Frame-Options` and no `frame-ancestors`:

```
HTTP/1.1 200 OK
Content-Type: text/html; charset=utf-8
Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data:; connect-src 'self'
Referrer-Policy: no-referrer
```

## Check 1: an `http://127.0.0.1` frame inside `tauri://localhost`

Capture of the app window (Docs view, overlay at bottom right): the control room frame renders its own page ("Wolven / portfolio, EVIDENCE OPERATIONS" header with the "Ready Play desk", "Sales room" and "Refresh view" links), and the probe frame renders its yellow result strip. The gazette frame is blank (check 4).

On-page console, as captured:

```
Q1 spike console (parent tauri://localhost#tab=docs&pane=staff)
load event: gazette http://127.0.0.1:8790/
load event: probe http://127.0.0.1:4790/probe.html
load event: control room http://127.0.0.1:4777/
```

No `securitypolicyviolation` and no `error` event was logged in the parent, and no mixed-content block appeared. The probe listener received the frame navigation (`GET /probe.html`, `Sec-Fetch-Dest: iframe`, `Sec-Fetch-Site: cross-site`) and the frame reported `location=http://127.0.0.1:4790/probe.html` and `ancestorOrigins=["tauri://localhost"]`.

Verdict: pass.

## Check 2: `typeof window.__TAURI__` inside the frame

Values the probe frame reported (listener log):

| Expression | Value |
| --- | --- |
| `typeof window.__TAURI__` | `undefined` |
| `typeof window.__TAURI_INTERNALS__` | `undefined` |
| `typeof window.ipc` | `undefined` |
| `typeof window.webkit.messageHandlers` | `object` |
| `window.top.location.href` | throws `SecurityError` |

`window.webkit.messageHandlers` exists in the frame, as WebKit exposes it to every frame; whether Tauri's `ipc` handler accepts a message from an `http://127.0.0.1` frame is decided by the capability file (design §5, `windows: ["main"]` with no remote URLs) and was not probed here.

Verdict: pass.

## Check 3: `Origin` of a frame POST to `cosreport://localhost/docs/organize`

The probe frame sent three POSTs with body `spike-not-json`, and none carried the app business in `x-cos-business`, so no write was possible past the Origin rule either. App stderr:

```
SPIKE cosreport POST /docs/organize origin=Some("http://127.0.0.1:4790")
cosreport: refused POST /docs/organize from origin Some("http://127.0.0.1:4790")
SPIKE cosreport POST /docs/organize origin=Some("http://127.0.0.1:4790")
cosreport: refused POST /docs/organize from origin Some("http://127.0.0.1:4790")
SPIKE cosreport POST /docs/organize origin=None
cosreport: refused POST /docs/organize from origin None
```

| Request from the frame | `Origin` received | Handler | `fetch` result in the frame |
| --- | --- | --- | --- |
| `POST`, `content-type: text/plain` | `http://127.0.0.1:4790` | 403 | `TypeError: Load failed` |
| `POST`, `content-type: application/json`, `x-cos-business: spike-no-such-business` | `http://127.0.0.1:4790` | 403 | `TypeError: Load failed` |
| `POST`, `mode: "no-cors"` | absent | 403 | status 0, type `opaque` |

WebKit sent no `OPTIONS` preflight to the scheme handler for the second request; the POST itself arrived. The header was never `null`. The no-cors POST arrives with no `Origin`, which the POST rule also refuses (design §5).

Verdict: recorded (does not gate). `origin_allowed` refuses all three.

## Check 4: gazette frame headers

`launchctl list com.longhand.chief-of-staff-dashboard` shows the gazette loaded (`"PID" = 5056`, `"LastExitStatus" = 0`, `node .../gazette-live-runtime/server.mjs 8790 ...`), so the check ran.

`curl -sI http://127.0.0.1:8790/`:

```
HTTP/1.1 405 Method Not Allowed
Content-Type: text/plain
Cache-Control: no-store
X-Content-Type-Options: nosniff
Referrer-Policy: no-referrer
Content-Security-Policy: default-src 'self' data:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'
```

The gazette answers HEAD with 405; `GET /` (`curl -s -D - -o /dev/null`) answers 200 with the same `Content-Security-Policy`:

| Header | Value |
| --- | --- |
| `X-Frame-Options` | absent |
| `frame-ancestors` | `'none'` |
| `Content-Security-Policy` | `default-src 'self' data:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'` |

`frame-ancestors 'none'` excludes `tauri://localhost`. In the app the gazette frame stayed blank while its `load` event still fired, so the `load` event alone cannot tell a framed page from a refused one.

Verdict: fail. Fallback: the gazette opens in the browser (Alternative F); the control room stays embedded. The gazette's `allow-forms` check (design §5) does not apply while it is not framed.

## Identity probe timing with a frame loaded

With the control room framed in the running app, the CLI's own probe (`cos business dashboards founding-bets`, one `PROBE` attempt, 3 s timeout, `src/dashboards.rs:137`), run from `target/debug/cos` against a scratch `COS_DASHBOARDS` file declaring the control room with probe `{ path = "/", contains = "<title>Wolven portfolio control room" }`:

```
founding-bets  Control room  http://127.0.0.1:4777/  control-room  answers
orphaned: founding-bets
real 0.66
```

A second run: `real 0.06`. Both figures are the whole command (process start and file loads included), so the probe itself took at most that. A bare `curl -0 -m 3 http://127.0.0.1:4777/` answered 200 in 0.000474s.

Verdict: answers within the 3 s probe timeout.

## Verdicts

- Verdict: check 1 pass.
- Verdict: check 2 pass.
- Verdict: check 3 recorded, `Origin: http://127.0.0.1:4790` on CORS POSTs and absent on a no-cors POST; all refused by `origin_allowed`.
- Verdict: check 4 fail for the gazette (`frame-ancestors 'none'`): the gazette opens in the browser (Alternative F); the control room has no frame restriction and stays embedded.
- Verdict: identity probe answers in 0.06s to 0.66s, within the 3 s timeout.
