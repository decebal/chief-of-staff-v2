//! The window's `el()` door and the kit builders in headless Chrome, on the
//! window's own scripts. Run with `COS_E2E_CHROME=<Chrome binary>` and
//! `--include-ignored` (`task test-e2e`).

#[allow(dead_code)]
#[path = "common/cdp.rs"]
mod cdp;

use cdp::Chrome;
use serde_json::{Value, json};

const UI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/apps/desktop/ui");

/// A page that loads the window's real scripts, in the order `index.html` does.
fn open(name: &str) -> Chrome {
    cdp::chrome_path();
    let dir = cdp::profile(name);
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("page.html");
    let scripts: String = [
        "vendor/lucide/icons-0.556.0.js",
        "art/art.js",
        "states.js",
        "kit.js",
    ]
    .iter()
    .map(|s| format!(r#"<script src="file://{UI}/{s}"></script>"#))
    .collect();
    std::fs::write(
        &html,
        format!(r#"<!doctype html><meta charset="utf-8"><body>{scripts}</body>"#),
    )
    .unwrap();
    let mut c = Chrome::start(&dir, &["--disable-gpu"]);
    c.open(&format!("file://{}", html.display()));
    c.wait_for("typeof el === 'function' && typeof safeFrameSrc === 'function'");
    c
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn el_door_refuses_what_is_not_on_the_allowlist() {
    let mut c = open("kit-el-door");
    let v: Value = c.eval(
        r#"(() => {
          const threw = (f) => { try { f(); return false; } catch (e) { return e instanceof TypeError; } };
          const a = el("a", { href: "javascript:alert(1)", textContent: "x" });
          const ok = el("a", { href: "https://example.com/x" });
          return {
            jsHrefSet: a.hasAttribute("href"),
            httpsHref: ok.getAttribute("href"),
            src: threw(() => el("img", { src: "x.png" })),
            formAction: threw(() => el("form", { formAction: "x" })),
            innerHTML: threw(() => el("div", { innerHTML: "<b>x</b>" })),
            style: threw(() => el("div", { style: "color: red" })),
            nonFunctionOn: threw(() => el("button", { onClick: "alert(1)" })),
            computedKey: threw(() => el("div", { ["data-x"]: "1" })),
            functionOn: threw(() => el("button", { onClick: () => {} })),
            markupText: (() => { const p = el("p", {}, "<b>x</b>"); return p.querySelector("b") === null && p.textContent === "<b>x</b>"; })(),
            frameJs: safeFrameSrc("javascript:alert(1)"),
            frameWeb: safeFrameSrc("https://example.com/"),
            frameReport: safeFrameSrc("cosreport://localhost/flows"),
            frameLoopback: safeFrameSrc("http://127.0.0.1:3900/"),
          };
        })()"#,
    );
    assert_eq!(
        v["jsHrefSet"],
        json!(false),
        "javascript: href was set: {v}"
    );
    assert_eq!(v["httpsHref"], json!("https://example.com/x"), "{v}");
    for k in [
        "src",
        "formAction",
        "innerHTML",
        "style",
        "nonFunctionOn",
        "computedKey",
    ] {
        assert_eq!(v[k], json!(true), "{k} did not throw: {v}");
    }
    assert_eq!(v["functionOn"], json!(false), "{v}");
    assert_eq!(
        v["markupText"],
        json!(true),
        "markup did not render as text: {v}"
    );
    assert_eq!(v["frameJs"], Value::Null, "{v}");
    assert_eq!(v["frameWeb"], Value::Null, "{v}");
    assert!(v["frameReport"].is_string(), "{v}");
    assert!(v["frameLoopback"].is_string(), "{v}");
}

/// One real key press on the focused element, as a person's keyboard sends it.
fn press(c: &mut Chrome, key: &str, code: &str, vk: u32, text: &str) {
    c.call(
        "Input.dispatchKeyEvent",
        json!({ "type": "keyDown", "key": key, "code": code, "windowsVirtualKeyCode": vk, "text": text }),
    );
    c.call(
        "Input.dispatchKeyEvent",
        json!({ "type": "keyUp", "key": key, "code": code, "windowsVirtualKeyCode": vk }),
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn fleet_renders_one_card_per_hire_moves_focus_and_opens_a_run() {
    let mut c = open("kit-fleet");
    c.eval(
        r#"(() => {
          window.__opened = [];
          const rows = [
            { id: "a", title: "Brief the board", roleName: "Builder", identity: 2, state: "working", latest: "Editing kit.js", activeMs: 61000, live: true, paused: false, doneToday: false },
            { id: "b", title: "Review it", roleName: "Builder", identity: 2, state: "needs-you", latest: "Waiting on you", activeMs: 9000, live: true, paused: true, doneToday: false },
            { id: "c", title: "Old one", roleName: "Reviewer", identity: 4, state: "failed", latest: "", activeMs: 5000, live: false, paused: false, doneToday: false },
          ];
          const onOpen = (id) => window.__opened.push(id);
          document.body.append(fleet(rows, { onOpen, onReply: onOpen, onOpenHerdr: onOpen, onOpenRun: onOpen }));
          document.querySelector(".kit-fleet__open").focus();
          return true;
        })()"#,
    );
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-fleet__card').length"),
        json!(3),
        "one card per hire"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-id-2').length"),
        json!(2),
        "one role, one identity"
    );

    let focused =
        "[...document.querySelectorAll('.kit-fleet__open')].indexOf(document.activeElement)";
    press(&mut c, "ArrowDown", "ArrowDown", 40, "");
    assert_eq!(c.eval(focused), json!(1), "arrow down moves focus");
    press(&mut c, "End", "End", 35, "");
    assert_eq!(
        c.eval(focused),
        json!(2),
        "End moves focus to the last card"
    );
    press(&mut c, "Home", "Home", 36, "");
    assert_eq!(
        c.eval(focused),
        json!(0),
        "Home moves focus to the first card"
    );

    press(&mut c, "Enter", "Enter", 13, "\r");
    assert_eq!(
        c.eval("window.__opened"),
        json!(["a"]),
        "Enter opens the run"
    );

    assert_eq!(
        c.eval("document.querySelectorAll('[data-clock-live]').length"),
        json!(1),
        "one live, unpaused clock"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-fleet__card time')[0].textContent"),
        json!("1:01")
    );
    c.pause(std::time::Duration::from_millis(1200));
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-fleet__card time')[0].textContent"),
        json!("1:02"),
        "a live clock advances once a second"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-fleet__card time')[1].textContent"),
        json!("0:09"),
        "a held clock stays still"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn fleet_keeps_keyboard_focus_on_the_same_card_across_a_refresh() {
    let mut c = open("kit-fleet-focus");
    c.eval(
        r#"(() => {
          const row = (id, state) => ({ id, title: "Hire " + id, roleName: "Builder", identity: 2, state, latest: "", activeMs: 0, live: false, paused: false, doneToday: false });
          window.__refresh = (focusId) => {
            document.body.replaceChildren(fleet([row("a", "working"), row("b", "working"), row("c", "done")], { onOpen() {}, onReply() {}, onOpenHerdr() {}, onOpenRun() {}, focusId }));
          };
          window.__refresh(null);
          document.querySelectorAll('.kit-fleet__open')[1].focus();
          window.__refresh("b");
          return true;
        })()"#,
    );
    c.wait_for("document.activeElement?.dataset.id === 'b'");
    assert_eq!(
        c.eval("document.activeElement.textContent"),
        json!("Hire b"),
        "the refreshed fleet focuses the same hire"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-fleet__open[tabindex=\"0\"]').length"),
        json!(1),
        "one card stays in the tab order"
    );
    press(&mut c, "ArrowDown", "ArrowDown", 40, "");
    assert_eq!(
        c.eval("document.activeElement.dataset.id"),
        json!("c"),
        "arrow keys still move from the focused card"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn run_card_says_the_verdict_once_judged() {
    let mut c = open("kit-run-verdict");
    run_panel(
        &mut c,
        r#"{ phase: "complete", activeMs: 60000, live: false, paused: false, line: "", verdict: "Accepted: every check passed, and the work stayed inside its scope." }"#,
        RUN_TL,
    );
    assert_eq!(
        c.eval("document.querySelector('#panel .kit-run__verdict')?.textContent ?? null"),
        json!("Accepted: every check passed, and the work stayed inside its scope."),
    );
    let mut c = open("kit-run-unjudged");
    run_panel(
        &mut c,
        r#"{ phase: "active", activeMs: 60000, live: true, paused: false, line: "Editing kit.js" }"#,
        RUN_TL,
    );
    assert_eq!(
        c.eval("document.querySelector('#panel .kit-run__verdict')"),
        Value::Null,
        "no verdict line before judging"
    );
}

const RUN_TL: &str = r#"{
  spans: [
    { mode: "working", word: "working", turn: 0, from: "2030-01-01T00:00:01Z", to: "2030-01-01T00:01:01Z", ms: 60000 },
    { mode: "waiting", word: "idle", turn: 1, from: "2030-01-01T00:01:01Z", to: "2030-01-01T00:01:31Z", ms: 30000 },
    { mode: "paused", word: null, turn: 1, from: "2030-01-01T00:01:31Z", to: "2030-01-01T00:02:01Z", ms: 30000 },
  ],
  handoffs: [{ at: "2030-01-01T00:01:01Z", kind: "said" }],
  marks: [{ at: "2030-01-01T00:00:00Z", kind: "hired" }],
  summary: { active_ms: 60000, paused_ms: 30000, paused_now: false },
  notes: [],
}"#;

/// A run card and timeline over `tl`, built the way app.js builds them.
fn run_panel(c: &mut Chrome, view: &str, tl: &str) {
    c.eval(&format!(
        r#"(() => {{
          window.__acts = [];
          const tl = {tl};
          const onAction = (k) => window.__acts.push(k);
          document.body.append(el("div", {{ id: "panel" }}, runPanel({view}, () => tl, {{ onAction }})));
          return true;
        }})()"#
    ));
    c.wait_for("document.querySelector('#panel .kit-tl') !== null");
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn run_phase_follows_the_designs_mapping() {
    let mut c = open("kit-run-phase");
    let v = c.eval(
        r#"(() => {
          const r = (status, extra = {}) => runPhase({ state: { status, ...extra.state }, paused_now: !!extra.paused });
          return {
            queued: r("queued"), seeding: r("seeding"),
            working: r("running", { state: { herdr: { word: "working" } } }),
            held: r("running", { paused: true, state: { herdr: { word: "working" } } }),
            needsYou: r("running", { state: { herdr: { word: "blocked", turn: 2 } } }),
            firstTurnIdle: r("running", { state: { herdr: { word: "idle", turn: 0 } } }),
            idleAfterTurn: r("running", { state: { herdr: { word: "idle", turn: 1 } } }),
            done: r("done"), failed: r("failed"), lost: r("lost"), timedOut: r("timed_out"), killed: r("killed"),
          };
        })()"#,
    );
    assert_eq!(
        v["idleAfterTurn"],
        json!("needs_you"),
        "idle after Herdr's first turn: {v}"
    );
    assert_eq!(v["queued"], json!("standby"), "{v}");
    assert_eq!(v["seeding"], json!("standby"), "{v}");
    assert_eq!(v["working"], json!("active"), "{v}");
    assert_eq!(v["held"], json!("held"), "{v}");
    assert_eq!(v["needsYou"], json!("needs_you"), "{v}");
    assert_eq!(v["firstTurnIdle"], json!("active"), "{v}");
    assert_eq!(v["done"], json!("complete"), "{v}");
    assert_eq!(v["failed"], json!("errored"), "{v}");
    assert_eq!(v["lost"], json!("errored"), "{v}");
    assert_eq!(v["timedOut"], json!("errored"), "{v}");
    assert_eq!(v["killed"], json!("stopped"), "{v}");
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn run_card_footer_actions_follow_the_phase_and_spans_and_hand_offs_are_drawn() {
    let mut c = open("kit-run-card");
    run_panel(
        &mut c,
        r#"{ phase: "active", activeMs: 60000, live: true, paused: false, line: "Editing kit.js" }"#,
        RUN_TL,
    );
    assert_eq!(
        c.eval(
            "[...document.querySelectorAll('#panel .kit-run__foot button')].map(b => b.textContent)"
        ),
        json!(["Pause", "Mark done"]),
        "active offers Pause and Mark done"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#panel .kit-tl__span').length"),
        json!(3)
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#panel .kit-tl__tick').length"),
        json!(1),
        "one hand-off"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#panel .kit-tl__playhead').length"),
        json!(1),
        "live run has a playhead"
    );

    c.eval("document.querySelector('#panel .kit-tl__span[data-mode=working]').focus(); true");
    press(&mut c, "ArrowRight", "ArrowRight", 39, "");
    let detail = c.eval("document.querySelector('#panel .kit-tl__detail').textContent");
    let detail = detail.as_str().unwrap_or_default();
    assert!(detail.contains("Waiting"), "{detail}");
    assert!(detail.contains("length 30 s"), "{detail}");
    assert!(detail.contains("word idle"), "{detail}");
    assert!(detail.contains("turn 1"), "{detail}");
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn a_legacy_run_and_a_prep_failure_say_what_the_log_cannot_show() {
    let mut c = open("kit-run-notes");
    run_panel(
        &mut c,
        r#"{ phase: "complete", activeMs: 0, live: false, paused: false, line: "" }"#,
        r#"{ spans: [], handoffs: [], marks: [], summary: { active_ms: 0, paused_ms: 0, paused_now: false }, notes: ["word history starts at this version"] }"#,
    );
    assert!(
        c.eval("document.querySelector('#panel .kit-tl__note').textContent")
            .as_str()
            .unwrap_or_default()
            .contains("starts at this version")
    );
    let mut c = open("kit-run-notes-prep");
    run_panel(
        &mut c,
        r#"{ phase: "errored", activeMs: 0, live: false, paused: false, line: "" }"#,
        r#"{ spans: [], handoffs: [], marks: [], summary: { active_ms: 0, paused_ms: 0, paused_now: false }, notes: ["ended before it started"] }"#,
    );
    assert!(
        c.eval("document.querySelector('#panel .kit-tl__note').textContent")
            .as_str()
            .unwrap_or_default()
            .contains("ended before it started")
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn steps_list_the_beads_under_build_and_fold_past_six() {
    let mut c = open("kit-steps-beads");
    c.eval(
        r#"(() => {
          const names = ["think", "spec", "approve", "plan", "build", "review"];
          const bead = (i, status) => ({ id: "t-" + i, title: "Bead " + i, status });
          const seven = [1, 2, 3, 4, 5, 6, 7].map((i) => bead(i, i === 1 ? "done" : "open"));
          const six = seven.slice(0, 6);
          document.body.append(steps(names, "build", { beads: seven }));
          document.body.append(steps(names, "build", { beads: six }));
          return true;
        })()"#,
    );
    assert_eq!(
        c.eval("document.querySelectorAll('ol.kit-steps')[0].querySelectorAll('.kit-steps__beads > li:not(.kit-steps__more)').length"),
        json!(5),
        "more than six beads fold to five"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('ol.kit-steps')[0].querySelector('.kit-steps__more').textContent"),
        json!("2 more beads"),
    );
    assert_eq!(
        c.eval("document.querySelectorAll('ol.kit-steps')[1].querySelectorAll('.kit-steps__beads > li:not(.kit-steps__more)').length"),
        json!(6),
        "six beads all show"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('ol.kit-steps')[1].querySelectorAll('.kit-steps__more').length"),
        json!(0),
    );
    assert_eq!(
        c.eval("document.querySelector('ol.kit-steps li[aria-current=step]').textContent.includes('Build')"),
        json!(true),
        "the beads sit under Build, the current step"
    );
    assert_eq!(
        c.eval("document.querySelector('ol.kit-steps .kit-steps__beads li').dataset.beadState"),
        json!("finished"),
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn steps_mark_before_at_and_after_the_current_step_and_a_drift() {
    let mut c = open("kit-steps");
    c.eval(
        r#"(() => {
          const names = ["think", "spec", "approve", "plan", "build"];
          document.body.append(steps(names, "approve", { state: "awaiting_approval" }));
          document.body.append(steps(names, "spec", { drift: "the design changed under it" }));
          return true;
        })()"#,
    );
    let states =
        "[...document.querySelectorAll('ol.kit-steps')[0].children].map(li => li.dataset.state)";
    assert_eq!(
        c.eval(states),
        json!([
            "finished",
            "finished",
            "awaiting_approval",
            "queued",
            "queued"
        ]),
        "before the gate finished, the gate awaiting, after queued"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('ol.kit-steps')[0].querySelector('[aria-current=step]').textContent.includes('Approve')"),
        json!(true)
    );
    assert_eq!(
        c.eval(
            "[...document.querySelectorAll('ol.kit-steps')[1].children].map(li => li.dataset.state)"
        ),
        json!(["finished", "errored", "queued", "queued", "queued"]),
        "the drifted step is errored"
    );
    assert_eq!(
        c.eval("document.querySelector('li[data-state=errored] .kit-steps__state').textContent"),
        json!("the design changed under it"),
        "the reason replaces the state"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('ol.kit-steps time').length"),
        json!(0),
        "steps carry no times"
    );
}

/// Keeps the decision stack's callbacks where the page can read them.
const GATE_ITEM: &str = r#"(key, extra = {}) => ({
  key, title: "Design " + key, repo: "web", epic: "t-" + key, design: "docs/" + key + ".md",
  brief: null, design_sha256: "abcdef1234567890", design_error: null, review: null, history: [],
  by: "Ann", clean: false, done: null, confirming: false, busy: false, error: "",
  names: ["think", "spec", "approve", "plan", "build"], flow: { step: "spec", beads: [] },
  ...extra,
})"#;

const CLEAN_REVIEW: &str = r#"{ path: "docs/y-review.md", verdict: "clean", current: true, blocking: 0, should: 0, nit: 0 }"#;

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn decision_stack_peeks_rotate_and_focus_lands_on_the_next_approve() {
    let mut c = open("kit-decisions");
    c.eval(&format!(
        r#"(() => {{
          const item = {GATE_ITEM};
          window.__asked = []; window.__approved = [];
          window.__cb = {{ onOpen() {{}}, onAsk: (k) => window.__asked.push(k), onApprove: (k) => window.__approved.push(k), onCancel() {{}} }};
          window.__stack = (items, opts) => {{ document.body.replaceChildren(decisions(items, {{ ...window.__cb, ...opts }})); }};
          window.__stack([item("a"), item("b"), item("c"), item("d", {{ done: {{ check: "approved" }} }})]);
          return true;
        }})()"#
    ));
    assert_eq!(c.eval("[...document.querySelectorAll('.kit-stack__card')].map(x => x.dataset.key + x.dataset.position)"), json!(["a0", "b1", "c2"]), "top card and two peeks");
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-decisions__history article[data-key=d]').length"),
        json!(1),
        "the decided card is in the history"
    );
    assert_eq!(
        c.eval(
            "document.querySelector('.kit-decisions__history article[data-key=d]').dataset.status"
        ),
        json!("approved-without-review"),
        "a decided card says what the decision was"
    );

    c.eval("document.querySelector('.kit-stack__card[data-key=b] .gate-actions button')?.focus(); true");
    press(&mut c, "ArrowDown", "ArrowDown", 40, "");
    assert_eq!(
        c.eval("document.querySelector('.kit-stack__card[data-position=\"0\"]').dataset.key"),
        json!("b"),
        "arrow down brings the next card to the top"
    );

    c.eval("document.querySelector('.kit-stack__card[data-key=a]').click(); true");
    assert_eq!(
        c.eval("document.querySelector('.kit-stack__card[data-position=\"0\"]').dataset.key"),
        json!("a"),
        "clicking a peek brings it up"
    );

    c.eval(
        "document.querySelector('.kit-stack__card[data-key=a] .gate-actions button').click(); true",
    );
    assert_eq!(
        c.eval("window.__asked"),
        json!(["a"]),
        "Approve asks first, through the callback"
    );

    c.eval(&format!(
        r#"(() => {{
          const item = {GATE_ITEM};
          window.__stack([item("x", {{ clean: true, done: {{ check: "approved" }} }}), item("y",{{ clean: true, review: {CLEAN_REVIEW} }}), item("z")], {{ focusNext: true }});
          return true;
        }})()"#
    ));
    assert_eq!(
        c.eval("document.activeElement.textContent"),
        json!("Approve and start Plan"),
        "after a decision focus lands on the next Approve"
    );
    assert_eq!(
        c.eval(
            "document.querySelector('.kit-decisions__history article[data-key=x]').dataset.status"
        ),
        json!("approved"),
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn decision_cards_read_their_steps_and_their_beads_under_build() {
    let mut c = open("kit-decision-steps");
    c.eval(&format!(
        r#"(() => {{
          const item = {GATE_ITEM};
          window.__stack = (items, opts) => {{ document.body.replaceChildren(decisions(items, {{ onOpen() {{}}, onAsk() {{}}, onApprove() {{}}, onCancel() {{}}, ...opts }})); }};
          window.__stack([item("a", {{ flow: {{ step: "spec", beads: [{{ id: "t-1", title: "One", status: "open" }}] }} }})]);
          return true;
        }})()"#
    ));
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-stack__card ol.kit-steps > li[data-state=awaiting_approval]').length"),
        json!(1),
        "the gate step awaits the approval"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-stack__card .kit-steps__beads li').length"),
        json!(1),
        "the bead sits under Build"
    );
}
