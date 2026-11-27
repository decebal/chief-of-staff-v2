//! The Flows page in headless Chrome: the board's filters, search and
//! grouping, the palette's selects, the flow graph and its drawer, the Map on
//! SVG and on SwiftShader's GPU with the handover, and the messages the page
//! sends its window. Run with `COS_E2E_CHROME=<Chrome binary>` and
//! `--include-ignored` (`task test-e2e`); each test drives fictional flows.

#[allow(dead_code)]
#[path = "common/cdp.rs"]
mod cdp;
#[path = "common/flows.rs"]
mod fixture;

use cdp::{Chrome, SWIFTSHADER};
use fixture::FlowsPage;
use serde_json::{Value, json};

/// 1,500 beads, or `COS_E2E_NODES`: more Map nodes than the SVG engine's
/// limit, so the GPU engine has something only it can show.
fn beads() -> usize {
    std::env::var("COS_E2E_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500)
        .clamp(1001, 5000)
}

fn open(name: &str, flags: &[&str], query: &str) -> (Chrome, FlowsPage) {
    cdp::chrome_path();
    let dir = cdp::profile(&format!("flows-{name}"));
    let page = fixture::write(&dir.join("page"), beads());
    let mut c = Chrome::start(&dir, flags);
    c.open(&format!("file://{}{query}", page.page.display()));
    c.wait_for("document.body.dataset.engineState === 'decided'");
    (c, page)
}

fn num(c: &mut Chrome, key: &str) -> usize {
    let v = c.data(key);
    v.parse()
        .unwrap_or_else(|_| panic!("dataset.{key} = {v:?}"))
}

/// One real key press: what a person's keyboard sends, so a dialog's Escape
/// and the page's shortcuts run as they would.
fn key(c: &mut Chrome, key: &str, code: &str, vk: u32, modifiers: u32) {
    for kind in ["rawKeyDown", "keyUp"] {
        c.call(
            "Input.dispatchKeyEvent",
            json!({ "type": kind, "key": key, "code": code, "windowsVirtualKeyCode": vk, "modifiers": modifiers }),
        );
    }
}

fn type_text(c: &mut Chrome, text: &str) {
    c.call("Input.insertText", json!({ "text": text }));
}

fn click(c: &mut Chrome, selector: &str) {
    let ok = c.eval(&format!(
        "(() => {{ const e = document.querySelector({selector:?}); if (!e) return false; e.click(); return true; }})()"
    ));
    assert_eq!(ok, true, "nothing matches {selector}");
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn refresh_shows_outside_the_app_and_asks_for_a_fresh_scan() {
    let (mut c, _page) = open("refresh", &["--disable-gpu"], "?engine-test");
    assert_eq!(
        c.eval("getComputedStyle(document.querySelector('[data-act=rescan]')).display !== 'none'"),
        true,
        "Refresh shows under cos dash too"
    );
    click(&mut c, "#facets input[data-facet=repo][value=gamma-data]");
    key(&mut c, "r", "KeyR", 82, 0);
    c.wait_for("location.search.includes('fresh=1')");
    c.wait_for("document.readyState === 'complete'");
    assert!(
        c.eval("location.hash")
            .as_str()
            .unwrap()
            .contains("repo=gamma-data"),
        "the view survives the refresh"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn board_filters_groups_and_searches() {
    let (mut c, page) = open("board", &["--disable-gpu"], "?engine-test");
    assert_eq!(c.data("view"), "board");
    assert_eq!(num(&mut c, "flows"), page.flows, "All shows every flow");
    assert_eq!(
        c.eval("document.querySelector('.col[data-step=done] button.unfold')?.textContent"),
        "1 done ▸",
        "done flows are folded to a count"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#needs .need').length"),
        3,
        "the gate, the drift and the review findings need you"
    );

    click(&mut c, "#facets input[data-facet=repo][value=gamma-data]");
    assert_eq!(num(&mut c, "flows"), page.generated[1]);
    assert_eq!(
        c.eval("cosFlowsTest.state().facets.repo"),
        json!(["gamma-data"])
    );
    assert!(
        c.eval("location.hash")
            .as_str()
            .unwrap()
            .contains("repo=gamma-data"),
        "the filter is in the hash"
    );
    click(&mut c, "#facets input[data-facet=repo][value=gamma-data]");
    assert_eq!(num(&mut c, "flows"), page.flows);

    c.eval("(() => { const q = document.getElementById('q'); q.value = 't-b004'; q.dispatchEvent(new Event('input')); })()");
    assert_eq!(num(&mut c, "flows"), 1, "a bead id finds its flow");
    c.eval("(() => { const q = document.getElementById('q'); q.value = 'no such thing'; q.dispatchEvent(new Event('input')); })()");
    assert_eq!(num(&mut c, "flows"), 0);
    assert!(
        c.eval("document.getElementById('board').textContent")
            .as_str()
            .unwrap()
            .contains("No flows match")
    );
    click(&mut c, "#board [data-act=clear]");
    assert_eq!(num(&mut c, "flows"), page.flows);

    c.eval("(() => { const s = document.querySelector('select[data-pref=group]'); s.value = 'repo'; s.dispatchEvent(new Event('change', { bubbles: true })); })()");
    assert_eq!(
        c.eval("document.querySelectorAll('#board details.lane').length"),
        4,
        "one lane per repo"
    );
    click(&mut c, "#presets [data-preset=needs]");
    assert_eq!(num(&mut c, "flows"), 3);
    click(&mut c, "#presets [data-preset=all]");

    click(&mut c, ".fcard[data-key='alpha-api/t-b000'] .pin");
    assert_eq!(
        c.eval("cosFlowsTest.state().pins"),
        json!(["alpha-api/t-b000"])
    );
    assert_eq!(
        c.eval("document.querySelector('#pins [data-open]')?.dataset.open"),
        "alpha-api/t-b000"
    );

    c.eval("document.activeElement?.blur()");
    key(&mut c, "[", "BracketLeft", 219, 0);
    assert_eq!(
        c.eval("document.getElementById('app').classList.contains('collapsed')"),
        true,
        "[ collapses the sidebar"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn palette_selects_apply_as_facets() {
    let (mut c, page) = open("palette", &["--disable-gpu"], "?engine-test");
    key(&mut c, "k", "KeyK", 75, 2);
    assert_eq!(c.eval("document.getElementById('palette').open"), true);
    type_text(&mut c, "repo:gamma");
    assert!(
        c.eval("document.getElementById('pal-list').textContent")
            .as_str()
            .unwrap()
            .contains("gamma-data")
    );
    key(&mut c, "Enter", "Enter", 13, 0);
    assert_eq!(
        c.eval("[...document.querySelectorAll('#pal-in .chip')].map((e) => e.textContent)"),
        json!(["repo: gamma-data"])
    );
    key(&mut c, "Escape", "Escape", 27, 0);
    assert_eq!(c.eval("document.getElementById('palette').open"), false);
    assert_eq!(
        c.eval("cosFlowsTest.state().facets.repo"),
        json!(["gamma-data"])
    );
    assert_eq!(num(&mut c, "flows"), page.generated[1]);

    key(&mut c, "k", "KeyK", 75, 2);
    type_text(&mut c, "t-b004");
    key(&mut c, "Enter", "Enter", 13, 0);
    assert_eq!(c.data("view"), "flow", "a bead result opens its flow");
    assert_eq!(c.eval("cosFlowsTest.state().bead"), "t-b004");
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn flow_view_opens_a_card_drawer() {
    let (mut c, _page) = open("flow", &["--disable-gpu"], "?engine-test");
    c.eval("cosFlowsTest.openFlow('alpha-api/t-b000')");
    assert_eq!(c.data("view"), "flow");
    assert_eq!(num(&mut c, "beads"), 6);
    let states = c.eval(
        "Object.fromEntries([...document.querySelectorAll('#cards [data-bead]')].map((e) => [e.dataset.bead, e.querySelector('.state').textContent]))",
    );
    assert_eq!(
        states,
        json!({ "t-b001": "Done", "t-b002": "In progress", "t-b003": "Ready", "t-b004": "Blocked",
                "t-b005": "Blocked", "t-b006": "Blocked" })
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#cards [data-ghost]').length"),
        1,
        "the external blocker is a ghost card"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#edges path.open').length"),
        4,
        "b002→b004, b003→b005, b004→b005, ext→b006 wait on undone blockers"
    );
    assert_eq!(
        c.eval(
            "document.querySelector('#flowsteps [aria-current=step] .kit-steps__title').textContent"
        ),
        "Build"
    );

    click(&mut c, "#cards [data-bead=t-b004]");
    assert_eq!(c.eval("document.getElementById('drawer').hidden"), false);
    let drawer = c
        .eval("document.getElementById('drawer').textContent")
        .as_str()
        .unwrap()
        .to_owned();
    for want in [
        "US-004: backfill job",
        "Blocked by (1)",
        "t-b002 US-002: write path",
        "Checked: Test: the store rejects a bad row",
        "Backfill runs under the deadline",
    ] {
        assert!(drawer.contains(want), "drawer lacks {want:?}: {drawer}");
    }
    assert_eq!(
        c.eval("document.activeElement.classList.contains('x')"),
        true,
        "the close button takes focus"
    );
    key(&mut c, "Escape", "Escape", 27, 0);
    assert_eq!(c.eval("document.getElementById('drawer').hidden"), true);
    assert_eq!(
        c.eval("document.activeElement.dataset.bead"),
        "t-b004",
        "focus returns to the card"
    );

    key(&mut c, "ArrowLeft", "ArrowLeft", 37, 0);
    assert_eq!(
        c.eval("document.activeElement.dataset.bead"),
        "t-b002",
        "left moves to the blocker"
    );
    key(&mut c, "ArrowRight", "ArrowRight", 39, 0);
    assert_eq!(c.eval("document.activeElement.dataset.bead"), "t-b004");
    assert_eq!(
        c.eval("document.getElementById('world').classList.contains('dim')"),
        true,
        "a focused card lights its chain"
    );

    click(&mut c, "[data-act=crit]");
    assert_eq!(
        c.eval(
            "[...document.querySelectorAll('#cards .bcard.crit')].map((e) => e.dataset.bead).sort()"
        ),
        json!(["t-b002", "t-b004", "t-b005"]),
        "the longest chain of open beads"
    );

    key(&mut c, "Escape", "Escape", 27, 0);
    assert_eq!(c.data("view"), "board", "Esc leaves the flow for the board");
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn map_on_svg_without_a_gpu() {
    let (mut c, page) = open("map-svg", &["--disable-gpu"], "?engine-test");
    c.eval("cosFlowsTest.go('map')");
    assert_eq!(c.data("engine"), "svg");
    assert_eq!(c.data("hard"), "1000");
    assert_eq!(num(&mut c, "beads"), page.map_beads);
    let shown = page.map_nodes.min(1000);
    assert_eq!(num(&mut c, "shown"), shown);
    assert_eq!(
        c.eval("document.querySelectorAll('#mapworld .mnode').length"),
        shown
    );
    let note = c
        .eval("document.getElementById('mapnote').textContent")
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        note.contains("beads left out at the 1000-node limit"),
        "{note}"
    );

    // The default fit keeps the cards' words: an id, a state and a title on every card.
    let k = c.eval("+/scale\\(([\\d.]+)\\)/.exec(document.getElementById('mapworld').getAttribute('transform'))[1]");
    assert!(k.as_f64().unwrap() >= 0.45, "fit zoom {k}");
    assert_eq!(
        c.eval("document.getElementById('mapsvg').classList.contains('lod')"),
        false
    );

    c.eval("cosFlowsTest.mapZoom(1)");
    let id = "alpha-api/t-b000#t-b004";
    let at = c.eval(&format!("cosFlowsTest.screenOfMap({id:?})"));
    let (x, y) = (at[0].as_f64().unwrap(), at[1].as_f64().unwrap());
    c.mouse("mouseMoved", x, y, 0);
    c.mouse("mousePressed", x, y, 1);
    c.mouse("mouseReleased", x, y, 1);
    c.wait_for("!document.getElementById('drawer').hidden");
    assert_eq!(c.eval("cosFlowsTest.state().bead"), "t-b004");
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn map_on_gpu_on_swiftshader() {
    let (mut c, page) = open("map-gpu", SWIFTSHADER, "?engine-test=deadline:10000");
    let line = |c: &mut Chrome| {
        c.eval("document.getElementById('engine-line').textContent")
            .as_str()
            .unwrap_or("")
            .to_owned()
    };
    assert_eq!(c.data("adapter"), "swiftshader", "{}", line(&mut c));
    assert_eq!(c.data("engine"), "gpu", "{}", line(&mut c));
    assert_eq!(c.data("hard"), "5000");
    c.eval("cosFlowsTest.go('map')");
    assert_eq!(num(&mut c, "shown"), page.map_nodes);
    assert!(page.map_nodes > 1000, "more than the SVG engine can show");
    assert_eq!(
        num(&mut c, "drawnNodes") + num(&mut c, "overlay"),
        page.map_nodes
    );
    assert_eq!(c.eval("document.querySelectorAll('canvas.gpu').length"), 1);
    // The canvas has no text: the cards in view get theirs from the SVG over it.
    c.pause(std::time::Duration::from_millis(300));
    assert!(num(&mut c, "labels") > 0, "no card words over the canvas");
    assert_eq!(
        c.eval("[...document.querySelectorAll('#mapworld .mnode text')].some((t) => t.textContent.startsWith('Generated bead'))"),
        true
    );

    // A done bead the canvas drew (no SVG over it) shows the muted token at its centre.
    c.eval("cosFlowsTest.mapZoom(1)");
    c.pause(std::time::Duration::from_millis(300));
    assert_eq!(num(&mut c, "overlay"), 0, "nothing is in focus");
    let at = c.eval("cosFlowsTest.screenOfMap('alpha-api/t-b000#t-b001')");
    let shot = c.screenshot();
    let pixel = c.eval(&format!(
        "new Promise((res, rej) => {{ const img = new Image(); img.onload = () => {{ const cv = document.createElement('canvas'); cv.width = img.width; cv.height = img.height; \
         const x = cv.getContext('2d'); x.drawImage(img, 0, 0); res(Array.from(x.getImageData(Math.round({}), Math.round({}), 1, 1).data.slice(0, 3))); }}; img.onerror = rej; img.src = {shot:?}; }})",
        at[0], at[1]
    ));
    let want = c.eval("cosFlowsTest.token('muted')");
    let close = pixel
        .as_array()
        .unwrap()
        .iter()
        .zip(want.as_array().unwrap())
        .all(|(a, b)| (a.as_i64().unwrap() - b.as_i64().unwrap()).abs() <= 6);
    assert!(
        close,
        "the done bead drew {pixel}, the muted token is {want}"
    );

    c.eval("cosFlowsTest.loseDevice()");
    assert_eq!(c.data("engine"), "svg");
    assert_eq!(c.data("shown"), "1000");
    assert!(line(&mut c).contains("GPU device lost"), "{}", line(&mut c));
    assert_eq!(
        c.eval("document.querySelectorAll('canvas.gpu').length"),
        0,
        "the GPU engine removed its canvas"
    );
}

/// The page in a frame sandboxed as the desktop window's (`allow-scripts allow-popups`,
/// an opaque origin), under a host that records every message, and an isolated world
/// in the frame through which the test reaches its DOM.
fn framed(name: &str) -> (Chrome, Value, FlowsPage) {
    framed_with(name, |html| html)
}

/// [`framed`], over the fixture page as `edit` rewrites it.
fn framed_with(name: &str, edit: impl Fn(String) -> String) -> (Chrome, Value, FlowsPage) {
    cdp::chrome_path();
    let dir = cdp::profile(name);
    let page = fixture::write(&dir.join("page"), 1001);
    let html = std::fs::read_to_string(&page.page).unwrap();
    std::fs::write(&page.page, edit(html)).unwrap();
    let host = dir.join("page/host.html");
    std::fs::write(
        &host,
        "<!doctype html><body><script>window.got = []; addEventListener('message', (e) => got.push(e.data));</script>\
         <iframe id=\"f\" sandbox=\"allow-scripts allow-popups\" src=\"flows.html\" style=\"width:1200px;height:900px\"></iframe></body>",
    )
    .unwrap();
    // One process for both frames, so the sandboxed frame shows in this page's frame tree.
    let mut c = Chrome::start(
        &dir,
        &[
            "--disable-gpu",
            "--disable-site-isolation-trials",
            "--disable-features=IsolateOrigins,site-per-process",
        ],
    );
    c.open(&format!("file://{}", host.display()));
    assert!(page.flows > 0);
    let tree = c.call("Page.getFrameTree", json!({}));
    let frame = tree["frameTree"]["childFrames"][0]["frame"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no flows frame in {tree}"))
        .to_owned();
    let world = c.call(
        "Page.createIsolatedWorld",
        json!({ "frameId": frame, "worldName": "test" }),
    )["executionContextId"]
        .clone();
    (c, world, page)
}

fn inside(c: &mut Chrome, world: &Value, expr: &str) -> Value {
    c.call(
        "Runtime.evaluate",
        json!({ "expression": expr, "contextId": world, "awaitPromise": true, "returnByValue": true }),
    )["result"]["value"]
        .clone()
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn tabs_and_crumbs_switch_views_in_the_sandboxed_frame() {
    let (mut c, world, _page) = framed("flows-tabs");
    for _ in 0..50 {
        if inside(&mut c, &world, "document.body.dataset.view === 'board'") == json!(true) {
            break;
        }
        c.pause(std::time::Duration::from_millis(100));
    }
    // Each step clicks, waits a frame, and reads the view the page shows and the hash the host was told.
    let step = |c: &mut Chrome, click: &str| -> (String, String) {
        let ok = inside(
            c,
            &world,
            &format!(
                "(() => {{ const e = document.querySelector({click:?}); if (!e || e.disabled) return false; e.click(); return true; }})()"
            ),
        );
        assert_eq!(ok, json!(true), "{click} is missing or disabled");
        let view = inside(
            c,
            &world,
            "new Promise((r) => requestAnimationFrame(() => r(document.body.dataset.view)))",
        );
        let told =
            c.eval("(got.filter((m) => m.type === 'cos-flows-state').pop() || {}).hash ?? null");
        (
            view.as_str().unwrap_or("").to_owned(),
            told.as_str().unwrap_or("").to_owned(),
        )
    };
    let (view, told) = step(&mut c, ".views button[data-view=map]");
    assert_eq!((view.as_str(), told.as_str()), ("map", "v=map"));
    assert_eq!(
        inside(
            &mut c,
            &world,
            "[...document.querySelectorAll('#crumbs button')].map((b) => b.textContent)"
        ),
        json!(["Flows"]),
        "the crumb for where you are is not a button"
    );
    let (view, told) = step(&mut c, ".views button[data-view=flow]");
    assert_eq!(
        view, "flow",
        "with no flow chosen, Flow opens the board's first open flow"
    );
    assert!(told.starts_with("v=flow&f="), "{told}");
    let (view, told) = step(&mut c, "#crumbs button[data-go=board-repo]");
    assert_eq!(view, "board");
    assert!(told.contains("repo="), "{told}");
    let (view, told) = step(&mut c, "#crumbs button[data-go=board]");
    assert_eq!(view, "board");
    assert!(
        !told.contains("repo="),
        "Flows shows every flow again: {told}"
    );
    let (view, _) = step(&mut c, ".views button[data-view=flow]");
    assert_eq!(view, "flow", "the flow chosen before");
    let (view, told) = step(&mut c, ".views button[data-view=board]");
    assert_eq!(view, "board");
    assert!(told.starts_with("f="), "{told}");
    step(&mut c, "#sidebar-toggle");
    assert_eq!(
        inside(
            &mut c,
            &world,
            "document.getElementById('app').classList.contains('collapsed')"
        ),
        json!(true)
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn map_edges_run_between_cards_not_through_them() {
    let (mut c, _page) = open("map-edges", &["--disable-gpu"], "?engine-test");
    c.eval("location.hash = 'v=map&repo=alpha-api'");
    c.wait_for("document.body.dataset.view === 'map' && document.querySelectorAll('#mapworld .mnode').length > 0");
    c.eval("new Promise((r) => requestAnimationFrame(r))");
    assert_eq!(
        c.eval("[...document.querySelectorAll('#mapworld .mnode[data-id=\"alpha-api/t-b000#t-b004\"] text')].map((t) => t.textContent)"),
        json!(["t-b004", "Blocked", "US-004: backfill job"])
    );
    // t-b001 → t-b005 skips a layer; t-b006 waits on a bead in another flow.
    let crossings = c.eval(
        "(() => { const rects = [...document.querySelectorAll('#mapworld .mnode rect')].map((r) => [+r.getAttribute('x'), +r.getAttribute('y'), +r.getAttribute('width'), +r.getAttribute('height'), r.parentNode.dataset.id]); \
         const out = []; for (const p of document.querySelectorAll('#mapworld path.medge')) { const L = p.getTotalLength(); \
         for (let s = 3; s < L - 3; s += 3) { const q = p.getPointAtLength(s); for (const [x, y, w, h, id] of rects) if (q.x > x + 1 && q.x < x + w - 1 && q.y > y + 1 && q.y < y + h - 1) out.push(`${p.dataset.from}→${p.dataset.to} crosses ${id}`); } } \
         return [...new Set(out)]; })()",
    );
    assert_eq!(crossings, json!([]), "edges through cards");
    assert_eq!(
        c.eval("document.querySelectorAll('#mapworld path.medge.hint[data-from=\"\"][data-to=\"alpha-api/t-b000#t-b006\"]').length"),
        1,
        "the blocker in another flow is a dashed stub"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#mapworld path.medge[data-from=\"alpha-api/t-b000#t-b001\"][data-to=\"alpha-api/t-b000#t-b005\"]').length"),
        1
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn messages_name_only_repo_epic_and_key() {
    let (mut c, world, _page) = framed("flows-messages");
    let inside = |c: &mut Chrome, expr: &str| inside(c, &world, expr);
    for _ in 0..50 {
        if inside(&mut c, "!!document.querySelector('#needs .act')") == json!(true) {
            break;
        }
        c.pause(std::time::Duration::from_millis(100));
    }
    for script in [
        "document.querySelector('#needs .act').click()",
        "document.querySelector('.fcard[data-key=\"alpha-api/t-b000\"] .pin').click()",
        "document.querySelector('#facets input[data-facet=repo]').click()",
        "document.querySelector('[data-act=rescan]').click()",
        "document.querySelector('[data-open=\"alpha-api/t-b000\"]').click()",
        "document.querySelector('[data-epic]').click()",
        "document.querySelector('#drawer [data-file=design]').click()",
        "(() => { for (const b of document.querySelectorAll('[data-bead]')) { b.click(); const x = document.querySelector('#drawer .hire-row .act:not([disabled])'); if (x) { x.click(); return; } } throw new Error('no bead to hire'); })()",
    ] {
        assert_eq!(
            inside(&mut c, &format!("(() => {{ {script}; return true; }})()")),
            json!(true),
            "{script}"
        );
    }
    c.pause(std::time::Duration::from_millis(200));
    let got = c.eval("got");
    let got = got.as_array().unwrap();
    let kinds: Vec<&str> = got.iter().filter_map(|m| m["type"].as_str()).collect();
    for want in [
        "cos-flows-gate",
        "cos-flows-prefs",
        "cos-flows-state",
        "cos-flows-rescan",
        "cos-flows-open",
        "cos-flows-hire",
    ] {
        assert!(kinds.contains(&want), "no {want} in {kinds:?}");
    }
    let id = |v: &Value| {
        v.as_str().is_some_and(|s| {
            !s.is_empty()
                && s.len() <= 100
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        })
    };
    for m in got {
        let keys: Vec<&str> = m.as_object().unwrap().keys().map(String::as_str).collect();
        match m["type"].as_str().unwrap() {
            "cos-flows-gate" => {
                assert_eq!(keys.len(), 3, "{m}");
                assert!(id(&m["repo"]) && id(&m["epic"]), "{m}");
                assert_eq!(m["epic"], "t-a000");
            }
            "cos-flows-open" => {
                assert_eq!(keys.len(), 4, "{m}");
                assert!(id(&m["repo"]) && id(&m["epic"]), "{m}");
                assert_eq!(m["key"], "design");
            }
            "cos-flows-hire" => {
                assert_eq!(keys.len(), 4, "{m}: names the bead, carries no prompt");
                assert!(id(&m["repo"]) && id(&m["epic"]) && id(&m["bead"]), "{m}");
            }
            "cos-flows-state" | "cos-flows-rescan" => {
                assert_eq!(keys.len(), 2, "{m}");
                assert!(m["hash"].as_str().unwrap().len() <= 2000);
            }
            "cos-flows-prefs" => {
                assert_eq!(keys.len(), 2, "{m}");
                assert_eq!(m["prefs"]["pins"], json!(["alpha-api/t-b000"]));
            }
            other => panic!("unexpected message {other}: {m}"),
        }
    }
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn a_view_change_crossfades_with_the_outgoing_view_inert() {
    let (mut c, _page) = open("view-swap", &["--disable-gpu"], "?engine-test");
    c.wait_for("document.body.dataset.view === 'board'");
    let v = c.eval(
        "(() => { document.querySelector('.views button[data-view=map]').click(); const b = document.getElementById('board'); return [b.hidden, b.inert, document.getElementById('map').hidden]; })()",
    );
    assert_eq!(
        v,
        json!([false, true, false]),
        "the board fades out inert as the map comes in"
    );
    c.wait_for("document.getElementById('board').hidden");
    assert_eq!(
        c.eval("document.getElementById('board').inert"),
        json!(false)
    );

    let (mut c, _page) = open("view-swap-reduce", &["--disable-gpu"], "?engine-test");
    reduce(&mut c);
    c.wait_for("document.body.dataset.view === 'board'");
    let v = c.eval(
        "(() => { document.querySelector('.views button[data-view=map]').click(); return [document.getElementById('board').hidden, document.getElementById('board').inert]; })()",
    );
    assert_eq!(
        v,
        json!([true, false]),
        "under reduce the change is instant"
    );
}

/// Emulates `prefers-reduced-motion: reduce` from here on, and lets a view
/// crossfade started before it (one `--motion-slow`) finish.
fn reduce(c: &mut Chrome) {
    c.call(
        "Emulation.setEmulatedMedia",
        json!({ "features": [{ "name": "prefers-reduced-motion", "value": "reduce" }] }),
    );
    c.pause(std::time::Duration::from_millis(450));
}

/// How many animations the document runs, two frames from now.
fn animations(c: &mut Chrome) -> Value {
    c.eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(document.getAnimations().length))))")
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn the_flow_views_steps_column_ports_step_progress_list() {
    let (mut c, _page) = open("steps", &["--disable-gpu"], "?engine-test");
    c.eval("cosFlowsTest.openFlow('alpha-api/t-b000')");
    let items = "[...document.querySelectorAll('#flowsteps section.kit-steps > ol.kit-steps__list > li.kit-steps__item')].map((li) => [li.querySelector('.kit-steps__title').textContent, li.dataset.state, li.querySelector('.kit-steps__mark').textContent])";
    assert_eq!(
        c.eval(items),
        json!([
            ["Think", "finished", "✓"],
            ["Spec", "finished", "✓"],
            ["Plan", "finished", "✓"],
            ["Build", "in_progress", "◌"],
            ["Review", "queued", "○"],
            ["Ship", "queued", "○"],
            ["Reflect", "queued", "○"]
        ])
    );
    assert_eq!(
        c.eval("[...document.querySelectorAll('#flowsteps [aria-current=step]')].filter((n) => n.matches('li.kit-steps__item')).map((li) => li.querySelector('.kit-steps__title').textContent)"),
        json!(["Build"]),
        "only the step in progress is current"
    );
    assert_eq!(
        c.eval("document.querySelector('#flowsteps .kit-steps__count').textContent"),
        json!("3 of 7 steps finished")
    );
    assert_eq!(
        c.eval("[document.querySelectorAll('#flowsteps [role=status][aria-live=polite]').length, document.querySelector('#flowsteps [role=status]').textContent]"),
        json!([1, "3 of 7 steps finished"]),
        "one polite live region"
    );
    assert_eq!(
        c.eval("(() => { const b = document.querySelector('#flowsteps li[data-key=build] > button.kit-steps__row'); return [b.getAttribute('aria-expanded'), document.querySelectorAll('#flowsteps li[data-key=build] .kit-steps__sub').length]; })()"),
        json!(["true", 6]),
        "Build is open to its beads"
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#flowsteps .kit-steps__mark:not([aria-hidden=true]), #flowsteps .kit-steps__branch:not([aria-hidden=true])').length"),
        json!(0)
    );
    assert_eq!(
        c.eval("document.querySelectorAll('#flowsteps time').length"),
        json!(0),
        "steps carry no times"
    );
    let side = "(() => { const s = document.getElementById('flowsteps').getBoundingClientRect(), t = document.querySelector('.flowtabs').getBoundingClientRect(); return [s.right <= t.left + 1, s.bottom <= t.top + 1]; })()";
    assert_eq!(
        c.eval(side),
        json!([true, false]),
        "a side column beside the tabs"
    );
    narrow(&mut c, 60);
    c.pause(std::time::Duration::from_millis(100));
    assert_eq!(
        c.eval(side),
        json!([false, true]),
        "above the tabs below 64rem"
    );

    c.eval("cosFlowsTest.openFlow('alpha-api/t-f000')");
    assert_eq!(
        c.eval("document.querySelectorAll('#flowsteps li.kit-steps__item').length"),
        json!(7),
        "drift is a state of a step, not an eighth step"
    );
    assert_eq!(
        c.eval("(() => { const li = document.querySelector('#flowsteps li[data-state=errored]'); return [li.querySelector('.kit-steps__title').textContent, li.querySelector('.kit-steps__mark').textContent, li.querySelector('.kit-steps__state').textContent]; })()"),
        json!(["Plan", "✕", "design changed after approval"]),
        "the furthest step with a ritual file is the drifted one"
    );
    assert_eq!(
        c.eval("[...document.querySelectorAll('#flowsteps li.kit-is-failed')].map((li) => li.dataset.key)"),
        json!(["plan"]),
        "the drifted step flashes, and only it"
    );
    assert_eq!(
        c.eval("document.querySelector('#flowsteps [role=status]').textContent"),
        json!("2 of 7 steps finished"),
        "a drift waits on no step, so the count"
    );
    assert_eq!(
        c.eval("[...document.querySelectorAll('#flowsteps li[data-state=finished] > .kit-steps__row')].map((r) => r.getAttribute('aria-label'))"),
        json!(["Think", "Spec"]),
        "a finished step's name drops the empty state word"
    );

    c.eval("cosFlowsTest.openFlow('alpha-api/t-a000')");
    assert_eq!(
        c.eval("(() => { const li = document.querySelector('#flowsteps li[data-state=awaiting_approval]'); return [li?.querySelector('.kit-steps__title').textContent ?? null, document.querySelector('#flowsteps [role=status]').textContent]; })()"),
        json!(["Spec", "Spec: Needs you"]),
        "at the gate the live region names the step that needs you"
    );

    reduce(&mut c);
    c.eval("cosFlowsTest.openFlow('alpha-api/t-b000')");
    assert_eq!(
        animations(&mut c),
        json!(0),
        "reduced motion settles the steps"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn board_cards_carry_a_ring_whose_counts_are_in_the_name() {
    let (mut c, _page) = open("ring", &["--disable-gpu"], "?engine-test");
    let card = ".fcard[data-key='alpha-api/t-b000']";
    assert_eq!(
        c.eval(&format!(
            "document.querySelector(\"{card} button.open\").getAttribute('aria-label')"
        )),
        json!(
            "Request log on the new store, alpha-api: 1 of 6 beads done, 1 in progress, 1 ready, 3 blocked"
        ),
    );
    assert_eq!(
        c.eval(&format!(
            "(() => {{ const r = document.querySelector(\"{card} .ring\"); return [r.getAttribute('aria-hidden'), [...r.querySelectorAll('path.ring__arc')].map((p) => p.dataset.state), r.querySelector('.ring__n').textContent]; }})()"
        )),
        json!([
            "true",
            ["done", "active", "ready", "blocked", "blocked", "blocked"],
            "1"
        ]),
        "one arc per bead in bead order, the done count in the centre"
    );
    assert_eq!(
        c.eval(&format!(
            "document.querySelectorAll(\"{card} path.ring__arc.kit-is-connecting[pathLength='1']\").length"
        )),
        json!(6),
        "the arcs draw in"
    );
    assert_eq!(
        c.eval(&format!(
            "document.querySelectorAll(\"{card} [role=img]\").length"
        )),
        json!(0),
        "the counts are said once, in the button's name"
    );
    assert_eq!(
        c.eval("document.querySelector(\".fcard[data-key='alpha-api/t-a000'] .ring\")"),
        json!(null),
        "a flow with no beads has no ring"
    );
    reduce(&mut c);
    c.eval("cosFlowsTest.go('map')");
    c.eval("cosFlowsTest.go('board')");
    assert_eq!(
        animations(&mut c),
        json!(0),
        "reduced motion settles the rings"
    );
}

/// Narrows the viewport to `rem` root ems (16 px each).
fn narrow(c: &mut Chrome, rem: u32) {
    c.call(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": rem * 16, "height": 800, "deviceScaleFactor": 1, "mobile": false }),
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn the_scene_heads_the_board_while_a_flow_waits_on_you() {
    let (mut c, _page) = open("scene", &["--disable-gpu"], "?engine-test");
    assert_eq!(
        c.eval("(() => { const s = document.getElementById('board').firstElementChild; return [s.className, s.getAttribute('aria-hidden'), !!s.querySelector('svg')]; })()"),
        json!(["scene", "true", true]),
        "the scene heads the Board"
    );
    assert_eq!(
        c.eval("[...document.querySelectorAll('#board .scene text')].map((t) => t.textContent)"),
        json!([
            "Rate limits per tenant",
            "No beads yet",
            "No beads yet",
            "No beads yet",
            "You approve",
            "Agents build"
        ]),
        "the first flow that waits on you, branching into its beads and two roles"
    );
    assert_eq!(
        c.eval("[document.querySelectorAll('#board .scene path.kit-is-connecting[pathLength=\"1\"]').length > 0, document.querySelectorAll('#board .scene g.kit-is-appearing').length]"),
        json!([true, 6]),
        "links draw, nodes settle"
    );
    narrow(&mut c, 30);
    assert_eq!(
        c.eval("[...document.querySelectorAll('#board .scene text')].every((t) => getComputedStyle(t).display === 'none')"),
        json!(true),
        "labels hide below 32rem"
    );
    narrow(&mut c, 80);
    click(&mut c, "#presets [data-preset=ready]");
    assert_eq!(
        c.eval("document.querySelector('#board .scene')"),
        json!(null),
        "no flow shown waits on you: no scene"
    );
    reduce(&mut c);
    click(&mut c, "#presets [data-preset=all]");
    assert_eq!(
        animations(&mut c),
        json!(0),
        "reduced motion settles the scene"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn the_scene_is_the_art_of_the_empty_board() {
    cdp::chrome_path();
    let dir = cdp::profile("flows-scene-empty");
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("flows.html");
    std::fs::write(
        &page,
        cos::flows::page_html(&cos::flows::Scan::default(), true).unwrap(),
    )
    .unwrap();
    let mut c = Chrome::start(&dir, &["--disable-gpu"]);
    c.open(&format!("file://{}?engine-test", page.display()));
    assert_eq!(
        c.eval("[!!document.querySelector('#board .empty'), document.querySelector('#board .scene')?.getAttribute('aria-hidden')]"),
        json!([true, "true"]),
        "no flows: the scene is the empty state's art"
    );
}

const BEAD_COLUMNS: &str = "Object.fromEntries([...document.querySelectorAll('#beadcols [data-col]')].map((col) => [col.dataset.col, [...col.querySelectorAll('[data-glide]')].map((e) => e.dataset.glide)]))";

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn beads_view_sorts_by_state_and_a_rescan_glides_a_bead_to_its_new_column() {
    let (mut c, page) = open("beads", &["--disable-gpu"], "?engine-test");
    c.eval("location.hash = 'v=flow&f=alpha-api/t-b000&fv=beads'");
    c.wait_for("document.body.dataset.flowView === 'beads'");
    assert_eq!(
        c.eval("[...document.querySelectorAll('.flowtabs [role=tab]')].map((t) => t.textContent + ':' + t.getAttribute('aria-selected'))"),
        json!(["Graph:false", "Beads:true", "Lanes:false"])
    );
    assert_eq!(
        c.eval("[...document.querySelectorAll('#beadcols [data-col] h4 span:first-child')].map((e) => e.textContent)"),
        json!(["Ready", "In progress", "Blocked", "Done"])
    );
    assert_eq!(
        c.eval(BEAD_COLUMNS),
        json!({ "ready": ["t-b003"], "active": ["t-b002"], "blocked": ["t-b004", "t-b005", "t-b006"], "done": ["t-b001"] }),
        "status, then ready, then open blockers decide the column"
    );
    assert!(
        c.eval("document.querySelector('#beadcols [data-glide=t-b002]').textContent")
            .as_str()
            .unwrap()
            .contains("claude 1a2b"),
        "a card carries its owner"
    );
    click(&mut c, "#beadcols [data-glide=t-b004]");
    assert_eq!(
        c.eval("cosFlowsTest.state().bead"),
        "t-b004",
        "a card opens the drawer"
    );
    key(&mut c, "Escape", "Escape", 27, 0);

    // The scan finds t-b003 started: the page on disk changes, as a rescan's would.
    let html = std::fs::read_to_string(&page.page).unwrap();
    let was = r#""id":"t-b003","title":"US-003: read path","status":"open""#;
    assert!(html.contains(was), "the fixture's t-b003 moved in the JSON");
    std::fs::write(
        &page.page,
        html.replace(was, &was.replace("\"open\"", "\"in-progress\"")),
    )
    .unwrap();
    c.call("Animation.enable", json!({}));
    c.call(
        "Animation.setPlaybackRate",
        json!({ "playbackRate": 0.001 }),
    );
    key(&mut c, "r", "KeyR", 82, 0);
    c.wait_for("location.search.includes('fresh=1') && document.readyState === 'complete' && document.body.dataset.flowView === 'beads'");
    assert_eq!(
        c.eval("document.querySelector('#beadcols [data-glide=t-b003]').getAnimations().map((a) => a.effect.getKeyframes()[0].transform.startsWith('translate'))"),
        json!([true]),
        "the moved card slides from its old column"
    );
    assert_eq!(
        c.eval(BEAD_COLUMNS),
        json!({ "ready": [], "active": ["t-b002", "t-b003"], "blocked": ["t-b004", "t-b005", "t-b006"], "done": ["t-b001"] })
    );
    assert_eq!(
        c.data("moved"),
        "t-b003",
        "the bead that changed column glides"
    );
    assert!(
        !c.eval("location.hash").as_str().unwrap().contains("was="),
        "the columns before the rescan leave the hash once read"
    );

    reduce(&mut c);
    c.eval("location.replace(location.href.split('#')[0] + '#v=flow&f=alpha-api/t-b000&fv=beads&was=t-b003.r')");
    c.reload();
    c.wait_for("document.body.dataset.flowView === 'beads'");
    assert_eq!(c.data("moved"), "t-b003");
    assert_eq!(
        animations(&mut c),
        json!(0),
        "under reduce the move is instant"
    );
}

fn focused(c: &mut Chrome) -> Value {
    c.eval("document.activeElement?.dataset?.bead ?? null")
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn lanes_put_beads_in_hire_owner_and_unclaimed_lanes_by_column() {
    let (mut c, _page) = open("lanes", &["--disable-gpu"], "?engine-test");
    c.eval("location.hash = 'v=flow&f=alpha-api/t-b000&fv=lanes'");
    c.wait_for("document.body.dataset.flowView === 'lanes'");
    assert_eq!(
        c.eval("[...document.querySelectorAll('#lanesview [data-lane]')].map((l) => [l.dataset.lane, l.querySelector('.ln__name').textContent, [...l.querySelectorAll('[data-bead]')].map((b) => b.dataset.bead + '@' + b.parentElement.dataset.col)])"),
        json!([
            ["hire:run-read", "Read pathbuilder · working", ["t-b003@1"]],
            ["owner:ann", "ann", ["t-b001@0"]],
            ["owner:claude:1a2b3c4d", "claude 1a2b", ["t-b002@0"]],
            ["none", "Not yet claimed", ["t-b006@0", "t-b004@1", "t-b005@2"]]
        ]),
        "hires, then owners, then the unclaimed; a done hire has no lane; columns are layers"
    );
    assert_eq!(
        c.eval(
            "document.querySelector('#lanesview [data-bead=t-b005]').getAttribute('aria-label')"
        ),
        json!("t-b005 US-005: dashboards, Blocked, waits on t-b003, t-b004")
    );
    assert_eq!(
        c.eval("(() => { const s = document.querySelector('#lanesview svg.lanes__links'); return [s.getAttribute('aria-hidden'), s.getAttribute('viewBox'), s.querySelectorAll('path.lanes__edge').length, s.querySelectorAll('path.lanes__edge.open').length]; })()"),
        json!(["true", "0 0 3 4", 5, 3]),
        "one connector per blocker edge, dashed into an open blocker, in column and row units"
    );
    assert_eq!(
        c.eval("['t-b001', 't-b002', 't-b003', 't-b004'].map((id) => document.querySelector(`#lanesview [data-bead=${id}]`).className)"),
        json!(["lbox done kit-is-appearing", "lbox active kit-is-running", "lbox ready", "lbox blocked"])
    );
    assert_eq!(
        c.eval("!!document.querySelector('#lanesview [data-bead=t-b001] path.kit-is-done[pathLength=\"1\"]')"),
        json!(true),
        "a done box settles with a tick"
    );

    assert_eq!(
        c.eval("[...document.querySelectorAll('#lanesview [data-bead]')].filter((b) => b.tabIndex === 0).map((b) => b.dataset.bead)"),
        json!(["t-b003"]),
        "one tab stop into the lanes"
    );
    c.eval("document.querySelector('#lanesview [data-bead=t-b003]').focus()");
    let moves: [(&str, &str, u32, &str); 7] = [
        ("ArrowDown", "ArrowDown", 40, "t-b001"),
        ("ArrowDown", "ArrowDown", 40, "t-b002"),
        ("ArrowDown", "ArrowDown", 40, "t-b006"),
        ("ArrowRight", "ArrowRight", 39, "t-b004"),
        ("End", "End", 35, "t-b005"),
        ("Home", "Home", 36, "t-b006"),
        ("ArrowUp", "ArrowUp", 38, "t-b002"),
    ];
    for (k, code, vk, want) in moves {
        key(&mut c, k, code, vk, 0);
        assert_eq!(focused(&mut c), json!(want), "{k}");
    }
    for kind in ["keyDown", "keyUp"] {
        c.call(
            "Input.dispatchKeyEvent",
            json!({ "type": kind, "key": "Enter", "code": "Enter", "windowsVirtualKeyCode": 13, "text": "\r" }),
        );
    }
    assert_eq!(
        c.eval("cosFlowsTest.state().bead"),
        "t-b002",
        "Enter opens the drawer"
    );
    key(&mut c, "Escape", "Escape", 27, 0);

    narrow(&mut c, 36);
    assert_eq!(
        c.eval("(() => { const r = ['t-b006', 't-b004', 't-b005'].map((id) => document.querySelector(`#lanesview [data-bead=${id}]`).getBoundingClientRect()); return [r[0].left === r[1].left && r[1].left === r[2].left, r[0].top < r[1].top && r[1].top < r[2].top, getComputedStyle(document.querySelector('#lanesview svg.lanes__links')).display]; })()"),
        json!([true, true, "none"]),
        "narrow: one list per lane, no connectors"
    );
    narrow(&mut c, 80);
    reduce(&mut c);
    c.eval("cosFlowsTest.go('board')");
    c.eval("location.hash = 'v=flow&f=alpha-api/t-b000&fv=lanes'");
    c.wait_for("document.body.dataset.flowView === 'lanes'");
    assert_eq!(
        animations(&mut c),
        json!(0),
        "reduced motion settles the lanes"
    );
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn the_lanes_gate_breathes_and_asks_the_window_for_the_approval() {
    // The gate flow gains one planned bead, so the gate has a column to feed.
    let (mut c, world, _page) = framed_with("flows-lanes-gate", |html| {
        let at = html.find(r#""id":"t-a000""#).unwrap();
        let beads = at + html[at..].find(r#""beads":[]"#).unwrap();
        let bead = r#""beads":[{"id":"t-a001","title":"US-001: limiter","status":"open","parent":"t-a000","blocked_by":[],"ready":true,"priority":null,"owner":null,"created_at":null,"done_at":null,"done_reason":null,"layer":0,"criteria":[],"criteria_more":0,"open_blockers":[],"handoff":""}]"#;
        format!(
            "{}{bead}{}",
            &html[..beads],
            &html[beads + r#""beads":[]"#.len()..]
        )
    });
    let inside = |c: &mut Chrome, expr: &str| inside(c, &world, expr);
    for _ in 0..50 {
        if inside(&mut c, "document.body.dataset.view === 'board'") == json!(true) {
            break;
        }
        c.pause(std::time::Duration::from_millis(100));
    }
    inside(
        &mut c,
        "(() => { document.querySelector('[data-open=\"alpha-api/t-a000\"]').click(); document.querySelector('[data-fv=lanes]').click(); })()",
    );
    for _ in 0..50 {
        if inside(&mut c, "document.body.dataset.flowView === 'lanes'") == json!(true) {
            break;
        }
        c.pause(std::time::Duration::from_millis(100));
    }
    assert_eq!(
        inside(
            &mut c,
            "(() => { const g = document.querySelector('#lanesview .gate'), li = document.querySelector('#lanesview [data-bead=t-a001]').parentElement; return [g.querySelector('.gate__diamond').classList.contains('kit-is-waiting'), document.querySelectorAll('#lanesview svg.lanes__links path.kit-is-travelling[pathLength=\"1\"]').length, g.querySelector('button').textContent, li.dataset.col, li.style.left]; })()"
        ),
        json!([true, 1, "Review and approve", "0", "12rem"]),
        "the gate breathes before column 0, a light running along its connector"
    );
    inside(
        &mut c,
        "document.querySelector('#lanesview .gate button').click()",
    );
    c.pause(std::time::Duration::from_millis(200));
    assert_eq!(
        c.eval("got.filter((m) => m.type === 'cos-flows-gate')"),
        json!([{ "type": "cos-flows-gate", "repo": "alpha-api", "epic": "t-a000" }])
    );
}
