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
  runsAs: "fleet",
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

// ---- vocabulary: plain words for a run's states, and Herdr's own words

const LIVE = ["starting", "queued", "seeding", "running"];
const STATUS = {
  starting: ["starting", "info"],
  queued: ["waiting for a slot", "warn"],
  seeding: ["preparing its worktree", "info"],
  running: ["in Herdr", "info"],
  done: ["finished", "ok"],
  failed: ["failed", "bad"],
  killed: ["stopped", "muted"],
  timed_out: ["ran out of time", "bad"],
  lost: ["lost", "bad"],
};
// What its Herdr terminal says, shown as Herdr says it; never recomputed.
const HERDR = {
  idle: ["idle", "muted"],
  working: ["working", "info"],
  done: ["says it is done", "ok"],
  blocked: ["waiting on you in Herdr", "warn"],
  error: ["says it hit an error", "bad"],
};
const OUTCOME = {
  accepted: ["accepted", "ok"],
  needs_you: ["needs you", "warn"],
  rejected: ["rejected", "bad"],
};
// The run card's summary once judged: the outcome, then the first reason.
const verdictLine = (v) => (v ? `${OUTCOME[v.outcome]?.[0] ?? v.outcome}: ${v.reasons[0] ?? "every check passed, and the work stayed inside its scope."}` : null);
const EXPECTS = {
  diff: "a diff against its base commit",
  file: "a file it leaves",
  none: "no change to the repo",
};

const money = (v) => (v == null ? "—" : `$${Number(v).toFixed(2)}`);
const duration = (s) => (s == null ? "—" : s < 90 ? `${s} s` : `${Math.round(s / 60)} min`);
const pct = (a, n) => (n ? `${Math.round((100 * a) / n)}%` : "—");
const status = (s) => badge(...(STATUS[s] ?? [s, "muted"]));
const outcome = (v) => (v ? badge(...(OUTCOME[v.outcome] ?? [v.outcome, "muted"])) : null);
const roleTitle = (id) =>
  id == null ? "Unassigned" : (ui.roles.find((r) => r.role.id === id)?.role.title ?? id);
const isLive = (a) => LIVE.includes(a.state.status);
// A median cost, or why there is none: runs in Herdr measure no cost.
const cost = (s) => (s.median_cost_usd == null && s.unmeasured ? "not measured in Herdr" : money(s.median_cost_usd));
const RUN_ID = /^[0-9]{8}-[0-9]{6}-[0-9a-z]{4}$/;
// Herdr's word for the run, with what it waits on when it is blocked.
function herdrBadge(a) {
  const h = a.state.herdr;
  if (!h) return null;
  if (!h.live) return badge("ended in Herdr", "muted");
  const [text, tone] = HERDR[h.word] ?? [h.word ?? "no status yet", "muted"];
  return badge(h.word === "blocked" && h.kind ? `${text} (${h.kind})` : text, tone);
}
// ---- header: the business picker, views

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
  const busy = ui.agents.filter((a) => a.spec.terminal && isLive(a)).length;
  // Nothing until the backend has said how many slots there are.
  $("slots").hidden = ui.slots == null;
  fill($("slots"), ui.slots == null ? null : icon("cpu"), ui.slots == null ? "" : `${busy} of ${ui.slots} slots busy`);
  const needs = ui.agents.filter((a) => a.needs);
  $("agent-count").textContent = String(needs.length);
  $("agent-count").hidden = !needs.length;
  $("agent-count").title = needs.map((a) => `${a.spec.title}: ${a.needs}`).join("\n");
  $("runs-count").textContent = ui.agents.length ? String(ui.agents.length) : "";
}

const VIEWS = ["today", "agents", "flows", "tax", "docs", "dash"];
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
    if (tab === "agents" && h.get("run")) openRun(h.get("run"));
    if (tab === "today" && LINK_RUN.test(h.get("run") ?? "")) showRun(h.get("run"));
    if (tab === "today" && LINK_DAY.test(h.get("day") ?? "")) showDay(h.get("day"));
  }
  shownBrief.routed = true;
  const args = seenArgs();
  if (args) invoke("brief_seen", args).catch(seenFailed);
}

// A Hire dialog with typed text, or contract edits, counts as unsaved.
const hireHasEdits = () =>
  $("hire-dialog").open && (hire.edited || $("h-prompt").value.trim() !== "" || $("h-title").value.trim() !== "");
let pendingDiscard = null;

// Nothing is written to disk; the reload drops every view and cache of the old one.
// `hash` lands the reloaded window somewhere else than the view it was on.
async function switchTo(name, hash) {
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
    hash ?? (tab === "dash" ? `tab=dash&dash=${encodeURIComponent(current.dataset.dash)}` : `tab=${tab}&pane=${pane}`);
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
// its Run brief asks for a run: the message carries nothing the run uses. Its
// Not needed on a coverage row names the source and the window answers it.
const reportState = { tab: "", y: 0 };
window.addEventListener("message", (ev) => {
  const frame = $("report").contentWindow;
  if (ev.source !== frame) return;
  const d = ev.data ?? {};
  if (d.type === "cos-report-ready") {
    frame.postMessage({ type: "cos-host", hire: true, runBrief: true, sources: true }, "*");
    if (briefNow) frame.postMessage(briefNow, "*");
    tellHires();
    if (d.brief != null) {
      shownBrief.last = { file: d.brief, mtime: d.mtime };
      const args = seenArgs();
      if (args) invoke("brief_seen", args).catch(seenFailed);
    }
  } else if (d.type === "cos-run-brief") {
    runBrief();
  } else if (d.type === "cos-source-not-needed" && typeof d.id === "string" && d.id) {
    invoke("source_not_needed", { business: ui.business, id: d.id })
      .then(() => frame.postMessage({ type: "cos-source-state", id: d.id, ok: true }, "*"))
      .catch((e) => frame.postMessage({ type: "cos-source-state", id: d.id, ok: false, error: String(e) }, "*"));
  } else if (d.type === "cos-report-state") {
    reportState.tab = /^[a-z]+$/.test(d.tab ?? "") ? d.tab : "";
    reportState.y = Number.isFinite(d.y) ? Math.max(0, Math.round(d.y)) : 0;
  } else if (d.type === "cos-hire" && Number.isInteger(d.brief) && d.brief > 0) {
    document.querySelector('.app-tab[data-view="agents"]').click();
    openHire({ brief: d.brief }).catch((e) => logDev("hire from report", e));
  } else if (d.type === "cos-open-herdr" && RUN_ID.test(d.id ?? "")) {
    openInHerdr(d.id).catch((e) => logDev("open in herdr from report", e));
  }
});

// Today's inbox and hero pills follow the hires live: the read-only snapshot
// (never agents_list, which reconciles) in its own message, so the frame's
// cos-host state is never touched.
async function tellHires() {
  const frame = $("report").contentWindow;
  if (!frame || !ui.business) return;
  try {
    const hires = await invoke("agents_snapshot", { business: ui.business });
    frame.postMessage({ type: "cos-hires", hires }, "*");
  } catch (e) {
    logDev("agents_snapshot", e);
  }
}

// A report already shown is swapped without a state over it: runs rewrite it often.
function reloadReport() {
  const hash = reportState.tab ? `#${reportState.tab}` : "";
  const page = linkRun.day ? `brief/${linkRun.day}` : "";
  frameLoad("report", `${REPORT}${page}?t=${Date.now()}&y=${reportState.y}${hash}`, {
    title: "Opening today's report",
    waiting: () => "It is put together from the latest brief, the goals and the workspace.",
    quiet: true,
  });
}

// ---- agents: panes

const PANES = ["staff", "role", "runs", "run", "scorecard"];

function showPane(name) {
  ui.pane = name;
  const to = $(`pane-${name}`);
  const from = PANES.map((p) => $(`pane-${p}`)).find((p) => p !== to && !p.hidden && !p.inert) ?? null;
  for (const p of PANES) {
    const pane = $(`pane-${p}`);
    if (pane !== to && pane !== from && !pane.inert) pane.hidden = true;
  }
  swapPane(from, to);
  $("hire-open").hidden = name === "role";
  const tab = name === "role" ? "staff" : name === "run" ? ui.from : name;
  for (const t of $("agent-panes").children) t.setAttribute("aria-selected", String(t.dataset.pane === tab));
  render();
}

for (const [id, mode] of [["runs-as-fleet", "fleet"], ["runs-as-table", "table"]]) {
  $(id).addEventListener("click", () => {
    ui.runsAs = mode;
    renderRuns();
  });
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
    runPage.reset(id);
    $("d-say").value = "";
    $("d-reply-note").textContent = "";
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
  const running = mine.filter((a) => a.spec.terminal && isLive(a)).length;
  const waiting = mine.filter((a) => a.state.herdr?.live && a.state.herdr.word === "blocked").length;
  const sc = ui.scores.roles.find((r) => (r.role ?? null) === id);
  const now = [];
  if (running) now.push(badge(`${running} in Herdr`, "info"));
  if (waiting) now.push(badge(`${waiting} waiting on you in Herdr`, "warn"));
  const record = sc
    ? el(
        "ul",
        { className: "stats compact" },
        stat(sc.runs, "runs", "repeat"),
        stat(pct(sc.accepted, sc.runs), "accepted", "check"),
        stat(sc.needs_you, "need you", "hand"),
        stat(cost(sc), "median cost", "coins"),
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
            ]),
          ),
        ),
      )
    : null;

  const scoreTable = scores.length
    ? table(["harness", "model", "repo", "runs", "accepted", "needs you", "rejected", "overclaimed", "cost", "time"], scores.map((s) => [s.harness, s.model ?? "default", s.repo ?? "(scratch)", s.runs, s.accepted, s.needs_you, s.rejected, s.overclaimed, cost(s), duration(s.median_secs)]), 3)
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

// ---- runs

function runRow(a) {
  const { spec, state } = a;
  const tr = el(
    "tr",
    { className: spec.id === ui.run ? "selected" : "", tabIndex: 0 },
    el("td", {}, el("code", { textContent: spec.id })),
    el("td", { textContent: roleTitle(spec.role ?? null) }),
    el("td", {}, status(state.status), a.needs ? badge(a.needs, "warn") : null),
    el("td", {}, herdrBadge(a) ?? el("span", { className: "muted", textContent: spec.terminal ? "not yet" : "before Herdr" })),
    el("td", {}, outcome(state.verdict) ?? el("span", { className: "muted", textContent: isLive(a) ? "when it says done" : "judging" })),
    el("td", { textContent: spec.title }),
    el("td", { className: "latest", textContent: state.error ?? state.herdr?.msg ?? "", title: state.error ?? state.herdr?.msg ?? "" }),
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
      el("thead", {}, el("tr", {}, ...["id", "role", "status", "in Herdr", "verdict", "title", "latest"].map((h) => el("th", { textContent: h })))),
      el("tbody", {}, ...list.map(runRow)),
    ),
  );
}

const fromRuns = (id, reply = false) => {
  ui.from = "runs";
  openRun(id);
  if (reply) $("d-say").focus();
};
function renderRuns() {
  const fleetOn = ui.runsAs === "fleet";
  $("runs-as-fleet").setAttribute("aria-pressed", String(fleetOn));
  $("runs-as-table").setAttribute("aria-pressed", String(!fleetOn));
  $("agents").tBodies[0].replaceChildren(...ui.agents.map(runRow));
  $("agents").closest(".table-container").hidden = fleetOn || !ui.agents.length;
  $("fleet").hidden = !fleetOn || !ui.agents.length;
  $("no-agents").hidden = ui.agents.length > 0;
  if (ui.agents.length) {
    if (fleetOn) {
      const rows = ui.agents.map((a) => fleetRow(a, roleTitle(a.spec.role ?? null)));
      const focused = document.activeElement?.closest?.("#fleet") ? document.activeElement.dataset.id : null;
      fill($("fleet"), fleet(rows, { onOpen: fromRuns, onReply: (id) => fromRuns(id, true), onOpenHerdr: openInHerdr, onOpenRun: fromRuns, focusId: focused ?? null }));
    }
    return;
  }
  fill(
    $("no-agents"),
    ui.loaded.agents
      ? stateView("empty", {
          title: "No runs yet",
          detail: "Hire one from the staff. Each runs in its own worktree, in a Herdr terminal that outlives this window.",
          action: { icon: "user-plus", label: "Hire", variant: "primary", onClick: () => openHire({}) },
        })
      : stateView("reading", { title: "Reading the runs of this business…" }),
  );
}

// ---- one run: cos's verdict and its work here; the conversation is in Herdr

async function renderRun() {
  const row = ui.agents.find((a) => a.spec.id === ui.run);
  if (!row) {
    $("d-title").textContent = "This agent is gone";
    for (const id of ["d-badges", "d-herdr", "d-verdict", "d-contract", "d-meta"]) $(id).replaceChildren();
    $("d-record-card").hidden = true;
    $("d-reply").hidden = true;
    $("d-error").textContent = "Its directory was cleaned, or another window removed it.";
    $("d-error").hidden = false;
    return;
  }
  const { spec, state } = row;
  const live = isLive(row);
  const h = state.herdr;
  const talking = Boolean(h?.live) && state.status === "running";
  $("d-title").textContent = spec.title || spec.id;
  fill(
    $("d-badges"),
    status(state.status),
    herdrBadge(row),
    outcome(state.verdict),
    badge(roleTitle(spec.role ?? null), "muted"),
    state.verdict?.overclaimed ? badge("claimed success, the evidence disagrees", "bad") : null,
  );
  $("d-error").textContent = state.error ?? "";
  $("d-error").hidden = !state.error;
  fill($("d-herdr"), herdrItem(row));
  $("d-open").hidden = !h;
  $("d-reply").hidden = !talking;
  $("d-pause").disabled = !talking || h?.word !== "working";
  $("d-done").disabled = !talking;
  if (talking) {
    $("d-reply-note").textContent = h.word === "blocked" ? "It waits on you in Herdr: a message here is refused until you answer there." : "";
  }
  $("d-stop").disabled = !live;
  $("d-rejudge").disabled = state.status === "starting" || state.status === "seeding" || (talking && h?.word === "working");
  $("d-clean").disabled = live;

  const acts = { pause: () => $("d-pause").click(), done: () => $("d-done").click(), reply: () => $("d-say").focus(), herdr: () => $("d-open").click(), screen: () => { $("d-screen-box").open = true; runPage.screen(spec.id); } };
  const view = { phase: runPhase(row), activeMs: row.active_ms, live, paused: row.paused_now, line: h?.msg ?? "", verdict: verdictLine(state.verdict) };
  fill($("d-card"), runPanel(view, () => invoke("agents_timeline", { business: ui.business, id: spec.id }), { onAction: (k) => acts[k]() }));
  $("d-verdict").replaceChildren(...verdictView(state.verdict, live));
  const c = spec.contract ?? {};
  $("d-contract").replaceChildren(
    ...kv([
      ["done when", c.done_when ?? "not stated"],
      ["yours", c.finish_line],
      ["expects", `${EXPECTS[c.expects ?? "diff"]}${c.output ? `: ${c.output}` : ""}`],
      ["checks", c.checks?.length ? c.checks : "none: nothing proves it is done", Boolean(c.checks?.length)],
      ["scope", c.scope?.length ? c.scope : "anywhere in the repo", Boolean(c.scope?.length)],
      ["deadline", `${spec.deadline_secs} s: its stop key, then its terminal ends`],
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
      ["Herdr terminal", spec.terminal ? `${spec.terminal}${h?.address ? ` (${h.address})` : ""}` : "none: it ran before Herdr", true],
      ["Herdr says", h ? (h.live ? (h.word ?? "no status yet") : `ended (${h.reason ?? "exited"})`) : null],
      ["judged on", state.judged_on],
      ["started", state.started ?? "waiting"],
      ["ended", state.ended],
      ["cost", spec.terminal ? "not measured in Herdr" : money(state.cost_usd)],
      ["exit", state.exit != null ? String(state.exit) : null],
    ]),
  );
  $("d-summary-box").hidden = !state.summary;
  $("d-summary").textContent = state.summary ?? "";
  $("d-screen-box").hidden = !spec.terminal;
  renderRecorded(state);
  await runPage.seed(spec.id);
  if ($("d-screen-box").open) runPage.screen(spec.id);
}

// What the run page reads besides the list: its seed log, and its screen
// in Herdr while the Details fold is open. One read of each at a time.
const runPage = {
  run: null,
  seeding: null,
  reading: null,
  reset(id) {
    this.run = id;
    $("d-seed").hidden = true;
    $("d-seed-log").textContent = "";
    $("d-screen").textContent = "";
  },
  async seed(id) {
    if (this.seeding) return this.seeding;
    this.seeding = (async () => {
      try {
        const t = await invoke("agents_seed", { id });
        if (this.run !== id) return;
        $("d-seed").hidden = !t.seedLog;
        $("d-seed-log").textContent = t.seedLog ?? "";
      } catch (e) {
        logDev("agents_seed", e);
      } finally {
        this.seeding = null;
      }
    })();
    return this.seeding;
  },
  async screen(id) {
    if (this.reading) return this.reading;
    this.reading = (async () => {
      try {
        const text = await invoke("agents_screen", { id, lines: 80 });
        if (this.run === id) $("d-screen").textContent = text || "Nothing on its screen yet.";
      } catch (e) {
        logDev("agents_screen", e);
        $("d-screen").textContent = `Its screen could not be read: ${e}`;
      } finally {
        this.reading = null;
      }
    })();
    return this.reading;
  },
};

// Where the conversation is: Herdr, waiting on you, or over.
function herdrItem(row) {
  const { spec, state } = row;
  const h = state.herdr;
  const name = spec.terminal ?? "";
  if (!spec.terminal) {
    return stateView("asleep", { title: "Ran before cos followed hires in Herdr", detail: "It cannot be resumed. Its verdict and record stay; hire again to carry the work on." });
  }
  if (!h) {
    if (!LIVE.includes(state.status)) return el("p", { className: "muted", textContent: `It ended before Herdr started its terminal ${name}.` });
    return stateView("starting", { title: state.status === "seeding" ? "Preparing its worktree" : "Starting", detail: `Then it starts in Herdr as the terminal ${name}.`, line: true });
  }
  const open = { icon: "external-link", label: "Open in Herdr", variant: "primary", onClick: () => openInHerdr(spec.id) };
  if (h.live && h.word === "blocked") {
    return stateView("herdr_waiting", { title: `Waiting on you in Herdr (${h.kind ?? "a question"})`, detail: `Answer it in its terminal ${name}: approvals and questions happen in Herdr.`, action: open });
  }
  if (h.first_message && h.first_message.sent === false) {
    return stateView("needs_you", { title: "Its first message did not go in", detail: `Herdr said: ${h.first_message.reason ?? "no reason given"}. Read its screen in Herdr before sending it again.`, action: open });
  }
  if (h.live) {
    return el("p", { className: "muted", textContent: `It runs in the Herdr terminal ${name}${h.address ? ` (${h.address})` : ""}: talk to it there, or send a message below. cos judges it once, when Herdr says it is done or it ends.` });
  }
  return el("p", { className: "muted", textContent: `Its Herdr terminal ${name} has ended${h.reason ? ` (${h.reason})` : ""}. Its last screen is folded below.` });
}

async function openInHerdr(id) {
  try {
    const said = await invoke("agents_open", { id });
    if (ui.run === id) $("d-reply-note").textContent = said;
  } catch (e) {
    if (ui.run === id) {
      $("d-error").textContent = String(e);
      $("d-error").hidden = false;
    } else logDev("agents_open", e);
  }
}

$("d-screen-box").addEventListener("toggle", () => {
  if ($("d-screen-box").open && ui.run) runPage.screen(ui.run);
});
$("d-open").addEventListener("click", () => ui.run && openInHerdr(ui.run));

// Each of these asks Herdr through cos; the note under the box says what happened.
async function runAction(button, doing, act, done) {
  button.disabled = true;
  $("d-reply-note").textContent = doing;
  try {
    await act();
    $("d-reply-note").textContent = done;
  } catch (e) {
    $("d-reply-note").textContent = String(e);
  }
  button.disabled = false;
  await refreshAgents();
}

$("d-reply").addEventListener("submit", (ev) => {
  ev.preventDefault();
  const text = $("d-say").value.trim();
  if (!text) return;
  runAction($("d-send"), "Sending into its Herdr terminal…", async () => {
    await invoke("agents_say", { business: ui.business, id: ui.run, text });
    $("d-say").value = "";
  }, "Sent: it is in its Herdr terminal.");
});

$("d-say").addEventListener("keydown", (ev) => {
  if (ev.key === "Enter" && (ev.metaKey || ev.ctrlKey)) $("d-reply").requestSubmit();
});

$("d-pause").addEventListener("click", () =>
  runAction($("d-pause"), "Pressing its stop key…", () => invoke("agents_pause", { business: ui.business, id: ui.run }), "Paused: its turn stopped; it waits for your next message."),
);

$("d-done").addEventListener("click", () =>
  runAction(
    $("d-done"),
    "Ending its Herdr terminal, then judging it…",
    async () => {
      await invoke("agents_done", { business: ui.business, id: ui.run });
      refreshStaff();
    },
    "Done: its terminal ended and it was judged.",
  ),
);

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
  if (!v) return [stateView(live ? "reading" : "refreshing", { title: live ? "Judged once, when Herdr says it is done or it ends." : "Being judged: the checks are running.", line: true })];
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
        ...[s.runs, s.accepted, s.needs_you, s.rejected, s.overclaimed, cost(s), duration(s.median_secs)].map((v) => el("td", { className: "n num", textContent: String(v) })),
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

const hire = { tasks: { brief: [], goals: [] }, edited: false, seq: 0, plan: null, bead: null, herdr: null };

// Every hire runs in Herdr: without it the dialog says so, offers to start
// it, and Hire stays off.
async function checkHerdr() {
  const box = $("h-herdr");
  try {
    const s = await invoke("herdr_status");
    hire.herdr = s.up;
    if (s.up) {
      box.replaceChildren();
    } else {
      fill(
        box,
        stateView("herdr_down", {
          title: "Herdr isn't running: start it, or open Herdr",
          detail: `Every hire runs in a Herdr terminal, so nothing can be hired until it answers. ${s.why ?? ""}`,
          action: { icon: "power", label: "Start Herdr", variant: "primary", onClick: startHerdr },
        }),
      );
    }
  } catch (e) {
    hire.herdr = false;
    fill(box, stateView("unreachable", { title: "Could not ask whether Herdr is running", detail: String(e), action: { icon: "rotate-ccw", label: "Try again", onClick: checkHerdr } }));
  }
  $("h-go").disabled = hire.herdr === false || Boolean(hire.plan?.refused) || !hire.plan;
}

async function startHerdr() {
  fill($("h-herdr"), stateView("starting", { title: "Starting Herdr…", line: true }));
  try {
    await invoke("herdr_start");
  } catch (e) {
    logDev("herdr_start", e);
  }
  await checkHerdr();
}

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
  $("h-go").disabled = Boolean(plan.refused) || hire.herdr === false;
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
  checkHerdr();
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
  if (hire.herdr === false) {
    fill(msg, stateView("herdr_down", { title: "Herdr isn't running: start it, or open Herdr.", line: true }));
    return;
  }
  const { args, missing } = hireArgs();
  if (!args) return void fill(msg, stateView("empty", { title: missing, line: true }));
  fill(msg, stateView("reading", { title: "Hiring: its worktree first, then its Herdr terminal…", line: true }));
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
    $("h-go").disabled = Boolean(hire.plan?.refused) || hire.herdr === false;
  }
});

// ---- run brief: one at a time, owned by the app; Today refreshes when it writes.
// The button sits on the report's tab bar; the window tells the frame the state.

let briefNow = null;

function tellBrief(state, text, title = "") {
  briefNow = { type: "cos-brief-state", state, text, title };
  $("report").contentWindow?.postMessage(briefNow, "*");
}

const BRIEF_ENDED = {
  stopped: "brief stopped",
  lost: "brief lost: hover for why",
};

function renderBrief(s) {
  // `brief-changed` carries asks too; they have no place on the report's tab bar.
  if (s?.kind === "ask") return;
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
            : (BRIEF_ENDED[s.state] ?? "brief failed: hover for why")
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

// ---- links: a cos:// link opens Today on a run (the run panel, above the
// report) or on a day's brief. The backend checked the link against its
// grammar and switched the window when it named another business; a switch
// still reloads, so the reloaded window lands through the hash.

const LINK_RUN = /^r-[0-9]{8}-[0-9]{6}-[a-z0-9]{4}$/;
const LINK_DAY = /^[0-9]{4}-[0-9]{2}-[0-9]{2}$/;
const linkRun = { run: null, day: null, timer: null, reloaded: null };
const RUN_STATE = {
  queued: ["queued", "info"],
  running: ["running", "info"],
  done: ["done", "ok"],
  failed: ["failed", "bad"],
  deferred: ["put off", "warn"],
  stopped: ["stopped", "muted"],
  lost: ["lost", "bad"],
};
const RUN_OWNER = {
  daemon: "the cos daemon",
  app: "the Chief of Staff app",
  mcp: "Claude Desktop or Codex, without the daemon",
  self: "a terminal or the schedule",
};
const isLiveRun = (v) => v.state === "queued" || v.state === "running";

// An answer is model output: rendered as the docs room renders a document,
// marked then DOMPurify, and a link that is not http, https or mailto loses
// its target so it cannot move the window.
const MARKED = window.marked && new window.marked.Marked({ gfm: true, breaks: false, async: false });
const PURIFY = {
  RETURN_DOM_FRAGMENT: true,
  ALLOW_DATA_ATTR: false,
  FORBID_TAGS: ["style", "form", "button", "textarea", "select", "option", "iframe", "frame", "frameset", "object", "embed", "link", "meta", "base", "video", "audio", "source", "track", "picture", "svg", "math", "dialog", "img"],
  FORBID_ATTR: ["style", "id", "name", "class", "srcset", "formaction", "ping", "target", "autofocus", "tabindex"],
};
function markdown(text) {
  if (!MARKED || !window.DOMPurify) return el("pre", { className: "run-answer", textContent: text });
  const frag = window.DOMPurify.sanitize(MARKED.parse(String(text)), PURIFY);
  for (const a of frag.querySelectorAll("a")) {
    if (!safeUrl(a.getAttribute("href"))) a.removeAttribute("href");
  }
  return el("div", { className: "run-answer" }, frag);
}

function closeRunPanel() {
  const day = linkRun.day;
  clearTimeout(linkRun.timer);
  Object.assign(linkRun, { run: null, day: null, timer: null });
  fill($("run-panel"));
  $("run-panel").hidden = true;
  if (day) reloadReport();
}

function panelHead(title, glyph, ...rest) {
  return el(
    "div",
    { className: "run-panel-head" },
    el("h2", {}, icon(glyph), el("span", { textContent: title })),
    ...rest,
    el("div", { className: "run-panel-actions" }, iconButton("x", "Close", { iconOnly: true, onClick: closeRunPanel })),
  );
}

function showLinkRefused(reason) {
  clearTimeout(linkRun.timer);
  const day = linkRun.day;
  Object.assign(linkRun, { run: null, day: null });
  if (day) reloadReport();
  $("run-panel").hidden = false;
  fill(
    $("run-panel"),
    stateView("unreachable", {
      title: "This link was not opened",
      detail: reason,
      action: { icon: "x", label: "Dismiss", onClick: closeRunPanel },
    }),
  );
}

async function showRun(run) {
  clearTimeout(linkRun.timer);
  const day = linkRun.day;
  Object.assign(linkRun, { run, day: null, reloaded: null });
  if (day) reloadReport();
  $("run-panel").hidden = false;
  fill($("run-panel"), stateView("reading", { title: "Reading the run", line: true }));
  await refreshRun();
}

function showDay(day) {
  clearTimeout(linkRun.timer);
  Object.assign(linkRun, { run: null, day });
  $("run-panel").hidden = false;
  fill($("run-panel"), panelHead(`Brief of ${day}`, "newspaper", iconButton("sun", "Latest brief", { variant: "outline", onClick: closeRunPanel })));
  reloadReport();
}

async function refreshRun() {
  const run = linkRun.run;
  if (!run) return;
  clearTimeout(linkRun.timer);
  let v;
  try {
    v = await invoke("run_get", { business: ui.business, run });
  } catch (e) {
    if (linkRun.run !== run) return;
    fill(
      $("run-panel"),
      panelHead("Run", "cloud-off", el("span", { className: "id", textContent: run })),
      stateView("unreachable", {
        title: "Could not read the run",
        detail: String(e),
        action: { icon: "rotate-ccw", label: "Try again", onClick: refreshRun },
      }),
    );
    return;
  }
  if (linkRun.run !== run) return;
  renderRun(v);
  if (isLiveRun(v)) linkRun.timer = setTimeout(refreshRun, 2000);
}

function runBody(v) {
  const said = v.reason ?? undefined;
  switch (v.state) {
    case "queued":
      return stateView("starting", { title: "Waiting to start", line: true });
    case "running":
      return stateView("reading", { title: v.kind === "ask" ? "Answering" : "Writing the brief", line: true });
    case "done":
      if (v.kind === "ask") return v.answer ? markdown(v.answer) : stateView("empty", { title: "The answer came back empty", line: true });
      return stateView("clear", { title: "Brief ready: the report below shows it", line: true });
    case "lost":
      return stateView("unreachable", { title: "Run lost", detail: said ?? "Its process is gone; nothing more will come of it." });
    case "deferred":
      return stateView("asleep", { title: "Put off: the Mac was asleep", detail: said, line: true });
    default:
      return stateView("unreachable", { title: v.state === "stopped" ? "Stopped" : "Failed", detail: said, line: true });
  }
}

function renderRun(v) {
  const [word, tone] = RUN_STATE[v.state] ?? [v.state, "muted"];
  const live = isLiveRun(v);
  const stop = v.stoppable
    ? iconButton("square", "Stop", { variant: "outline", onClick: (e) => stopRun(e.currentTarget, v.run) })
    : null;
  const head = panelHead(
    v.kind === "ask" ? "Ask" : "Brief",
    v.kind === "ask" ? "message-square" : "newspaper",
    badge(word, tone),
    el("span", { className: "muted small", textContent: `started ${spanWords(nowSecs() - v.started)} ago` }),
    el("span", { className: "id", textContent: v.run }),
    stop,
  );
  const where = RUN_OWNER[v.owner] ?? v.owner;
  const facts = el(
    "dl",
    { className: "kv" },
    ...kv([
      ["started by", live && !v.stoppable ? `${where}: stop it there` : where],
      ["reason", v.reason],
    ]),
  );
  fill($("run-panel"), head, facts, runBody(v));
  if (v.kind === "brief" && v.state === "done" && linkRun.reloaded !== v.run) {
    linkRun.reloaded = v.run;
    reloadReport();
  }
}

async function stopRun(button, run) {
  button.disabled = true;
  setLabel(button, "Stopping…");
  try {
    await invoke("run_stop", { business: ui.business, run });
  } catch (e) {
    logDev("run_stop", e);
    button.after(stateView("unreachable", { title: "Could not stop it", detail: String(e), line: true }));
    button.disabled = false;
    setLabel(button, "Stop");
    return;
  }
  refreshRun();
}

async function openLink(nav) {
  if (!nav) return;
  if (nav.view === "settings-assistants") {
    window.openSettings();
    return;
  }
  openTab("today", "");
  if (!nav.shown) {
    showLinkRefused(nav.refused ?? "refused");
    return;
  }
  const run = nav.view === "run" && LINK_RUN.test(nav.run ?? "") ? nav.run : null;
  const day = nav.view === "brief" && LINK_DAY.test(nav.day ?? "") ? nav.day : null;
  if (!run && !day) return;
  if (nav.business !== ui.business) {
    // Only the user's own pick may discard Hire edits; a link never does.
    if (hireHasEdits()) {
      showLinkRefused("The Hire dialog has unsaved edits. Save or close it, then open the link again.");
      return;
    }
    const hash = run ? `tab=today&run=${run}` : `tab=today&day=${day}`;
    await switchTo(nav.business, hash).catch((e) => showLinkRefused(String(e)));
    return;
  }
  if (run) showRun(run);
  else showDay(day);
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
  flows: [],
  decided: null,
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
      this.flows = res.flows ?? [];
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
    const items = this.list.map((g) => this.item(g));
    fill(
      box,
      decisions(items, {
        onOpen: (g, path) => invoke("ritual_open", { repo: g.repo, epic: g.epic, path }),
        onAsk: (k) => this.ask(k),
        onApprove: (k) => this.approve(k),
        onCancel: (k) => this.cancel(k),
        focusNext: this.decided !== null,
      }),
    );
    this.decided = null;
  },

  item(g) {
    const k = this.key(g);
    const flow = this.flows.find((f) => f.repo === g.repo && f.epic === g.epic);
    return {
      ...g,
      key: k,
      names: this.steps,
      flow: flow ?? { step: "spec", beads: [] },
      done: this.done[k] ?? null,
      confirming: this.confirming === k,
      busy: this.busy === k,
      error: this.errors[k] ?? "",
    };
  },

  ask(k) {
    delete this.errors[k];
    this.confirming = k;
    this.render();
  },

  cancel(k) {
    this.confirming = null;
    this.render();
  },


  async approve(k) {
    const g = this.list.find((x) => this.key(x) === k);
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
      this.decided = k;
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
  tellHires();
  // A verdict lands in the ledger as the agent ends.
  clearTimeout(staffTimer);
  staffTimer = setTimeout(refreshStaff, 800);
});
listen("brief-changed", (ev) => renderBrief(ev.payload));
pollBrief();
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
listen("brief-changed", (ev) => {
  if (ev.payload?.run && ev.payload.run === linkRun.run) refreshRun();
});
listen("link-open", (ev) => openLink(ev.payload));
render();
loadPaths()
  .then(() => {
    loadRepos().catch((e) => logDev("repos", e));
    return openFromHash();
  })
  // A link that launched the app was held until now.
  .then(async () => openLink(await invoke("link_ready")))
  .then(pollBrief)
  .catch((e) => logDev("paths", e));
reloadReport();
refreshAgents();
refreshStaff();
