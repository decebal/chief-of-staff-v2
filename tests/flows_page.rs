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
        2,
        "the gate and the review findings need you"
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
    assert_eq!(num(&mut c, "flows"), 2);
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
        c.eval("document.querySelector('#steprail [aria-current]').textContent"),
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
    cdp::chrome_path();
    let dir = cdp::profile(name);
    let page = fixture::write(&dir.join("page"), 1001);
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
