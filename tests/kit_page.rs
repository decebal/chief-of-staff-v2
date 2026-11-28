//! The window's `el()` door and the kit builders in headless Chrome, on the
//! window's own scripts. Run with `COS_E2E_CHROME=<Chrome binary>` and
//! `--include-ignored` (`task test-e2e`).

#[allow(dead_code)]
#[path = "common/cdp.rs"]
mod cdp;

#[path = "../apps/desktop/examples/support/report_base.rs"]
mod report_base;

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
    let sheets: String = ["tokens.css", "style.css", "kit.css"]
        .iter()
        .map(|s| format!(r#"<link rel="stylesheet" href="file://{UI}/{s}">"#))
        .collect();
    std::fs::write(
        &html,
        format!(r#"<!doctype html><meta charset="utf-8">{sheets}<body>{scripts}</body>"#),
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

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn the_previews_report_and_flows_frames_pass_safe_frame_src() {
    let mut c = open("kit-preview-frames");
    let base = report_base::report_base(8793);
    let v: Value = c.eval(&format!(
        "[safeFrameSrc({b:?}), safeFrameSrc({b:?} + 'flows')]",
        b = base
    ));
    assert!(v[0].is_string(), "report frame refused: {base} {v}");
    assert!(v[1].is_string(), "flows frame refused: {base} {v}");
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
    c.wait_for(
        "document.querySelector('#panel .kit-tl') !== null && document.querySelector('#panel [inert]') === null",
    );
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
          const seven = [1, 2, 3, 4, 5, 6, 7].map((i) => bead(i, i === 1 ? "done" : i === 4 ? "in_progress" : "open"));
          const six = seven.slice(0, 6);
          document.body.append(steps(names, "build", { beads: seven }));
          document.body.append(steps(names, "build", { beads: six }));
          return true;
        })()"#,
    );
    let first = "document.querySelectorAll('section.kit-steps')[0]";
    assert_eq!(
        c.eval(&format!(
            "{first}.querySelectorAll('li[data-key=build] .kit-steps__sub').length"
        )),
        json!(5),
        "more than six beads fold to five"
    );
    assert_eq!(
        c.eval(&format!(
            "[...{first}.querySelectorAll('.kit-steps__fold')].map((b) => b.textContent)"
        )),
        json!(["1 earlier", "1 later"]),
        "the window centres on the bead in progress"
    );
    assert_eq!(
        c.eval(&format!("[...{first}.querySelectorAll('.kit-steps__sub')].map((s) => [s.querySelector('.kit-steps__sub-text').textContent, s.dataset.state, s.getAttribute('aria-current')])")),
        json!([
            ["Bead 2", "queued", null],
            ["Bead 3", "queued", null],
            ["Bead 4", "in_progress", "step"],
            ["Bead 5", "queued", null],
            ["Bead 6", "queued", null]
        ])
    );
    assert_eq!(
        c.eval("document.querySelectorAll('section.kit-steps')[1].querySelectorAll('.kit-steps__sub').length"),
        json!(6),
        "six beads all show"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('section.kit-steps')[1].querySelectorAll('.kit-steps__fold').length"),
        json!(0),
    );
    assert_eq!(
        c.eval("document.querySelectorAll('section.kit-steps')[1].querySelector('.kit-steps__sub').dataset.state"),
        json!("finished"),
    );
    assert_eq!(
        c.eval(&format!("(() => {{ const li = {first}.querySelector('li[aria-current=step]'); const b = li.querySelector(':scope > button.kit-steps__row'); return [li.dataset.key, b.getAttribute('aria-expanded'), b.getAttribute('aria-controls') === li.querySelector('.kit-steps__subs').id]; }})()")),
        json!(["build", "true", true]),
        "the beads sit under Build, the step in progress, open"
    );
    c.eval(&format!("{first}.querySelectorAll('.kit-steps__fold')[1].focus(); {first}.querySelectorAll('.kit-steps__fold')[1].click(); true"));
    assert_eq!(
        c.eval(&format!("[{first}.querySelectorAll('.kit-steps__sub').length, [...{first}.querySelectorAll('.kit-steps__fold')].map((b) => b.textContent), document.activeElement.textContent]")),
        json!([7, ["Show fewer"], "Show fewer"]),
        "a fold unfolds every bead and keeps focus on the fold"
    );
    c.eval(&format!(
        "{first}.querySelector('li[data-key=build] > button.kit-steps__row').click(); true"
    ));
    assert_eq!(
        c.eval(&format!("[{first}.querySelectorAll('.kit-steps__sub').length, {first}.querySelector('li[data-key=build] > button').getAttribute('aria-expanded')]")),
        json!([0, "false"]),
        "the row closes its beads"
    );
    assert_eq!(
        c.eval(&format!("[...{first}.querySelectorAll('[role=status][aria-live=polite]')].map((n) => n.textContent)")),
        json!(["4 of 6 steps finished"]),
        "one polite live region with the count"
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
    let first = "document.querySelectorAll('section.kit-steps')[0]";
    assert_eq!(
        c.eval(&format!("[...{first}.querySelectorAll('li.kit-steps__item')].map((li) => [li.dataset.state, li.querySelector('.kit-steps__mark').textContent, li.querySelector('.kit-steps__state')?.textContent ?? null, !!li.querySelector('.kit-steps__gate')])")),
        json!([
            ["finished", "✓", null, false],
            ["finished", "✓", null, false],
            ["awaiting_approval", "◇", "Needs you", true],
            ["queued", "○", "Queued", false],
            ["queued", "○", "Queued", false]
        ]),
        "marks and pills as StepProgressList draws them"
    );
    assert_eq!(
        c.eval(&format!("[...{first}.querySelectorAll('.kit-steps__row')].map((r) => r.getAttribute('aria-label'))")),
        json!(["Think", "Spec", "Approve, Needs you", "Plan, Queued", "Build, Queued"]),
        "a finished step's name drops the empty state word, as item_name does"
    );
    assert_eq!(
        c.eval(&format!("[...{first}.querySelectorAll('[role=status][aria-live=polite]')].map((n) => n.textContent)")),
        json!(["Approve: Needs you"]),
        "the live region names the step that waits on the user"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('section.kit-steps')[1].querySelector('[role=status]').textContent"),
        json!("1 of 5 steps finished"),
        "a drift waits on nothing, so the count"
    );
    assert_eq!(
        c.eval("[...document.querySelectorAll('section.kit-steps li.kit-is-failed')].map((li) => li.dataset.state)"),
        json!(["errored"]),
        "the drifted step flashes, and only it"
    );
    assert_eq!(
        c.eval(&format!("[{first}.querySelector('.kit-steps__heading').textContent, {first}.querySelector('.kit-steps__count').textContent]")),
        json!(["Steps", "2 of 5 steps finished"])
    );
    assert_eq!(
        c.eval(
            "[...document.querySelectorAll('section.kit-steps')[1].querySelectorAll('li.kit-steps__item')].map(li => li.dataset.state)"
        ),
        json!(["finished", "errored", "queued", "queued", "queued"]),
        "the drifted step is errored"
    );
    assert_eq!(
        c.eval("(() => { const li = document.querySelector('li[data-state=errored]'); return [li.querySelector('.kit-steps__mark').textContent, li.querySelector('.kit-steps__state').textContent]; })()"),
        json!(["✕", "the design changed under it"]),
        "the reason replaces the state"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('section.kit-steps time, section.kit-steps .kit-steps__time').length"),
        json!(0),
        "steps carry no times"
    );
    assert_eq!(
        c.eval(&format!("[...{first}.querySelectorAll('li.kit-steps__item')].map((li) => !!li.querySelector(':scope > .kit-steps__branch[aria-hidden=true]') && !!li.querySelector(':scope > .kit-steps__row > .kit-steps__mark[aria-hidden=true]'))")),
        json!([true, true, true, true, true]),
        "every step has a branch and a mark"
    );
    assert_eq!(
        c.eval(&format!("(() => {{ const px = (n, p) => getComputedStyle(n)[p]; const li = {first}.querySelector('li.kit-steps__item'); const m = li.querySelector('.kit-steps__mark'), r = li.querySelector('.kit-steps__row'), b = li.querySelector('.kit-steps__branch'), pill = {first}.querySelector('.kit-steps__state'); return [px(m, 'width'), px(m, 'height'), px(r, 'minHeight'), px(b, 'left'), px(b, 'top'), px(b, 'width'), px(pill, 'height'), px({first}, 'paddingTop')]; }})()")),
        json!(["24px", "24px", "40px", "17px", "38px", "2px", "20px", "16px"]),
        "the rhythm of StepProgressList: 24 px marks, 40 px rows, the branch at 17 px"
    );
    assert_eq!(
        c.eval(&format!(
            "{first}.querySelectorAll('[aria-current]').length"
        )),
        json!(0),
        "only a step in progress is current"
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
        c.eval("document.querySelectorAll('.kit-stack__card section.kit-steps li.kit-steps__item[data-state=awaiting_approval]').length"),
        json!(1),
        "the gate step awaits the approval"
    );
    assert_eq!(
        c.eval("document.querySelector('.kit-stack__card li[data-key=build] > button.kit-steps__row').getAttribute('aria-expanded')"),
        json!("false"),
        "Build, queued, starts closed"
    );
    c.eval("document.querySelector('.kit-stack__card li[data-key=build] > button.kit-steps__row').click(); true");
    assert_eq!(
        c.eval("document.querySelectorAll('.kit-stack__card li[data-key=build] .kit-steps__sub').length"),
        json!(1),
        "the bead sits under Build"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn a_pane_change_crossfades_with_the_outgoing_pane_inert() {
    let mut c = open("kit-swap-pane");
    let v = c.eval(
        r#"(() => {
          const a = el("section", { id: "a" }, "staff");
          const b = el("section", { id: "b", hidden: true }, "runs");
          document.body.append(el("div", { id: "panes" }, a, b));
          swapPane(a, b);
          return { aInert: a.inert, aHidden: a.hidden, aLabel: a.getAttribute("aria-hidden"), bHidden: b.hidden, running: document.getAnimations().length > 0 };
        })()"#,
    );
    assert_eq!(
        v,
        json!({ "aInert": true, "aHidden": false, "aLabel": "true", "bHidden": false, "running": true }),
        "while it fades the outgoing pane is inert"
    );
    c.wait_for("document.getElementById('a').hidden");
    assert_eq!(
        c.eval("[document.getElementById('a').inert, document.getElementById('a').getAttribute('aria-hidden'), document.getElementById('a').style.position]"),
        json!([false, null, ""]),
        "a settled pane is plain again"
    );
    c.eval("swapPane(document.getElementById('b'), document.getElementById('a')); swapPane(document.getElementById('a'), document.getElementById('b')); true");
    assert_eq!(
        c.eval("[document.getElementById('a').hidden || document.getElementById('a').inert, document.getElementById('b').hidden, document.getElementById('b').inert]"),
        json!([true, false, false]),
        "a pane brought back mid-fade is live at once"
    );

    let mut c = open("kit-swap-pane-reduce");
    c.call(
        "Emulation.setEmulatedMedia",
        json!({ "features": [{ "name": "prefers-reduced-motion", "value": "reduce" }] }),
    );
    let v = c.eval(
        r#"(() => {
          const a = el("section", {}, "staff");
          const b = el("section", { hidden: true }, "runs");
          document.body.append(a, b);
          swapPane(a, b);
          return [a.hidden, b.hidden, document.getAnimations().length];
        })()"#,
    );
    assert_eq!(
        v,
        json!([true, false, 0]),
        "under reduce the change is instant"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn the_run_page_crossfades_from_reading_to_ready() {
    let mut c = open("kit-run-swap");
    c.eval(&format!(
        r#"(() => {{
          window.__tl = new Promise((r) => {{ window.__ready = () => r({RUN_TL}); }});
          document.body.append(el("div", {{ id: "panel" }}, runPanel({{ phase: "active", activeMs: 60000, live: true, paused: false, line: "" }}, () => window.__tl, {{ onAction() {{}} }})));
          return true;
        }})()"#
    ));
    assert_eq!(
        c.eval("document.querySelectorAll('#panel [data-kind=reading]').length"),
        json!(1),
        "reading first"
    );
    let v = c.eval(
        r##"(async () => {
          window.__ready();
          await window.__tl;
          await new Promise((r) => setTimeout(r, 0));
          const out = document.querySelector("#panel [inert]");
          return { out: !!out, reading: !!out?.querySelector("[data-kind=reading]"), ready: !!document.querySelector("#panel .kit-tl:not([inert] *)") };
        })()"##,
    );
    assert_eq!(
        v,
        json!({ "out": true, "reading": true, "ready": true }),
        "the reading state fades out inert while the timeline comes in"
    );
    c.wait_for("document.querySelector('#panel [inert]') === null");
    assert_eq!(
        c.eval("document.querySelectorAll('#panel .kit-tl').length"),
        json!(1)
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn reading_states_carry_shimmer_rows_and_settle_under_reduce() {
    let mut c = open("kit-shimmer");
    let v = c.eval(
        r#"(() => {
          const rows = (s) => { const box = s.querySelector(".state-shimmer"); return box ? [box.getAttribute("aria-hidden"), box.children.length, [...box.children].every((r) => r.classList.contains("kit-is-running"))] : null; };
          return {
            reading: rows(stateView("reading", { title: "Reading" })),
            refreshing: rows(stateView("refreshing", { title: "Refreshing" })),
            starting: rows(stateView("starting", { title: "Starting" })),
            empty: rows(stateView("empty", { title: "Nothing" })),
            line: rows(stateView("reading", { title: "Reading", line: true })),
          };
        })()"#,
    );
    assert_eq!(
        v,
        json!({ "reading": ["true", 3, true], "refreshing": ["true", 3, true], "starting": ["true", 3, true], "empty": null, "line": null })
    );
    c.eval("document.body.append(stateView('reading', { title: 'Reading' })); true");
    assert_eq!(
        c.eval("new Promise((r) => requestAnimationFrame(() => r(document.getAnimations().length > 0)))"),
        json!(true),
        "the rows shimmer"
    );
    c.call(
        "Emulation.setEmulatedMedia",
        json!({ "features": [{ "name": "prefers-reduced-motion", "value": "reduce" }] }),
    );
    assert_eq!(
        c.eval(
            "new Promise((r) => requestAnimationFrame(() => r(document.getAnimations().length)))"
        ),
        json!(0),
        "no shimmer runs under reduce"
    );
}

// ---- the Today report, rendered by `cos::report::render` and opened as a page

/// Today's data with a brief and two hires at work.
fn today_data() -> Value {
    json!({
        "now_ts": "2026-10-29T14:30:00Z",
        "today": "2026-10-29",
        "name": "Alex Rivera",
        "brief": { "day": "2026-10-29", "report": {
            "headline": "Release 3.4 ships in three days; the board meets Friday.",
            "needs_you": [{ "verb": "do", "owner": "Alex", "recommendation": "Review api-gateway#412", "why": "", "next_step": "",
              "source": "", "deadline": "", "deadline_basis": "none", "goal_id": "", "deliverable_id": "", "agent_prompt": "Review it." }],
            "coverage": [], "gaps": []
        } },
        "workspace": { "timezone": "America/Chicago", "goals": [] },
        "hires": [
            { "id": "20261029-100000-a001", "title": "Ship the parser", "role": "builder", "identity": 2, "phase": "working",
              "active_ms": 720000, "paused_now": false, "started": "2026-10-29T15:00:00Z", "ended": null, "done_today": false,
              "deadline_secs": 3600, "word": "working", "msg": null, "source": "brief:1" },
            { "id": "20261029-100100-a002", "title": "Review the gateway", "role": "reviewer", "identity": 4, "phase": "needs-you",
              "active_ms": 60000, "paused_now": false, "started": "2026-10-29T15:10:00Z", "ended": null, "done_today": false,
              "deadline_secs": 3600, "word": "blocked", "msg": null, "source": "brief:1" }
        ]
    })
}

/// The report for `data`, written to a file and opened; `init` runs before
/// any of the page's scripts.
fn report(name: &str, data: &Value, init: &str) -> Chrome {
    cdp::chrome_path();
    let dir = cdp::profile(name);
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("report.html");
    std::fs::write(&html, cos::report::render(data).unwrap()).unwrap();
    let mut c = Chrome::start(&dir, &["--disable-gpu"]);
    if !init.is_empty() {
        c.call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": init }),
        );
    }
    c.open(&format!("file://{}", html.display()));
    c.wait_for("document.getElementById('headline') !== null");
    c
}

/// Captures the report's 10-second live-update timer so a test can fire it.
const FIXED_CLOCK: &str = r#"(() => {
  const real = window.setTimeout;
  window.__live = [];
  window.setTimeout = (f, ms, ...a) => (ms === 10000 ? (window.__live.push(f), -1) : real(f, ms, ...a));
})();"#;

/// The hires as the window's `cos-hires` message carries them: the needs-you
/// hire is now working, the working one is done today.
const MOVED: &str = r#"[
  { id: "20261029-100000-a001", title: "Ship the parser", role: "builder", identity: 2, phase: "done",
    active_ms: 900000, paused_now: false, started: "2026-10-29T15:00:00Z", ended: "2026-10-29T15:20:00Z", done_today: true,
    deadline_secs: 3600, word: "done", msg: null, source: "brief:1" },
  { id: "20261029-100100-a002", title: "Review the gateway", role: "reviewer", identity: 4, phase: "working",
    active_ms: 120000, paused_now: false, started: "2026-10-29T15:10:00Z", ended: null, done_today: false,
    deadline_secs: 3600, word: "working", msg: null, source: "brief:1" }
]"#;

const COLUMN_OF: &str = "(id) => document.querySelector(`#hires [data-glide=\"${id}\"]`)?.closest('[data-col]')?.dataset.col ?? null";

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn not_needed_shows_only_for_a_host_and_says_what_the_window_answered() {
    let mut data = today_data();
    data["brief"]["report"]["coverage"] = json!([
        { "source": "email", "health": "auth_failed", "review": "not_reviewed", "window_start": "", "window_end": "", "note": "401" }
    ]);
    data["profile_sources"] = json!([{ "id": "email", "service": "mail" }]);
    let mut c = report("today-not-needed", &data, FIXED_CLOCK);
    let row = "(() => { const b = document.querySelector('[data-source-id=\"email\"]'); return [b.hidden, b.closest('.src-row').querySelector('.src-say').textContent]; })()";
    assert_eq!(
        c.eval(row),
        json!([true, ""]),
        "unframed: no host, no button"
    );
    c.eval("window.postMessage({ type: 'cos-host', hire: true, sources: true }, '*'); true");
    c.wait_for("document.querySelector('[data-source-id=\"email\"]').hidden === false");
    c.eval("window.postMessage({ type: 'cos-source-state', id: 'email', ok: false, error: 'profile is gone' }, '*'); true");
    c.wait_for("document.querySelector('[data-source-id=\"email\"]').closest('.src-row').querySelector('.src-say').textContent !== ''");
    assert_eq!(c.eval(row), json!([false, "Not saved: profile is gone"]));
    c.eval("window.postMessage({ type: 'cos-source-state', id: 'email', ok: true }, '*'); true");
    c.wait_for("document.querySelector('[data-source-id=\"email\"]').hidden === true");
    assert_eq!(
        c.eval(row),
        json!([true, "Not needed — left out from the next brief"])
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn a_hires_message_moves_cards_between_columns_and_leaves_the_host_state() {
    let mut c = report("today-hires", &today_data(), FIXED_CLOCK);
    c.eval(&format!("window.__col = {COLUMN_OF}; true"));
    assert_eq!(
        c.eval("[__col('20261029-100000-a001'), __col('20261029-100100-a002')]"),
        json!(["running", "needs-you"])
    );
    let host = "[document.getElementById('host-run').hidden, document.querySelector('[data-hire]').textContent]";
    c.eval("window.postMessage({ type: 'cos-host', hire: true, runBrief: true }, '*'); true");
    c.wait_for("document.getElementById('host-run').hidden === false");
    let before = c.eval(host);
    assert_eq!(
        before,
        json!([false, "Hire"]),
        "the cos-host message set the host state"
    );
    c.eval(
        r#"(() => {
          const f = document.createElement("iframe");
          f.srcdoc = "<script>parent.postMessage({ type: 'cos-hires', hires: [] }, '*')<\/script>";
          document.body.append(f);
          return true;
        })()"#,
    );
    c.pause(std::time::Duration::from_millis(400));
    assert_eq!(
        c.eval("document.querySelectorAll('#hires [data-glide]').length"),
        json!(2),
        "a message from another frame is ignored"
    );
    c.eval(&format!(
        "window.postMessage({{ type: 'cos-hires', hires: {MOVED} }}, '*'); true"
    ));
    c.wait_for("__col('20261029-100000-a001') === 'done'");
    assert_eq!(
        c.eval("__col('20261029-100100-a002')"),
        json!("running"),
        "the waiting hire moved to Running"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('.hero-pill').length"),
        json!(1),
        "the pills follow the message"
    );
    assert_eq!(c.eval(host), before, "the cos-host state changed");
    assert_eq!(
        c.eval("[...document.querySelectorAll('#hires .inbox-open')].filter((b) => b.textContent !== 'Open in Herdr').length"),
        json!(0),
        "the redrawn inbox keeps the host's Open in Herdr"
    );
    assert_eq!(
        c.eval("document.body.innerText.includes('20261029-')"),
        json!(false),
        "a run id in visible text"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn the_inbox_says_as_last_read_when_no_live_update_comes() {
    let mut c = report("today-stale", &today_data(), FIXED_CLOCK);
    let stale = "[...document.querySelectorAll('[data-stale]')].filter((s) => !s.hidden).length";
    assert_eq!(c.eval(stale), json!(0), "fresh at load");
    assert_eq!(c.eval("window.__live.length"), json!(1), "one 10 s timer");
    c.eval("window.__live.forEach((f) => f()); true");
    assert_eq!(
        c.eval(stale),
        json!(3),
        "the heading, the pills and Running say as last read"
    );
    c.eval(&format!(
        "window.postMessage({{ type: 'cos-hires', hires: {MOVED} }}, '*'); true"
    ));
    c.wait_for(&format!("{stale} === 0"));

    let mut c = report("today-live", &today_data(), FIXED_CLOCK);
    c.eval(&format!(
        "window.postMessage({{ type: 'cos-hires', hires: {MOVED} }}, '*'); true"
    ));
    c.wait_for("document.querySelector('#hires [data-col=done] [data-glide]') !== null");
    c.eval("window.__live.forEach((f) => f()); true");
    assert_eq!(
        c.eval(stale),
        json!(0),
        "a live update inside 10 s keeps it fresh"
    );
}

/// Captures the schedule board's minute timer and lets a test move the clock.
const MINUTE_CLOCK: &str = r#"(() => {
  const realInterval = window.setInterval, realNow = Date.now;
  window.__minute = [];
  window.__shift = 0;
  Date.now = () => realNow() + window.__shift;
  window.setInterval = (f, ms, ...a) => (ms === 60000 ? (window.__minute.push(f), -1) : realInterval(f, ms, ...a));
})();"#;

fn schedule_data() -> Value {
    let mut d = today_data();
    d["brief"]["report"]["calendar"] = json!([
        { "date": "2026-10-29", "start": "09:00", "end": "10:00", "what": "Design review", "prep": "Read the RFC." },
        { "date": "2026-10-29", "start": "10:30", "end": "11:00", "what": "Vendor call", "prep": "" },
        { "date": "2026-10-29", "start": "10:32", "end": "10:40", "what": "Vendor follow-up", "prep": "" }
    ]);
    d
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn the_schedule_board_moves_under_the_now_line_each_minute_and_selects_a_row() {
    let mut c = report("today-schedule", &schedule_data(), MINUTE_CLOCK);
    let left = "(name) => document.querySelector(`#schedule .kit-asb__marker[aria-label^=\"${name},\"]`).style.left";
    c.eval(&format!("window.__left = {left}; true"));
    assert_eq!(
        c.eval(
            "[__left('Vendor call'), document.querySelector('#schedule .kit-asb__now').style.left]"
        ),
        json!(["60%", "50%"]),
        "an hour ahead sits a tenth of the axis right of the centred now line"
    );
    assert_eq!(
        c.eval("window.__minute.length"),
        json!(1),
        "one minute timer"
    );
    c.eval("window.__shift = 30 * 60000; window.__minute.forEach((f) => f()); true");
    assert_eq!(
        c.eval(
            "[__left('Vendor call'), document.querySelector('#schedule .kit-asb__now').style.left]"
        ),
        json!(["55%", "50%"]),
        "half an hour later the marker has moved left under the fixed now line"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#schedule .kit-asb__tick').length > 0"),
        json!(true)
    );
    assert_eq!(
        c.eval("[...document.querySelectorAll('#schedule .kit-asb__panel')].filter((p) => !p.hidden).map((p) => p.querySelector('h3').textContent)"),
        json!(["Design review"]),
        "the running slot is selected at load"
    );
    c.eval("document.querySelector('#schedule .kit-asb__marker[aria-label^=\"Vendor call,\"]').click(); true");
    assert_eq!(
        c.eval("[document.querySelector('#schedule [aria-current=true] .kit-asb__name').textContent, [...document.querySelectorAll('#schedule .kit-asb__panel')].filter((p) => !p.hidden).map((p) => p.querySelector('h3').textContent)]"),
        json!(["Vendor call", ["Vendor call"]]),
        "a marker selects its row and shows its detail"
    );
    assert_eq!(
        c.eval("(() => { const b = [...document.querySelectorAll('#schedule button:not([hidden])')].map((n) => n.className); const at = b.indexOf('kit-asb__row-hit'); return [at > 0, b.slice(0, at).every((k) => k === 'kit-asb__marker'), b.slice(at).every((k) => k === 'kit-asb__row-hit'), document.querySelectorAll('#schedule [tabindex]').length]; })()"),
        json!([true, true, true, 0]),
        "keyboard order as the gallery: the timeline's markers, then the rows"
    );
    assert_eq!(
        c.eval("[...document.querySelectorAll('#schedule .kit-asb__marker:not([hidden])')].every((m) => { const r = m.getBoundingClientRect(); return r.width >= 24 && r.height >= 24; })"),
        json!(true),
        "a marker's hit area is at least 1.5rem square"
    );
    let overlaps = "(() => { const r = [...document.querySelectorAll('#schedule .kit-asb__marker:not([hidden])')].map((m) => m.getBoundingClientRect()); let n = 0; for (let i = 0; i < r.length; i++) for (let j = i + 1; j < r.length; j++) if (r[i].left < r[j].right && r[j].left < r[i].right && r[i].top < r[j].bottom && r[j].top < r[i].bottom) n++; return n; })()";
    assert_eq!(
        c.eval(overlaps),
        json!(0),
        "markers two minutes apart never cover each other's hit area"
    );
    c.eval("window.__shift = 31 * 60000; window.__minute.forEach((f) => f()); true");
    assert_eq!(
        c.eval(overlaps),
        json!(0),
        "nor after the minute moves them"
    );
    c.eval("(() => { const p = document.querySelector('#schedule').closest('[role=tabpanel]'); p.hidden = true; window.__shift = 33 * 60000; window.__minute.forEach((f) => f()); p.hidden = false; return true; })()");
    assert_eq!(
        c.eval(&format!("new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(() => ok({overlaps}))))")),
        json!(0),
        "nor when the minute ticked while Today was hidden behind another tab"
    );
    assert_eq!(
        c.eval("(() => { const a = document.querySelector('#schedule .kit-asb__axis').getBoundingClientRect(); const labels = [...document.querySelectorAll('#schedule .kit-asb__tick')].map((t) => t.getBoundingClientRect()); return [...document.querySelectorAll('#schedule .kit-asb__marker:not([hidden])')].every((m) => { const s = getComputedStyle(m, '::before'); const r = m.getBoundingClientRect(); const d = parseFloat(s.height); const top = r.top + (r.height - d) / 2; const bottom = top + d; return top >= a.top && bottom <= a.bottom && labels.every((l) => bottom <= l.top || r.right <= l.left || r.left >= l.right); }); })()"),
        json!(true),
        "every marker's dot stays inside the axis band and clear of the hour labels"
    );
    c.eval("document.querySelector('#schedule .kit-asb__marker[aria-label^=\"Design review,\"]').focus(); true");
    press(&mut c, "Enter", "Enter", 13, "\r");
    assert_eq!(
        c.eval("[document.querySelector('#schedule [aria-current=true] .kit-asb__name').textContent, [...document.querySelectorAll('#schedule .kit-asb__panel')].filter((p) => !p.hidden).map((p) => p.querySelector('h3').textContent), document.activeElement.classList.contains('kit-asb__marker')]"),
        json!(["Design review", ["Design review"], true]),
        "Enter on a focused marker selects its row and panel and keeps focus"
    );
    c.eval("document.querySelector('#schedule .kit-asb__marker[aria-label^=\"Vendor call,\"]').click(); true");
    c.eval("document.querySelector('#schedule [aria-current=true]').focus(); true");
    press(&mut c, "ArrowUp", "ArrowUp", 38, "");
    assert_eq!(
        c.eval("document.activeElement.querySelector('.kit-asb__name')?.textContent ?? null"),
        json!("Review the gateway"),
        "arrow up moves to the row before"
    );
}

/// Records every KPI ticker the page adds, before its scripts run.
const WATCH_TICKERS: &str = r#"(() => {
  window.__tickers = [];
  new MutationObserver((list) => {
    for (const m of list) for (const n of m.addedNodes) {
      if (n.classList?.contains("kit-ticker")) window.__tickers.push({ from: n.dataset.from, hidden: n.getAttribute("aria-hidden"), text: n.textContent });
    }
  }).observe(document, { childList: true, subtree: true });
})();"#;

fn kpi_report_data() -> Value {
    let mut d = today_data();
    d["workspace"]["kpis"] = json!([{
        "id": "K-1", "goalId": "G-1", "name": "Beta testers", "unit": "testers",
        "baseline": 0, "target": 500, "current": 140, "evidenceAt": "2026-10-28T09:00:00Z",
        "source": "email", "reportingWindow": "2026-10"
    }]);
    d["kpi_points"] = json!([
        { "kpi": "Beta testers", "value": 140, "evidenceAt": "2026-10-28T09:00:00Z", "recordedAt": "2026-10-28T09:00:00Z" },
        { "kpi": "Beta testers", "value": 20, "evidenceAt": "2026-10-14T09:00:00Z", "recordedAt": "2026-10-14T09:00:00Z" },
        { "kpi": "Beta testers", "value": 40, "evidenceAt": "2026-10-21T09:00:00Z", "recordedAt": "2026-10-21T09:00:00Z" }
    ]);
    d
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn a_kpi_tile_rolls_once_from_its_previous_point_to_the_recorded_value() {
    let mut c = report("today-kpi", &kpi_report_data(), WATCH_TICKERS);
    let value = "document.querySelector('[data-section=kpis] .kit-tile__v')";
    assert_eq!(c.eval(&format!("{value}.textContent")), json!("140"));
    assert_eq!(
        c.eval(&format!("{value}.getAttribute('aria-live')")),
        json!("polite")
    );
    assert_eq!(
        c.eval("window.__tickers"),
        json!([{ "from": "40", "hidden": "true", "text": "012345678901234567890123456789" }]),
        "one roll from the second-newest point, its digits hidden"
    );
    c.wait_for("document.querySelectorAll('.kit-ticker').length === 0");
    assert_eq!(
        c.eval(&format!("{value}.textContent")),
        json!("140"),
        "the value text equals the record"
    );
    c.eval("document.querySelector('#tab-plan').click(); document.querySelector('#tab-left').click(); true");
    c.pause(std::time::Duration::from_millis(300));
    assert_eq!(
        c.eval("window.__tickers.length"),
        json!(1),
        "later renders do not roll"
    );

    let mut c = report("today-kpi-reduce", &kpi_report_data(), WATCH_TICKERS);
    c.call(
        "Emulation.setEmulatedMedia",
        json!({ "features": [{ "name": "prefers-reduced-motion", "value": "reduce" }] }),
    );
    c.reload();
    c.wait_for("document.getElementById('headline') !== null");
    assert_eq!(
        c.eval("window.__tickers.length"),
        json!(0),
        "no roll under reduce"
    );
    assert_eq!(c.eval(&format!("{value}.textContent")), json!("140"));
}

const RUNNING: &str = "window.postMessage({ type: 'cos-brief-state', state: 'running', text: 'brief running…', title: '' }, '*'); true";

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn a_brief_run_shows_the_loader_then_brief_ready() {
    let mut c = report("today-loader", &today_data(), "");
    assert_eq!(
        c.eval("document.getElementById('brief-loader').hidden"),
        json!(true),
        "no loader before a run"
    );
    c.eval(RUNNING);
    c.wait_for("!document.getElementById('brief-loader').hidden");
    assert_eq!(
        c.eval("document.querySelector('#brief-loader .brief-loader__text').textContent"),
        json!("brief running…")
    );
    assert_eq!(
        c.eval("document.getElementById('brief-loader').dataset.state"),
        json!("running")
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#brief-loader .brief-loader__rays line').length"),
        json!(8),
        "the loader's rays"
    );
    c.eval("window.postMessage({ type: 'cos-brief-state', state: 'done', text: 'brief done', title: '' }, '*'); true");
    c.wait_for("document.getElementById('brief-loader').dataset.state === 'done'");
    assert_eq!(
        c.eval("document.querySelector('#brief-loader .brief-loader__text').textContent"),
        json!("Brief ready")
    );
    c.eval("window.postMessage({ type: 'cos-brief-state', state: 'error', text: 'brief failed: hover for why', title: '' }, '*'); true");
    c.wait_for("document.getElementById('brief-loader').dataset.state === 'failed'");
    assert_eq!(
        c.eval("document.querySelector('#brief-loader .brief-loader__text').textContent"),
        json!("brief failed: hover for why")
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn the_report_settles_with_no_animation_under_reduced_motion() {
    let mut c = report("today-reduce", &today_data(), "");
    c.call(
        "Emulation.setEmulatedMedia",
        json!({ "features": [{ "name": "prefers-reduced-motion", "value": "reduce" }] }),
    );
    c.reload();
    c.wait_for("document.getElementById('headline') !== null");
    c.eval(RUNNING);
    c.pause(std::time::Duration::from_millis(600));
    assert_eq!(
        c.eval("document.getAnimations().length"),
        json!(0),
        "an animation runs under reduce"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('.hero-pill').length"),
        json!(2)
    );
}
