// Every value from the backend reaches the DOM through textContent or a
// property, never innerHTML: titles, verdicts and activity lines quote agent
// output. `$`, `el`, `fill`, `icon` and `stateView` come from states.js.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const REPORT = navigator.userAgent.includes("Windows")
  ? "http://cosreport.localhost/"
  : "cosreport://localhost/";

const ui = {
  pane: "staff",
  from: "staff",
  role: null,
  run: null,
  agents: [],
  roles: [],
  defaults: {},
  scores: { scores: [], roles: [] },
  summary: null,
  deliverables: [],
  inflight: false,
  again: false,
  // Each list has been read once: before that an empty list is not "none".
  loaded: { agents: false, staff: false },
  slots: null,
};

// Basecoat's own variants read `.dark`; the tokens follow color-scheme.
const media = matchMedia("(prefers-color-scheme: dark)");
const applyTheme = () => document.documentElement.classList.toggle("dark", media.matches);
media.addEventListener("change", applyTheme);
applyTheme();

// ---- vocabulary: plain words for the supervisor's states

const LIVE = ["starting", "queued", "seeding", "running"];
const STATUS = {
  starting: ["starting", "info"],
  queued: ["waiting for a slot", "warn"],
  seeding: ["preparing its worktree", "info"],
  running: ["running", "info"],
  done: ["finished", "ok"],
  failed: ["failed", "bad"],
  killed: ["stopped", "muted"],
  timed_out: ["ran out of time", "bad"],
  lost: ["supervisor lost", "bad"],
};
const OUTCOME = {
  accepted: ["accepted", "ok"],
  needs_you: ["needs you", "warn"],
  rejected: ["rejected", "bad"],
};
const FLAG = { stalled: "gone quiet", looping: "repeating one call" };
const EXPECTS = {
  diff: "a diff against its base commit",
  file: "a file it leaves",
  none: "no change to the repo",
};

const money = (v) => (v == null ? "—" : `$${Number(v).toFixed(2)}`);
const duration = (s) => (s == null ? "—" : s < 90 ? `${s} s` : `${Math.round(s / 60)} min`);
const pct = (a, n) => (n ? `${Math.round((100 * a) / n)}%` : "—");
const badge = (text, tone) => el("span", { className: "badge", textContent: text, tone });
const status = (s) => badge(...(STATUS[s] ?? [s, "muted"]));
const outcome = (v) => (v ? badge(...(OUTCOME[v.outcome] ?? [v.outcome, "muted"])) : null);
const roleTitle = (id) =>
  id == null ? "Unassigned" : (ui.roles.find((r) => r.role.id === id)?.role.title ?? id);
const isLive = (a) => LIVE.includes(a.state.status);
const sm = (node, variant) => {
  node.dataset.variant = variant;
  node.dataset.size = "sm";
  return node;
};
const kv = (pairs) =>
  pairs
    .filter(([, v]) => v != null && v !== "" && !(Array.isArray(v) && !v.length))
    .flatMap(([k, v, mono]) => [
      el("dt", { textContent: k }),
      el(
        "dd",
        { className: mono ? "mono" : "" },
        ...(Array.isArray(v) ? [el("ul", { className: "codes" }, ...v.map((x) => el("li", { textContent: x })))] : [String(v)]),
      ),
    ]);

// ---- header: the business behind the gear, views

async function loadPaths() {
  const p = await invoke("paths");
  ui.business = p.business;
  ui.paths = p;
  $("biz-name").textContent = p.business;
  $("ws-name").textContent = p.business;
  $("biz-pin").textContent = p.pinned ? `Pinned by ${p.pinned}: this window stays on it.` : "";
  $("biz-pin").hidden = !p.pinned;
  fill(
    $("ws-paths"),
    ...kv([
      ["workspace", p.workspace, true],
      ["home", p.home, true],
      ["projects", p.projects, true],
      ["cos", p.cos ?? "not found", true],
    ]),
  );
  ui.slots = Number.isInteger(p?.slots) ? p.slots : null;
  ui.cos = p.cos;
  renderSlots();
}

function renderSlots() {
  const running = ui.agents.filter((a) => ["running", "seeding"].includes(a.state.status)).length;
  const queued = ui.agents.filter((a) => a.state.status === "queued").length;
  // Nothing until the backend has said how many slots there are.
  $("slots").hidden = ui.slots == null;
  fill($("slots"), ui.slots == null ? null : icon("cpu"), ui.slots == null ? "" : `${running} of ${ui.slots} slots busy${queued ? ` · ${queued} waiting` : ""}`);
  const live = running + queued;
  $("agent-count").textContent = String(live);
  $("agent-count").hidden = !live;
  $("runs-count").textContent = ui.agents.length ? String(ui.agents.length) : "";
}

$("ws-switch").addEventListener("click", () => {
  $("settings").open = false;
  openSwitcher(switchTo);
});

document.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape" && $("settings").open) $("settings").open = false;
});

const VIEWS = ["today", "agents", "flows", "docs", "dash"];
const SWITCH_PANES = ["staff", "runs", "scorecard"];

// The docs room names the business it was opened for; its saves carry it back.
const docsUrl = () => `${REPORT}docs?t=${Date.now()}&business=${encodeURIComponent(ui.business)}`;

// The report frame reloads on every report-changed whatever view is shown, so
// its brief counts as seen only while Today is visible. Until openFromHash has
// routed at launch, Today being visible says nothing about the view the owner
// will see.
const shownBrief = { last: null, routed: false };
const seenArgs = () =>
  shownBrief.last && shownBrief.routed && ui.business && $("view-today").classList.contains("active")
    ? { business: ui.business, file: shownBrief.last.file, mtime: shownBrief.last.mtime }
    : null;
const seenFailed = (e) => logDev("brief_seen", e);

function openTab(view, pane, dash) {
  for (const t of document.querySelectorAll(".app-tab")) {
    if (t.dataset.view === view && (view !== "dash" || t.dataset.dash === dash)) t.setAttribute("aria-current", "page");
    else t.removeAttribute("aria-current");
  }
  for (const v of document.querySelectorAll(".view")) {
    v.classList.toggle("active", v.id === `view-${view}`);
  }
  if (view !== "dash") window.hideDash();
  // Loaded on first open: the room scans every monitored repo.
  if (view === "docs" && !$("docs-frame").src) {
    frameLoad("docs-frame", docsUrl(), {
      title: "Opening the documentation room",
      waiting: () => "It reads the docs of every repo the profile watches.",
    });
  }
  // Every open reads the stores again: a flow's step changes as beads close.
  if (view === "flows") {
    flowsHost.load(false, true);
    gates.refresh().catch((e) => logDev("ritual_gates", e));
  }
  if (view === "agents" && SWITCH_PANES.includes(pane)) showPane(pane);
  const args = view === "today" && seenArgs();
  if (args) invoke("brief_seen", args).catch(seenFailed);
}

for (const tab of document.querySelectorAll(".app-tab")) {
  tab.addEventListener("click", () => openTab(tab.dataset.view, ""));
}

// The tabs load before routing, so the hash's dashboard exists before it is selected.
async function openFromHash() {
  await window.loadDashTabs();
  const h = new URLSearchParams(location.hash.slice(1));
  const tab = h.get("tab");
  if (tab === "dash") {
    const name = h.get("dash");
    const declared = [...document.querySelectorAll('.app-tab[data-view="dash"]')].some((t) => t.dataset.dash === name);
    if (declared) {
      openTab("dash", "", name);
      window.hideDash();
    } else {
      openTab("today", "");
    }
  } else if (VIEWS.includes(tab)) {
    openTab(tab, h.get("pane") ?? "");
  }
  if (h.get("settings") === "open") $("settings").open = true;
  shownBrief.routed = true;
  const args = seenArgs();
  if (args) invoke("brief_seen", args).catch(seenFailed);
}

// A Hire dialog with typed text, or contract edits, counts as unsaved.
const hireHasEdits = () =>
  $("hire-dialog").open && (hire.edited || $("h-prompt").value.trim() !== "" || $("h-title").value.trim() !== "");
let pendingDiscard = null;

// Nothing is written to disk; the reload drops every view and cache of the old one.
async function switchTo(name) {
  if (name === ui.business) return;
  if (hireHasEdits() && pendingDiscard !== name) {
    pendingDiscard = name;
    throw new Error("The Hire dialog has unsaved edits. Pick the business again to discard them.");
  }
  await invoke("switch_business", { args: { name } });
  const current = document.querySelector('.app-tab[aria-current="page"]');
  const tab = current?.dataset.view ?? "today";
  const pane = ui.pane === "role" ? "staff" : ui.pane === "run" ? ui.from : ui.pane;
  // Dashboard names contain spaces ("Control room").
  location.hash =
    tab === "dash" ? `tab=dash&dash=${encodeURIComponent(current.dataset.dash)}` : `tab=${tab}&pane=${pane}`;
  location.reload();
}
window.switchTo = switchTo;

// The Docs room saves through the app, not over its own scheme: only messages
// from that frame are honoured, and the reply goes back to it alone.
window.addEventListener("message", async (ev) => {
  const frame = $("docs-frame").contentWindow;
  if (ev.source !== frame || ev.data?.type !== "cos-docs-save") return;
  try {
    await invoke("docs_save", { business: ui.business, organize: ev.data.organize });
    frame.postMessage({ type: "cos-docs-saved", seq: ev.data.seq, ok: true }, "*");
  } catch (e) {
    frame.postMessage({ type: "cos-docs-saved", seq: ev.data.seq, ok: false, error: String(e) }, "*");
  }
});

// The report frame has no storage of its own (sandboxed, opaque origin): it
// reports its tab and scroll here so a reload after a run lands where it was,
// its Hire buttons open the hire dialog with that brief action picked, and
// its Run brief asks for a run: the message carries nothing the run uses.
const reportState = { tab: "", y: 0 };
window.addEventListener("message", (ev) => {
  const frame = $("report").contentWindow;
  if (ev.source !== frame) return;
  const d = ev.data ?? {};
  if (d.type === "cos-report-ready") {
    frame.postMessage({ type: "cos-host", hire: true, runBrief: true }, "*");
    if (briefNow) frame.postMessage(briefNow, "*");
    if (d.brief != null) {
      shownBrief.last = { file: d.brief, mtime: d.mtime };
      const args = seenArgs();
      if (args) invoke("brief_seen", args).catch(seenFailed);
    }
  } else if (d.type === "cos-run-brief") {
    runBrief();
  } else if (d.type === "cos-report-state") {
    reportState.tab = /^[a-z]+$/.test(d.tab ?? "") ? d.tab : "";
    reportState.y = Number.isFinite(d.y) ? Math.max(0, Math.round(d.y)) : 0;
  } else if (d.type === "cos-hire" && Number.isInteger(d.brief) && d.brief > 0) {
    document.querySelector('.app-tab[data-view="agents"]').click();
    openHire({ brief: d.brief }).catch((e) => logDev("hire from report", e));
  }
});

// A report already shown is swapped without a state over it: runs rewrite it often.
function reloadReport() {
  const hash = reportState.tab ? `#${reportState.tab}` : "";
  frameLoad("report", `${REPORT}?t=${Date.now()}&y=${reportState.y}${hash}`, {
    title: "Opening today's report",
    waiting: () => "It is put together from the latest brief, the goals and the workspace.",
    quiet: true,
  });
}

// ---- agents: panes

const PANES = ["staff", "role", "runs", "run", "scorecard"];

function showPane(name) {
  ui.pane = name;
  for (const p of PANES) $(`pane-${p}`).hidden = p !== name;
  $("hire-open").hidden = name === "role";
  const tab = name === "role" ? "staff" : name === "run" ? ui.from : name;
  for (const t of $("agent-panes").children) t.setAttribute("aria-selected", String(t.dataset.pane === tab));
  render();
}

for (const t of $("agent-panes").children) {
  t.addEventListener("click", () => {
    ui.from = t.dataset.pane;
    showPane(t.dataset.pane);
  });
}

function openRole(id) {
  ui.role = id;
  ui.from = "staff";
  showPane("role");
}

function openRun(id) {
  if (ui.run !== id) {
    ui.run = id;
    feed.reset(id);
    $("r-outcome").value = "passed";
    $("r-notes").value = "";
    $("r-evidence").value = "";
    $("r-msg").textContent = "";
  }
  showPane("run");
}

$("d-back").addEventListener("click", () => showPane(ui.from === "staff" && ui.role ? "role" : ui.from));

function render() {
  renderSlots();
  if (ui.pane === "staff") renderStaff();
  if (ui.pane === "role") renderRole();
  if (ui.pane === "runs") renderRuns();
  if (ui.pane === "run") renderRun();
  if (ui.pane === "scorecard") renderScores();
}

// ---- data

async function refreshAgents() {
  if (ui.inflight) {
    ui.again = true;
    return;
  }
  ui.inflight = true;
  try {
    ui.agents = await invoke("agents_list");
    ui.loaded.agents = true;
    $("agents-error").hidden = true;
    render();
  } catch (e) {
    logDev("agents_list", e);
    fill(
      $("agents-error"),
      stateView("unreachable", {
        title: "Could not read the agents",
        detail: `The runs of this business could not be read: ${e}`,
        action: { icon: "rotate-ccw", label: "Retry", onClick: refreshAgents },
      }),
    );
    $("agents-error").hidden = false;
  } finally {
    ui.inflight = false;
    if (ui.again) {
      ui.again = false;
      refreshAgents();
    }
  }
}

async function refreshStaff() {
  const [roles, scores, summary, deliverables] = await Promise.allSettled([
    invoke("agents_roles"),
    invoke("agents_scorecard"),
    invoke("brief_summary"),
    invoke("deliverables_open"),
  ]);
  if (roles.status === "fulfilled") {
    ui.roles = roles.value.roles;
    ui.defaults = roles.value.defaults ?? {};
    $("policy-note").hidden = !roles.value.note;
    $("policy-note-text").textContent = roles.value.note
      ? `${roles.value.note}. Agents are hired on the built-in defaults until the file is fixed.`
      : "";
  } else logDev("agents_roles", roles.reason);
  if (scores.status === "fulfilled") ui.scores = scores.value;
  else logDev("agents_scorecard", scores.reason);
  ui.summary = summary.status === "fulfilled" ? summary.value : null;
  ui.deliverables = deliverables.status === "fulfilled" ? deliverables.value : [];
  ui.loaded.staff = true;
  ui.staffError = roles.status === "rejected" ? String(roles.reason) : null;
  render();
}

// A drawn icon per role, so a card is found by its picture as well as its name.
const ROLE_ICON = {
  "chief-of-staff": "briefcase",
  builder: "hammer",
  reviewer: "search-check",
  "ritual-shaper": "compass",
  "ritual-goldfish": "fish",
  researcher: "book-open",
  investigator: "map",
  "docs-keeper": "file-text",
  "release-clerk": "package",
  tester: "flask-conical",
};
const roleIcon = (id) => (id == null ? "inbox" : (ROLE_ICON[id] ?? "bot"));
function roleGlyph(id) {
  const s = el("span", { className: "role-icon" }, icon(roleIcon(id)));
  s.dataset.role = id ?? "unassigned";
  return s;
}

// ---- staff

function renderStaff() {
  const head = ui.roles.find((r) => r.role.head)?.role;
  const card = $("head-card");
  if (!head) {
    card.replaceChildren(
      el("header", {}, el("h2", { className: "with-icon" }, icon("briefcase"), "Chief of staff")),
      el(
        "section",
        {},
        ui.staffError
          ? stateView("unreachable", {
              title: "The hiring policy could not be read",
              detail: ui.staffError,
              action: { icon: "rotate-ccw", label: "Retry", onClick: refreshStaff },
            })
          : stateView("reading", { title: "Reading the hiring policy…", line: true }),
      ),
    );
  } else {
    const s = ui.summary;
    const facts = !s
      ? [stateView("unreachable", { title: "The latest brief could not be read.", line: true, action: { icon: "rotate-ccw", label: "Retry", onClick: refreshStaff } })]
      : s.day
        ? [
            el(
              "ul",
              { className: "stats" },
              stat(s.day, "latest brief", "calendar"),
              stat(s.actions, "actions for you", "hand"),
              stat(s.prompts, "handoffs written", "send"),
              stat(s.openDeliverables, "open deliverables", "package"),
            ),
          ]
        : [stateView("empty", { title: `No brief saved yet: Run brief on the Today tab writes one. ${s.openDeliverables} open deliverables in the workspace.`, line: true })];
    card.replaceChildren(
      el(
        "header",
        {},
        el("h2", {}, icon("briefcase"), el("span", { className: "role-kicker", textContent: "Head" }), head.title),
        el("p", { textContent: head.mandate }),
      ),
      el("section", {}, ...facts, el("p", { className: "muted small", textContent: `${head.sandbox} ${head.judged}` })),
    );
  }
  if (!ui.loaded.staff) {
    fill($("roles"), stateView("reading", { title: "Reading the staff…", line: true }));
    return;
  }

  const cards = ui.roles.filter((r) => !r.role.head).map(({ role }) => roleCard(role.id, role.title, role.mandate));
  const unassigned = ui.agents.filter((a) => a.spec.role == null).length;
  const legacy = ui.scores.roles.find((r) => r.role == null);
  if (unassigned || legacy) cards.push(roleCard(null, "Unassigned", "Agents hired before roles, or without one."));
  $("roles").replaceChildren(...cards);
}

function stat(value, label, glyph) {
  return el("li", {}, el("b", { className: "num", textContent: String(value ?? "—") }), el("span", { className: "with-icon" }, glyph ? icon(glyph, { className: "xs" }) : null, label));
}

function roleCard(id, title, mandate) {
  const mine = ui.agents.filter((a) => (a.spec.role ?? null) === id);
  const running = mine.filter((a) => ["running", "seeding", "starting"].includes(a.state.status)).length;
  const waiting = mine.filter((a) => a.state.status === "queued").length;
  const flagged = mine.filter((a) => isLive(a) && a.state.flags?.length).length;
  const sc = ui.scores.roles.find((r) => (r.role ?? null) === id);
  const now = [];
  if (running) now.push(badge(`${running} running`, "info"));
  if (waiting) now.push(badge(`${waiting} waiting`, "warn"));
  if (flagged) now.push(badge(`${flagged} flagged`, "bad"));
  const record = sc
    ? el(
        "ul",
        { className: "stats compact" },
        stat(sc.runs, "runs", "repeat"),
        stat(pct(sc.accepted, sc.runs), "accepted", "check"),
        stat(sc.needs_you, "need you", "hand"),
        stat(money(sc.median_cost_usd), "median cost", "coins"),
      )
    : stateView("empty", { title: mine.length ? "No verdict yet." : "Not hired yet.", line: true });
  const hire = id
    ? iconButton("user-plus", "Hire", {
        variant: "outline",
        onClick: (ev) => {
          ev.stopPropagation();
          openHire({ role: id });
        },
      })
    : null;
  const card = el(
    "article",
    { className: "card role", tabIndex: 0 },
    el(
      "header",
      {},
      el("h3", { className: "with-icon" }, roleGlyph(id), title),
      el("p", { className: "clamp", textContent: mandate }),
      hire ? el("div", { className: "card-action" }, hire) : null,
    ),
    el("section", {}, now.length ? el("div", { className: "badges" }, ...now) : null, record),
  );
  card.dataset.size = "sm";
  const open = () => openRole(id);
  card.addEventListener("click", open);
  card.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") open();
  });
  return card;
}

// ---- role detail

function renderRole() {
  const pane = $("pane-role");
  const entry = ui.roles.find((r) => r.role.id === ui.role);
  const role = entry?.role;
  const t = entry?.terms ?? {};
  const back = iconButton("arrow-left", "Staff", { onClick: () => showPane("staff") });
  const title = role?.title ?? "Unassigned";
  const head = el("div", { className: "pane-head" }, back, el("h2", { className: "with-icon" }, roleGlyph(role?.id ?? null), title));
  if (role) {
    head.append(el("div", { className: "actions" }, iconButton("user-plus", "Hire", { variant: "primary", onClick: () => openHire({ role: role.id }) })));
  }

  const mine = ui.agents.filter((a) => (a.spec.role ?? null) === ui.role);
  const live = mine.filter(isLive);
  const judged = mine.filter((a) => a.state.verdict).slice(0, 8);
  const scores = ui.scores.scores.filter((s) => (s.role ?? null) === ui.role);

  const about = role
    ? el(
        "article",
        { className: "card" },
        el("header", {}, el("h3", { textContent: "Mandate" }), el("p", { textContent: role.mandate })),
        el(
          "section",
          {},
          el(
            "dl",
            { className: "kv" },
            ...kv([
              ["serves", role.serves],
              ["works in", role.sandbox],
              ["judged by", role.judged],
            ]),
          ),
        ),
      )
    : el("article", { className: "card" }, el("header", {}, el("h3", { textContent: "Unassigned" }), el("p", { textContent: "Agents hired before roles existed, or without one. Their verdicts still count in the scorecard." })));

  const contract = role
    ? el(
        "article",
        { className: "card" },
        el("header", {}, el("h3", { textContent: "Contract" }), el("p", { textContent: "What every hire in this role is judged on, before the repo adds its own terms." })),
        el(
          "section",
          {},
          el(
            "dl",
            { className: "kv" },
            ...kv([
              ["done when", role.done_when],
              ["expects", `${EXPECTS[role.expects ?? "diff"]}${role.output ? `: ${role.output}` : ""}`],
              ["checks", t.checks?.length ? t.checks : "the repo's checks", true],
              ["scope", t.scope?.length ? t.scope : "the repo's scope", true],
              ["deadline", t.deadline_secs ? `${t.deadline_secs} s` : null],
              ["budget", t.budget_usd ? money(t.budget_usd) : null],
              ["flag if quiet", t.stall_secs ? `${t.stall_secs} s` : null],
              ["flag if repeating", t.repeat ? `${t.repeat} identical calls` : null],
            ]),
          ),
        ),
      )
    : null;

  const scoreTable = scores.length
    ? table(["harness", "model", "repo", "runs", "accepted", "needs you", "rejected", "overclaimed", "cost", "time"], scores.map((s) => [s.harness, s.model ?? "default", s.repo ?? "(scratch)", s.runs, s.accepted, s.needs_you, s.rejected, s.overclaimed, money(s.median_cost_usd), duration(s.median_secs)]), 3)
    : stateView("empty", { title: "No verdicts in this role yet.", line: true });

  const liveList = live.length ? runTable(live) : stateView("empty", { title: "Nobody on this right now.", line: true });
  const verdicts = judged.length
    ? el(
        "ul",
        { className: "verdicts" },
        ...judged.map((a) => {
          const v = a.state.verdict;
          const li = el(
            "li",
            { tabIndex: 0 },
            el("code", { textContent: a.spec.id }),
            outcome(v),
            el("span", { className: "title", textContent: a.spec.title }),
            el("span", { className: "muted", textContent: v.reasons[0] ?? `${v.checks.length} check(s) passed, ${v.changed.length} file(s) changed` }),
          );
          li.addEventListener("click", () => {
            ui.from = "staff";
            openRun(a.spec.id);
          });
          return li;
        }),
      )
    : stateView("empty", { title: "No finished work in this role yet.", line: true });

  const waiting = role
    ? ui.deliverables.length
      ? el(
          "ul",
          { className: "waiting" },
          ...ui.deliverables.map((d) => {
            const go = iconButton("user-plus", "Hire for this", { variant: "outline", onClick: () => openHire({ role: role.id, deliverable: d.id }) });
            return el("li", {}, el("code", { textContent: d.id }), el("span", { className: "title", textContent: d.title }), el("span", { className: "muted", textContent: d.goal ?? d.goalId ?? "" }), go);
          }),
        )
      : stateView("empty", { title: "No open deliverables in the workspace.", line: true })
    : null;

  fill(
    pane,
    head,
    el("div", { className: "run-grid" }, about, contract),
    section("Scorecard", scoreTable),
    section("On it now", liveList),
    section("Recent verdicts", verdicts),
    waiting ? section("Open deliverables", waiting) : null,
  );
}

function section(title, body) {
  return el("section", { className: "block" }, el("h3", { className: "section-title", textContent: title }), body);
}

function table(head, rows, firstNumeric) {
  const t = el(
    "table",
    { className: "table" },
    el("thead", {}, el("tr", {}, ...head.map((h, i) => el("th", { textContent: h, className: i >= firstNumeric ? "n" : "" })))),
    el("tbody", {}, ...rows.map((r) => el("tr", {}, ...r.map((c, i) => el("td", { textContent: String(c), className: i >= firstNumeric ? "n num" : "" }))))),
  );
  return el("div", { className: "table-container" }, t);
}

// ---- runs

function runRow(a) {
  const { spec, state } = a;
  const tr = el(
    "tr",
    { className: spec.id === ui.run ? "selected" : "", tabIndex: 0 },
    el("td", {}, el("code", { textContent: spec.id })),
    el("td", { textContent: roleTitle(spec.role ?? null) }),
    el("td", {}, status(state.status), ...(state.flags ?? []).map((f) => badge(FLAG[f.kind] ?? f.kind, "bad"))),
    el("td", {}, outcome(state.verdict) ?? el("span", { className: "muted", textContent: isLive(a) ? "when it ends" : "judging" })),
    el("td", { textContent: spec.title }),
    el("td", { className: "n num", textContent: String(state.tool_calls) }),
    el("td", { className: "n num", textContent: money(state.cost_usd) }),
    el("td", { className: "latest", textContent: state.error || state.last, title: state.error || state.last }),
  );
  tr.addEventListener("click", () => {
    if (ui.pane === "runs") ui.from = "runs";
    openRun(spec.id);
  });
  tr.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") openRun(spec.id);
  });
  return tr;
}

function runTable(list) {
  return el(
    "div",
    { className: "table-container" },
    el(
      "table",
      { className: "table runs" },
      el("thead", {}, el("tr", {}, ...["id", "role", "status", "verdict", "title", "tools", "cost", "latest"].map((h, i) => el("th", { textContent: h, className: i === 5 || i === 6 ? "n" : "" })))),
      el("tbody", {}, ...list.map(runRow)),
    ),
  );
}

function renderRuns() {
  $("agents").tBodies[0].replaceChildren(...ui.agents.map(runRow));
  $("agents").hidden = !ui.agents.length;
  $("no-agents").hidden = ui.agents.length > 0;
  if (ui.agents.length) return;
  fill(
    $("no-agents"),
    ui.loaded.agents
      ? stateView("empty", {
          title: "No runs yet",
          detail: "Hire one from the staff. Each runs in its own worktree under a supervisor that outlives this window.",
          action: { icon: "user-plus", label: "Hire", variant: "primary", onClick: () => openHire({}) },
        })
      : stateView("reading", { title: "Reading the runs of this business…" }),
  );
}

// ---- one run

async function renderRun() {
  const row = ui.agents.find((a) => a.spec.id === ui.run);
  if (!row) {
    $("d-title").textContent = "This agent is gone";
    for (const id of ["d-badges", "d-flags", "d-verdict", "d-contract", "d-meta", "d-feed"]) $(id).replaceChildren();
    $("d-record-card").hidden = true;
    $("d-error").textContent = "Its directory was cleaned, or another window removed it.";
    $("d-error").hidden = false;
    return;
  }
  const { spec, state } = row;
  const live = isLive(row);
  $("d-title").textContent = spec.title || spec.id;
  fill(
    $("d-badges"),
    status(state.status),
    outcome(state.verdict),
    badge(roleTitle(spec.role ?? null), "muted"),
    state.verdict?.overclaimed ? badge("claimed success, the evidence disagrees", "bad") : null,
    ...(!live ? (state.flagged ?? []).map((k) => badge(`was ${FLAG[k] ?? k} during the run`, "warn")) : []),
  );
  $("d-flags").replaceChildren(
    ...(state.flags ?? []).map((f) =>
      el("div", { className: "alert", tone: "bad" }, el("h2", { textContent: `Flagged: ${FLAG[f.kind] ?? f.kind}` }), el("section", { textContent: `${f.detail}. The supervisor flags; it does not stop the agent.` })),
    ),
  );
  $("d-error").textContent = state.error ?? "";
  $("d-error").hidden = !state.error;
  $("d-stop").disabled = !live;
  $("d-rejudge").disabled = live;
  $("d-clean").disabled = live;

  $("d-verdict").replaceChildren(...verdictView(state.verdict, live));
  const c = spec.contract ?? {};
  $("d-contract").replaceChildren(
    ...kv([
      ["done when", c.done_when ?? "not stated"],
      ["yours", c.finish_line],
      ["expects", `${EXPECTS[c.expects ?? "diff"]}${c.output ? `: ${c.output}` : ""}`],
      ["checks", c.checks?.length ? c.checks : "none: nothing proves it is done", Boolean(c.checks?.length)],
      ["scope", c.scope?.length ? c.scope : "anywhere in the repo", Boolean(c.scope?.length)],
      ["deadline", `${spec.deadline_secs} s`],
      ["budget", money(spec.budget_usd)],
      ["flag if quiet", spec.limits ? `${spec.limits.stall_secs} s` : null],
      ["flag if repeating", spec.limits ? `${spec.limits.repeat} identical calls` : null],
      ["model", spec.model ?? "harness default"],
      ["base commit", spec.base ? spec.base.slice(0, 12) : null, true],
    ]),
  );
  $("d-meta").replaceChildren(
    ...kv([
      ["harness", spec.harness],
      ["workdir", spec.workdir, true],
      ["repo", spec.repo ?? "none: a scratch directory", true],
      ["source", spec.source],
      ["seeded", (state.seeded ?? []).join(", ") || "nothing"],
      ["build slot", (state.shimmed ?? []).join(", ") || "no build tools shimmed"],
      ["started", state.started ?? "waiting"],
      ["ended", state.ended],
      ["tool calls", String(state.tool_calls)],
      ["cost", money(state.cost_usd)],
      ["tokens", state.tokens != null ? state.tokens.toLocaleString() : null],
      ["exit", state.exit != null ? String(state.exit) : null],
      ["session", state.session, true],
      ["resume", state.session ? `cd ${spec.workdir} && ${spec.harness === "codex" ? "codex resume" : "claude --resume"} ${state.session}` : null, true],
    ]),
  );
  $("d-summary-box").hidden = !state.summary;
  $("d-summary").textContent = state.summary ?? "";
  renderRecorded(state);
  await feed.pull(spec.id, live);
}

// ---- activity feed
//
// One pull at a time per run: renderRun runs on every refresh, and two pulls
// from the same offset would show every line twice. Entries are keyed by
// their place in events.jsonl, so a range read again adds nothing.

const ENTRY = {
  start: ["start", "muted"],
  call: ["call", "info"],
  say: ["says", "muted"],
  denied: ["denied", "bad"],
  failed: ["failed", "bad"],
  end: ["ended", "ok"],
};

const feed = {
  run: null,
  from: 0,
  entries: new Map(),
  calls: new Map(),
  pulling: null,
  pulled: false,
  reset(id) {
    this.run = id;
    this.from = 0;
    this.entries = new Map();
    this.calls = new Map();
    this.pulled = false;
    fill($("d-feed"), el("li", { className: "feed-note" }, stateView("reading", { title: "Reading its activity…", line: true })));
  },
  async pull(id, live) {
    if (this.pulling) return this.pulling;
    this.pulling = (async () => {
      try {
        const t = await invoke("agents_tail", { id, from: this.from });
        if (this.run !== id) return;
        this.from = t.next;
        for (const e of t.entries) this.add(e);
        this.pulled = true;
        $("d-seed").hidden = !t.seedLog;
        $("d-seed-log").textContent = t.seedLog ?? "";
      } catch (e) {
        logDev("agents_tail", e);
      } finally {
        this.pulling = null;
      }
      this.empty(live);
    })();
    return this.pulling;
  },
  add(e) {
    if (this.entries.has(e.key)) return;
    $("d-feed").querySelector(".feed-note")?.remove();
    if (e.kind === "result") {
      const row = this.calls.get(e.call);
      if (row) {
        const mark = row.querySelector(".result");
        mark.replaceChildren(badge(e.ok === false ? "error" : "ok", e.ok === false ? "bad" : "ok"));
        if (e.detail) mark.append(el("span", { className: "muted", textContent: e.detail }));
        this.entries.set(e.key, row);
        return;
      }
    }
    const [word, tone] = e.kind === "result" ? [e.ok === false ? "error" : "result", e.ok === false ? "bad" : "ok"] : (ENTRY[e.kind] ?? [e.kind, "muted"]);
    const row = el(
      "li",
      { className: `entry ${e.kind}` },
      badge(word, tone),
      e.tool ? el("code", { className: "tool", textContent: e.tool }) : null,
      el("span", { className: e.kind === "call" ? "detail mono" : "detail", textContent: e.detail || (e.kind === "call" ? "(no arguments)" : "") }),
      el("span", { className: "result" }),
    );
    if (e.kind === "call" && e.call) this.calls.set(e.call, row);
    if (e.kind === "denied" && e.call && this.calls.has(e.call)) {
      this.calls.get(e.call).querySelector(".result").replaceChildren(badge("denied", "bad"), el("span", { className: "muted", textContent: e.detail }));
      this.entries.set(e.key, row);
      return;
    }
    this.entries.set(e.key, row);
    $("d-feed").append(row);
    $("d-feed").scrollTop = $("d-feed").scrollHeight;
  },
  empty(live) {
    const list = $("d-feed");
    if (this.entries.size) {
      list.querySelector(".feed-note")?.remove();
      return;
    }
    if (!this.pulled) return;
    const text = live ? "Nothing yet: the agent has not called a tool or said anything." : "No activity recorded.";
    fill(list, el("li", { className: "feed-note" }, stateView("empty", { title: text, line: true })));
  },
};

// ---- your result

function renderRecorded(state) {
  const judged = Boolean(state.verdict);
  $("d-record-card").hidden = !judged;
  const r = state.recorded;
  $("d-recorded").replaceChildren(
    ...(r
      ? [
          el(
            "div",
            { className: "badges" },
            badge(r.outcome, r.outcome === "passed" ? "ok" : r.outcome === "failed" ? "bad" : "warn"),
            el("span", { className: "muted", textContent: `recorded ${String(r.at).replace("T", " ").slice(0, 16)}` }),
          ),
          r.notes ? el("p", { textContent: r.notes }) : null,
          r.evidence?.length ? el("ul", { className: "codes" }, ...r.evidence.map((x) => el("li", { textContent: x }))) : null,
        ].filter(Boolean)
      : [stateView("empty", { title: "Nothing recorded yet.", line: true })]),
  );
  setLabel($("r-save"), r ? "Replace result" : "Record result");
}

$("d-record").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  $("r-msg").textContent = "Saving…";
  try {
    await invoke("agents_record", {
      business: ui.business,
      args: { id: ui.run, outcome: $("r-outcome").value, notes: $("r-notes").value, evidence: lines("r-evidence") },
    });
    $("r-msg").textContent = "Recorded on this run.";
    $("r-notes").value = "";
    $("r-evidence").value = "";
    await refreshAgents();
  } catch (e) {
    $("r-msg").textContent = String(e);
  }
});

function verdictView(v, live) {
  if (!v) return [stateView(live ? "reading" : "refreshing", { title: live ? "Judged when it ends." : "Being judged: the checks are running.", line: true })];
  const out = [el("div", { className: "badges" }, outcome(v), el("span", { className: "muted", textContent: `judged ${v.judged.replace("T", " ").slice(0, 19)}` }))];
  out.push(
    v.reasons.length
      ? el("ul", { className: "reasons" }, ...v.reasons.map((r) => el("li", { textContent: r })))
      : el("p", { textContent: "Every check passed, and the work stayed inside its scope." }),
  );
  if (v.checks.length) {
    out.push(el("h4", { textContent: "Checks cos re-ran" }));
    out.push(
      el(
        "ul",
        { className: "checks" },
        ...v.checks.map((k) =>
          el(
            "li",
            {},
            badge(k.passed ? "passed" : k.exit != null ? `exit ${k.exit}` : "did not finish", k.passed ? "ok" : "bad"),
            el("code", { textContent: k.cmd }),
            k.tail ? el("details", {}, el("summary", { textContent: "output" }), el("pre", { textContent: k.tail })) : null,
          ),
        ),
      ),
    );
  }
  out.push(el("h4", { textContent: `Changed files (${v.changed.length})` }));
  out.push(v.changed.length ? el("ul", { className: "codes" }, ...v.changed.map((f) => el("li", { textContent: f, tone: v.out_of_scope.includes(f) ? "bad" : undefined }))) : el("p", { className: "muted", textContent: "None." }));
  if (v.out_of_scope.length) out.push(el("p", { className: "error", textContent: `Outside its scope: ${v.out_of_scope.join(", ")}` }));
  return out;
}

$("d-stop").addEventListener("click", async () => {
  $("d-stop").disabled = true;
  try {
    await invoke("agents_stop", { business: ui.business, id: ui.run });
  } catch (e) {
    $("d-error").textContent = String(e);
    $("d-error").hidden = false;
  }
  refreshAgents();
});

$("d-rejudge").addEventListener("click", async () => {
  $("d-rejudge").disabled = true;
  $("d-verdict").replaceChildren(stateView("reading", { title: "Re-running the checks…", line: true }));
  try {
    await invoke("agents_rejudge", { business: ui.business, id: ui.run });
  } catch (e) {
    $("d-error").textContent = String(e);
    $("d-error").hidden = false;
  }
  await refreshAgents();
  refreshStaff();
});

$("d-clean").addEventListener("click", async () => {
  const id = ui.run;
  try {
    await invoke("agents_clean", { business: ui.business, id, force: false });
  } catch (e) {
    const force = confirm(`${e}\n\nRemove it anyway? The worktree's work exists nowhere else.`);
    if (!force) return;
    try {
      await invoke("agents_clean", { business: ui.business, id, force: true });
    } catch (e2) {
      $("d-error").textContent = String(e2);
      $("d-error").hidden = false;
      return;
    }
  }
  ui.run = null;
  showPane(ui.from);
  refreshAgents();
});

// ---- scorecard

function renderScores() {
  const rows = ui.scores.scores;
  $("scores").tBodies[0].replaceChildren(
    ...rows.map((s) =>
      el(
        "tr",
        {},
        el("td", { textContent: roleTitle(s.role ?? null) }),
        el("td", { textContent: s.harness }),
        el("td", { textContent: s.model ?? "default" }),
        el("td", { textContent: s.repo ?? "(scratch)" }),
        ...[s.runs, s.accepted, s.needs_you, s.rejected, s.overclaimed, s.flagged, money(s.median_cost_usd), duration(s.median_secs)].map((v) => el("td", { className: "n num", textContent: String(v) })),
      ),
    ),
  );
  $("scores").hidden = !rows.length;
  $("no-scores").hidden = rows.length > 0;
  if (rows.length) return;
  fill(
    $("no-scores"),
    ui.loaded.staff
      ? stateView("empty", { title: "No verdicts yet", detail: "A row lands here each time an agent ends and is judged." })
      : stateView("reading", { title: "Reading the ledger…" }),
  );
}

// ---- hire

const hire = { tasks: { brief: [], goals: [] }, edited: false, seq: 0, plan: null, bead: null };

async function loadRepos() {
  for (const r of await invoke("repos")) {
    $("h-repo").append(el("option", { value: r, textContent: r.split("/").pop() }));
  }
}

async function loadTasks(from) {
  const select = $(`h-${from}`);
  try {
    hire.tasks[from] = await invoke("agent_tasks", { from });
    select.replaceChildren(
      ...(hire.tasks[from].length
        ? hire.tasks[from].map((t) => el("option", { value: String(t.n), textContent: `${t.n} · ${t.title}` }))
        : [el("option", { value: "", textContent: "no handoffs in the latest brief" })]),
    );
  } catch (e) {
    hire.tasks[from] = [];
    select.replaceChildren(el("option", { value: "", textContent: String(e).includes("nothing saved") ? "no brief saved yet" : String(e) }));
  }
}

function work() {
  return document.querySelector('input[name="h-work"]:checked')?.value ?? "own";
}

const lines = (id) =>
  $(id)
    .value.split("\n")
    .map((s) => s.trim())
    .filter(Boolean);

function fillRole() {
  const entry = ui.roles.find((r) => r.role.id === $("h-role").value);
  if (!entry) return;
  const { role, terms } = entry;
  $("h-mandate").textContent = role.mandate;
  $("h-expects").textContent = `${EXPECTS[role.expects ?? "diff"]}${role.output ? `: ${role.output}` : ""}`;
  hire.edited = false;
  $("h-checks").value = (terms.checks ?? []).join("\n");
  $("h-checks").placeholder = "the repo's checks";
  $("h-scope").value = (terms.scope ?? []).join("\n");
  $("h-scope").placeholder = "the repo's scope";
  $("h-deadline").value = terms.deadline_secs ?? 1800;
  $("h-budget").value = terms.budget_usd ?? 5;
  if (role.harness) $("h-harness").value = role.harness;
  $("h-model").placeholder = role.model ?? "harness default";
  $("h-worktree").checked = role.repo !== false;
  checkFit();
}

// The dialog's fields as the backend reads them, or a reason they are not
// complete yet.
function hireArgs() {
  const args = {
    harness: $("h-harness").value,
    repo: $("h-repo").value || null,
    role: $("h-role").value,
    title: $("h-title").value || null,
    model: $("h-model").value || null,
    worktree: $("h-worktree").checked,
    deadlineSecs: Number($("h-deadline").value) || 1800,
    budgetUsd: Number($("h-budget").value) || 5,
    count: Number($("h-count").value) || 1,
    contract: hire.edited ? { checks: lines("h-checks"), scope: lines("h-scope") } : {},
  };
  const w = work();
  if (w === "bead") {
    if (!hire.bead) return { missing: "Open Hire from a bead in Flows, or pick another kind of work." };
    args.prompt = hire.bead.prompt;
    args.title = args.title || hire.bead.title;
    args.source = hire.bead.source;
    args.repo = args.repo || hire.bead.repo;
  } else if (w === "deliverable") {
    if (!$("h-deliverable").value) return { missing: "Pick a deliverable, or choose another kind of work." };
    args.deliverable = $("h-deliverable").value;
  } else if (w === "own") {
    args.prompt = $("h-prompt").value;
    if (!args.prompt.trim()) return { missing: "Write the prompt first." };
  } else {
    const t = hire.tasks[w].find((x) => String(x.n) === $(`h-${w}`).value);
    if (!t) return { missing: "The latest brief has no handoff to pick here." };
    args.prompt = t.prompt;
    args.title = args.title || t.title;
    args.source = t.source;
  }
  return { args };
}

const FIT = {
  fit: ["fits", "ok"],
  misfit: ["wrong role", "bad"],
  needs_you: ["needs you", "warn"],
  run_yourself: ["run it yourself", "warn"],
};

let fitTimer;
function checkFit() {
  clearTimeout(fitTimer);
  fitTimer = setTimeout(runFit, 150);
}

async function runFit() {
  const seq = ++hire.seq;
  const { args, missing } = hireArgs();
  if (!args) {
    hire.plan = null;
    renderFit(null, missing);
    return;
  }
  try {
    const plan = await invoke("agents_fit", { args });
    if (seq !== hire.seq) return;
    hire.plan = plan;
    renderFit(plan);
  } catch (e) {
    if (seq !== hire.seq) return;
    hire.plan = null;
    renderFit(null, String(e));
  }
}

function renderFit(plan, problem) {
  const call = $("h-fit-call");
  const notes = $("h-fit-notes");
  const actions = $("h-fit-actions");
  const cmd = $("h-fit-command");
  cmd.hidden = true;
  actions.replaceChildren();
  if (!plan) {
    fill(call, badge("not checked", "muted"), el("span", { className: "muted", textContent: ` ${problem ?? ""}` }));
    notes.replaceChildren();
    $("h-go").disabled = true;
    return;
  }
  const f = plan.fit;
  const [word, tone] = plan.refused && f.call === "fit" ? ["needs a repo", "bad"] : (FIT[f.call] ?? [f.call, "muted"]);
  fill(call, badge(word, tone), el("span", { textContent: ` ${plan.refused ?? "This role can do it from its sandbox."}` }));
  const items = [...f.notes, ...plan.warnings];
  if (plan.scratch_ok && !plan.repo) items.push("Runs in a scratch directory: this role needs no repo and is judged on its file.");
  notes.replaceChildren(...items.map((n) => el("li", { textContent: n })));
  if (f.command) {
    cmd.textContent = f.command;
    cmd.hidden = false;
  }
  if (f.suggest) {
    const title = roleTitle(f.suggest);
    actions.append(
      iconButton("user-plus", `Hire the ${title} instead`, {
        variant: "outline",
        onClick: () => {
          $("h-role").value = f.suggest;
          fillRole();
        },
      }),
    );
  }

  $("h-repo-from").textContent = plan.repo
    ? `${plan.repo.split("/").pop()}: ${plan.repo_from}${plan.candidates.length > 1 ? ` (also: ${plan.candidates.slice(1).map((c) => c.split("/").pop()).join(", ")})` : ""}`
    : plan.repo_from;
  const c = plan.contract;
  $("h-done").textContent = c.done_when ?? "not stated";
  $("h-finish").textContent = c.finish_line ?? "";
  $("h-finish").hidden = $("h-finish-k").hidden = !c.finish_line;
  $("h-expects").textContent = `${EXPECTS[c.expects ?? "diff"]}${c.output ? `: ${c.output}` : ""}`;
  if (!hire.edited) {
    $("h-checks").value = (c.checks ?? []).join("\n");
    $("h-scope").value = (c.scope ?? []).join("\n");
  }
  $("h-go").disabled = Boolean(plan.refused);
}

// A role card opens this with its role picked; a brief action, a deliverable
// or a Flows bead opens it with that work picked. A bead arrives as
// `flow_bead` read it from its store.
async function openHire({ role, deliverable, brief, bead } = {}) {
  hire.bead = bead ?? null;
  $("h-bead-choice").hidden = !bead;
  $("h-bead").hidden = !bead;
  if (bead) {
    const notes = [bead.title];
    if (bead.owner) notes.push(`Claimed by ${bead.owner}: the hire works beside them.`);
    if (!bead.criteria) notes.push("No acceptance criteria on this bead: the run is judged on its diff and checks alone.");
    $("h-bead").textContent = notes.join(" ");
  }
  $("h-msg").replaceChildren();
  $("h-title").value = "";
  $("h-role").replaceChildren(...ui.roles.filter((r) => !r.role.head).map((r) => el("option", { value: r.role.id, textContent: r.role.title })));
  if (role) $("h-role").value = role;
  $("h-deliverable").replaceChildren(
    ...(ui.deliverables.length
      ? ui.deliverables.map((d) => el("option", { value: d.id, textContent: `${d.id} · ${d.title}` }))
      : [el("option", { value: "", textContent: "no open deliverables" })]),
  );
  if (deliverable) $("h-deliverable").value = deliverable;
  $("h-repo").value = "";
  if (bead) {
    if (![...$("h-repo").options].some((o) => o.value === bead.repo)) {
      $("h-repo").append(el("option", { value: bead.repo, textContent: bead.repo.split("/").pop() }));
    }
    $("h-repo").value = bead.repo;
    $("h-title").value = bead.title;
  }
  const pick = bead ? "bead" : brief ? "brief" : deliverable || ui.deliverables.length ? "deliverable" : "own";
  document.querySelector(`input[name="h-work"][value="${pick}"]`).checked = true;
  fillRole();
  $("hire-dialog").showModal();
  await Promise.all([loadTasks("brief"), loadTasks("goals")]);
  if (brief && hire.tasks.brief.some((t) => t.n === brief)) $("h-brief").value = String(brief);
  checkFit();
}

$("hire-open").addEventListener("click", () => openHire({}));
$("h-role").addEventListener("change", fillRole);
for (const id of ["h-deliverable", "h-brief", "h-goals", "h-repo"]) $(id).addEventListener("change", checkFit);
$("h-prompt").addEventListener("input", checkFit);
for (const id of ["h-checks", "h-scope"]) {
  $(id).addEventListener("input", () => {
    hire.edited = true;
    checkFit();
  });
}
for (const r of document.querySelectorAll('input[name="h-work"]')) r.addEventListener("change", checkFit);
$("h-cancel").addEventListener("click", () => $("hire-dialog").close());

$("hire-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const msg = $("h-msg");
  if (!ui.cos) {
    fill(msg, stateView("unreachable", { title: "No cos binary found: build it (cargo build) or set COS_BIN.", line: true }));
    return;
  }
  const { args, missing } = hireArgs();
  if (!args) return void fill(msg, stateView("empty", { title: missing, line: true }));
  fill(msg, stateView("reading", { title: "Starting the run: a worktree, then its supervisor…", line: true }));
  $("h-go").disabled = true;
  try {
    const specs = await invoke("agents_spawn", { business: ui.business, args });
    $("hire-dialog").close();
    msg.replaceChildren();
    await refreshAgents();
    ui.from = "runs";
    openRun(specs[0].id);
    document.querySelector('.app-tab[data-view="agents"]').click();
  } catch (e) {
    fill(msg, stateView("unreachable", { title: `Not started: ${e}`, line: true, action: { icon: "rotate-ccw", label: "Try again", onClick: () => $("hire-form").requestSubmit() } }));
  } finally {
    $("h-go").disabled = Boolean(hire.plan?.refused);
  }
});

// ---- run brief: one at a time, owned by the app; Today refreshes when it writes.
// The button sits on the report's tab bar; the window tells the frame the state.

let briefNow = null;

function tellBrief(state, text, title = "") {
  briefNow = { type: "cos-brief-state", state, text, title };
  $("report").contentWindow?.postMessage(briefNow, "*");
}

function renderBrief(s) {
  // A run for another business says nothing about this window's report.
  const mine = !s?.business || s.business === ui.business;
  const st = !s || !mine ? "idle" : s.running ? "running" : s.ok === true ? "done" : s.ok === false ? "error" : "idle";
  const text = !s || !mine
    ? ""
    : s.running
      ? "brief running…"
      : s.ok === true
        ? "brief done"
        : s.ok === false
          ? s.deferred
            ? "brief put off: the Mac was asleep"
            : "brief failed: hover for why"
          : "";
  tellBrief(st, text, mine ? (s?.said ?? "") : "");
  const pill = framePill("report-box", "report-status");
  if (s?.running && mine) fill(pill, icon("loader-circle", { className: "spin" }), s.started ? `brief running · started ${spanWords(nowSecs() - s.started)} ago` : "brief running…");
  pill.hidden = !(s?.running && mine);
  clearTimeout(renderBrief.poll);
  if (s?.running) renderBrief.poll = setTimeout(pollBrief, 2000);
}

async function pollBrief() {
  try {
    renderBrief(await invoke("brief_status"));
  } catch (e) {
    logDev("brief_status", e);
  }
}

async function runBrief() {
  if (briefNow?.state === "running") return;
  tellBrief("running", "brief starting…");
  try {
    renderBrief(await invoke("brief_run", { business: ui.business }));
  } catch (e) {
    tellBrief("error", String(e));
  }
}

// ---- flows: the approval gate. The flows page runs its own script in a
// sandboxed frame with an opaque origin and no IPC, so the cards and their
// Approve button live here, in the window, and the page asks for anything
// else by message: each message is checked here before it acts.

const FLOW_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$/;
const FLOW_KEYS = ["brief", "design", "plan", "review", "report", "pr"];
// Per business: pins and the view are names in that business's projects.
const flowsPrefsKey = () => `cos.flows.v1.${ui.business ?? ""}`;
const flowsHost = {
  hash: "",
  prefs() {
    try {
      return JSON.parse(localStorage.getItem(flowsPrefsKey()) || "{}") || {};
    } catch {
      return {};
    }
  },
  // Where the scan is: "reading 3 of 8 repos (api-gateway)", from `flows-progress`.
  progress: null,
  waiting() {
    const p = flowsHost.progress;
    return p ? `Reading ${p.done + 1} of ${p.total} repos (${p.repo}) with cn.` : "Every ritual flow in this business's projects, read with cn.";
  },
  // Every reload carries the page's own view (its hash) and the stored
  // preferences. A held page comes back at once; `fresh` (Refresh) scans first.
  load(fresh, quiet = false) {
    const q = new URLSearchParams({ t: String(Date.now()), prefs: JSON.stringify(this.prefs()) });
    if (fresh) q.set("fresh", "1");
    frameLoad("flows-frame", `${REPORT}flows?${q}#${this.hash}`, { title: "Reading your flows", waiting: this.waiting, quiet });
  },
  // Only the shape the page writes: known keys, short strings, at most 200 pins.
  savePrefs(p) {
    if (!p || typeof p !== "object") return;
    const pins = Array.isArray(p.pins) ? p.pins.filter((x) => typeof x === "string" && x.length <= 200).slice(0, 200) : [];
    const pick = (v, ok) => (ok.includes(v) ? v : undefined);
    const clean = {
      pins,
      side: pick(p.side, ["open", "closed"]),
      group: pick(p.group, ["none", "repo", "owner"]),
      sort: pick(p.sort, ["needs", "ready", "touched", "title", "id"]),
      engine: pick(p.engine, ["auto", "svg"]),
    };
    try {
      localStorage.setItem(flowsPrefsKey(), JSON.stringify(clean));
    } catch (e) {
      logDev("flows prefs", e);
    }
  },
  async open(repo, epic, key) {
    const box = $("flow-open-error");
    box.textContent = "";
    box.hidden = true;
    try {
      await invoke("flow_open", { repo, epic, key });
    } catch (e) {
      box.textContent = `Could not open the ${key} of ${epic}: ${e}`;
      box.hidden = false;
    }
  },
  // The page names the bead; `flow_bead` reads it again and refuses one
  // that is done or still waits on another.
  async hire(repo, epic, bead) {
    const box = $("flow-open-error");
    box.textContent = "";
    box.hidden = true;
    try {
      const b = await invoke("flow_bead", { repo, epic, bead });
      document.querySelector('.app-tab[data-view="agents"]').click();
      await openHire({ bead: b });
    } catch (e) {
      box.textContent = `Could not hire for ${bead}: ${e}`;
      box.hidden = false;
    }
  },
  async gate(repo, epic) {
    const find = () => [...$("flow-gates").querySelectorAll("article.gate")].find((a) => a.dataset.key === `${repo}/${epic}`);
    if (!find()) await gates.refresh().catch((e) => logDev("ritual_gates", e));
    const card = find();
    if (!card) return;
    card.scrollIntoView({ block: "nearest" });
    card.querySelector(".gate-actions button")?.focus();
  },
};

window.addEventListener("message", (ev) => {
  if (ev.source !== $("flows-frame").contentWindow) return;
  const d = ev.data ?? {};
  const ids = FLOW_ID.test(d.repo ?? "") && FLOW_ID.test(d.epic ?? "");
  if (d.type === "cos-flows-state" && typeof d.hash === "string" && d.hash.length <= 2000) {
    flowsHost.hash = d.hash.replace(/^#/, "");
  } else if (d.type === "cos-flows-prefs") {
    flowsHost.savePrefs(d.prefs);
  } else if (d.type === "cos-flows-gate" && ids) {
    flowsHost.gate(d.repo, d.epic);
  } else if (d.type === "cos-flows-hire" && ids && FLOW_ID.test(d.bead ?? "")) {
    flowsHost.hire(d.repo, d.epic, d.bead);
  } else if (d.type === "cos-flows-open" && ids && FLOW_KEYS.includes(d.key)) {
    flowsHost.open(d.repo, d.epic, d.key);
  } else if (d.type === "cos-flows-rescan") {
    if (typeof d.hash === "string" && d.hash.length <= 2000) flowsHost.hash = d.hash.replace(/^#/, "");
    flowsHost.load(true);
    gates.refresh().catch((e) => logDev("ritual_gates", e));
  }
});

const STEP_NAME = { think: "Think", spec: "Spec", approve: "Approve", plan: "Plan", build: "Build", review: "Review", ship: "Ship", reflect: "Reflect" };
const sha12 = (sha) => (sha ? sha.slice(0, 12) : "—");
const fileName = (p) => p.split("/").pop();
const counts = (r) => `${r.blocking} blocking, ${r.should} should, ${r.nit} nit`;

// The approval's refusals, in words that say what to do next.
function approveError(e) {
  const s = String(e);
  if (s.includes("not clean")) return "The newest review found problems, so it cannot back a plain approval. Read the review and have the design reviewed again, or approve anyway.";
  if (s.includes("different version")) return "The review read an earlier version of the design. Have it reviewed again, or approve without a current review.";
  if (s.includes("no approver name")) return s;
  if (s.includes("the epic's design is")) return `The epic names a different design file: ${s}`;
  return `Not approved: ${s}`;
}

const gates = {
  list: [],
  steps: [],
  confirming: null,
  busy: null,
  done: {},
  errors: {},

  failed: null,
  read: false,
  async refresh() {
    if (!this.read) {
      $("flow-gates").hidden = false;
      fill($("flow-gates"), stateView("reading", { title: "Reading the designs waiting for your approval…", line: true }));
    }
    try {
      const res = await invoke("ritual_gates");
      this.steps = res.steps ?? [];
      this.list = res.gates;
      this.failed = null;
      this.read = true;
    } catch (e) {
      logDev("ritual_gates", e);
      this.failed = String(e);
    }
    this.done = {};
    this.errors = {};
    this.confirming = null;
    this.render();
  },

  key: (g) => `${g.repo}/${g.epic}`,

  render() {
    const box = $("flow-gates");
    box.hidden = false;
    if (this.failed) {
      return fill(box, stateView("unreachable", { title: `The approval gate could not be read: ${this.failed}`, line: true, action: { icon: "rotate-ccw", label: "Retry", onClick: () => this.refresh() } }));
    }
    if (!this.list.length) return fill(box, stateView("clear", { title: "No design is waiting for your approval.", line: true }));
    fill(box, ...this.list.map((g) => this.card(g)));
  },

  strip(current) {
    const at = this.steps.indexOf(current);
    const ol = el(
      "ol",
      { className: "strip" },
      ...this.steps.map((s, i) => {
        const li = el("li", { textContent: STEP_NAME[s] ?? s, className: i < at ? "past" : "" });
        if (i === at) li.setAttribute("aria-current", "step");
        return li;
      }),
    );
    ol.setAttribute("aria-label", "Ritual steps");
    return ol;
  },

  open(g, path, errBox) {
    return async () => {
      errBox.textContent = "";
      try {
        await invoke("ritual_open", { repo: g.repo, epic: g.epic, path });
      } catch (e) {
        errBox.textContent = `Could not open ${fileName(path)}: ${e}`;
      }
    };
  },

  link(g, label, path, errBox) {
    const b = sm(el("button", { className: "btn", type: "button" }, icon("file-text"), el("span", { textContent: label })), "link");
    b.addEventListener("click", this.open(g, path, errBox));
    return b;
  },

  verdict(g) {
    const r = g.review;
    if (!r) return el("p", { className: "gate-verdict muted", textContent: "No review of this design yet." });
    const stale = r.current ? "" : " · it read an earlier version of the design";
    return el(
      "p",
      { className: "gate-verdict" },
      badge(r.verdict, r.verdict === "clean" && r.current ? "ok" : "warn"),
      ` Latest review: ${counts(r)}${stale}`,
    );
  },

  card(g) {
    const k = this.key(g);
    const done = this.done[k];
    const errBox = el("p", { className: "error small", role: "status" });
    if (this.errors[k]) errBox.textContent = this.errors[k];
    const files = el(
      "div",
      { className: "gate-files" },
      g.brief ? this.link(g, "Brief", g.brief, errBox) : null,
      this.link(g, "Design", g.design, errBox),
      g.review ? this.link(g, "Latest review", g.review.path, errBox) : null,
      el("span", { className: "sha", textContent: `sha256 ${sha12(g.design_sha256)}`, title: g.design_sha256 ?? g.design_error ?? "" }),
    );
    const history = g.history.length
      ? el(
          "details",
          { className: "small" },
          el("summary", { textContent: `Earlier reviews (${g.history.length})` }),
          el(
            "ul",
            { className: "rounds" },
            ...g.history.map((r) =>
              el("li", {}, this.link(g, fileName(r.path), r.path, errBox), badge(r.verdict, "muted"), el("span", { className: "muted", textContent: counts(r) })),
            ),
          ),
        )
      : null;
    const card = el(
      "article",
      { className: "gate" },
      el(
        "div",
        { className: "gate-head" },
        el("h3", { textContent: g.title, title: g.title }),
        el("span", { className: "id", textContent: g.epic }),
        el("span", { className: "repo", textContent: g.repo }),
      ),
      this.strip(done ? "plan" : g.step),
      files,
      this.verdict(g),
      done ? this.approved(done) : this.actions(g, k),
      errBox,
      history,
    );
    card.dataset.key = k;
    if (done) card.dataset.state = "approved";
    return card;
  },

  approved(done) {
    return el("p", { className: "gate-verdict" }, badge("approved", "ok"), ` ${done.check}`);
  },

  actions(g, k) {
    if (g.design_error) return el("p", { className: "error small", textContent: g.design_error });
    if (!g.by) return el("p", { className: "muted small", textContent: `No approver name: set "owner" in the workspace profile, or git user.name in ${g.repo}.` });
    if (this.confirming === k) return this.confirm(g, k);
    const errBox = el("p", { className: "error small", role: "status" });
    const ask = (label, variant) => {
      const b = sm(el("button", { className: "btn", type: "button" }, icon("badge-check"), el("span", { textContent: label })), variant);
      b.addEventListener("click", () => {
        this.confirming = k;
        delete this.errors[k];
        this.render();
      });
      return b;
    };
    const r = g.review;
    if (r && r.current && r.blocking > 0) {
      const n = r.blocking;
      const read = sm(el("button", { className: "btn", type: "button" }, icon("file-search"), el("span", { textContent: `Read ${n} blocking finding${n === 1 ? "" : "s"}` })), "primary");
      read.addEventListener("click", this.open(g, r.path, errBox));
      return el("div", { className: "gate-actions" }, read, ask("Approve anyway", "outline"), errBox);
    }
    if (g.clean) return el("div", { className: "gate-actions" }, ask("Approve and start Plan", "primary"));
    if (r && r.current) {
      const carried = [r.should && `${r.should} should`, r.nit && `${r.nit} nit`].filter(Boolean).join(", ");
      return el("div", { className: "gate-actions" }, ask(`Approve: ${carried} go to Plan`, "primary"));
    }
    return el("div", { className: "gate-actions" }, ask("Approve without a current review", "outline"));
  },

  confirm(g, k) {
    const busy = this.busy === k;
    const yes = sm(el("button", { className: "btn", type: "button", disabled: busy }, icon(busy ? "loader-circle" : "badge-check", { className: busy ? "spin" : "" }), el("span", { textContent: busy ? "Approving…" : "Approve and start Plan" })), "primary");
    const no = sm(el("button", { className: "btn", type: "button", disabled: busy }, icon("x"), el("span", { textContent: "Cancel" })), "ghost");
    no.addEventListener("click", () => {
      this.confirming = null;
      this.render();
    });
    yes.addEventListener("click", () => this.approve(g, k));
    const box = el(
      "div",
      { className: "gate-confirm" },
      el("p", {}, "Approve ", el("code", { textContent: g.design }), " at sha256 ", el("code", { textContent: sha12(g.design_sha256) }), ` as ${g.by}?`),
      el("p", {
        className: "muted small",
        textContent: g.clean
          ? `Records the digest on ${g.epic} with the clean review, then approves the epic in its task store.`
          : `There is no clean review of this version: ${g.epic} records that it was approved without review.`,
      }),
      el("div", { className: "gate-actions" }, yes, no),
    );
    box.setAttribute("role", "group");
    box.setAttribute("aria-label", "Confirm approval");
    return box;
  },

  async approve(g, k) {
    this.busy = k;
    this.render();
    try {
      const res = await invoke("ritual_approve", {
        business: ui.business,
        repo: g.repo,
        epic: g.epic,
        design: g.design,
        review: g.clean ? g.review.path : null,
        without_review: !g.clean,
      });
      this.done[k] = res;
      this.confirming = null;
      flowsHost.load(true);
    } catch (e) {
      this.errors[k] = approveError(e);
      this.confirming = null;
    } finally {
      this.busy = null;
      this.render();
    }
  },
};

// ---- start

let staffTimer;
listen("report-changed", () => {
  reloadReport();
  refreshStaff();
});
listen("agents-changed", () => {
  refreshAgents();
  // A verdict lands in the ledger as the agent ends.
  clearTimeout(staffTimer);
  staffTimer = setTimeout(refreshStaff, 800);
});
listen("brief-changed", (ev) => renderBrief(ev.payload));
// A scan behind a held Flows page: the pill says how far it is, and the
// page is swapped for the new one when it lands.
listen("flows-progress", (ev) => {
  flowsHost.progress = ev.payload;
  frameWaiting("flows-frame");
  const pill = framePill("flows-box", "flows-status");
  if (framing["flows-frame"]?.loaded && !framing["flows-frame"]?.over) {
    fill(pill, icon("refresh-cw", { className: "spin" }), `refreshing · ${ev.payload.done + 1} of ${ev.payload.total} repos`);
    pill.hidden = false;
  }
});
listen("flows-changed", () => {
  flowsHost.progress = null;
  framePill("flows-box", "flows-status").hidden = true;
  if ($("view-flows").classList.contains("active")) flowsHost.load(false, true);
});
$("biz-open").addEventListener("click", () => openSwitcher(switchTo));
listen("switch-business", () => openSwitcher(switchTo));
render();
loadPaths()
  .then(() => {
    loadRepos().catch((e) => logDev("repos", e));
    return openFromHash();
  })
  .then(pollBrief)
  .catch((e) => logDev("paths", e));
reloadReport();
refreshAgents();
refreshStaff();
