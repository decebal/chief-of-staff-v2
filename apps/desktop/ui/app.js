// Every value from the backend reaches the DOM through textContent or a
// property, never innerHTML: titles, verdicts and activity lines quote agent
// output.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const REPORT = navigator.userAgent.includes("Windows")
  ? "http://cosreport.localhost/"
  : "cosreport://localhost/";

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...kids) => {
  const { tone, ...rest } = props;
  const n = Object.assign(document.createElement(tag), rest);
  if (tone) n.dataset.tone = tone;
  n.append(...kids.filter((k) => k != null && k !== false));
  return n;
};

// replaceChildren would print a missing optional child as the text "null".
const fill = (node, ...kids) => node.replaceChildren(...kids.filter((k) => k != null && k !== false));

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
};

const logDev = (where, e) => console.debug(`[cos] ${where}`, e);

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

// ---- header, workspace, views

async function loadPaths() {
  const p = await invoke("paths");
  ui.business = p.business;
  $("biz-name").textContent = p.business;
  $("biz-pin").textContent = `pinned by ${p.pinned}`;
  $("biz-pin").hidden = !p.pinned;
  $("ws-path").textContent = p.workspace;
  $("ws-path").title = `home ${p.home}\nprojects ${p.projects}\ncos ${p.cos ?? "not found"}`;
  ui.slots = p.slots;
  ui.cos = p.cos;
  renderSlots();
}

function renderSlots() {
  const running = ui.agents.filter((a) => ["running", "seeding"].includes(a.state.status)).length;
  const queued = ui.agents.filter((a) => a.state.status === "queued").length;
  $("slots").textContent = `${running} of ${ui.slots} slots busy${queued ? ` · ${queued} waiting` : ""}`;
  const live = running + queued;
  $("agent-count").textContent = String(live);
  $("agent-count").hidden = !live;
  $("runs-count").textContent = ui.agents.length ? String(ui.agents.length) : "";
}

const VIEWS = ["today", "agents", "flows", "docs"];
const SWITCH_PANES = ["staff", "runs", "scorecard"];

// The docs room names the business it was opened for; its saves carry it back.
const docsUrl = () => `${REPORT}docs?t=${Date.now()}&business=${encodeURIComponent(ui.business)}`;

function openTab(view, pane) {
  for (const t of document.querySelectorAll(".app-tab")) {
    if (t.dataset.view === view) t.setAttribute("aria-current", "page");
    else t.removeAttribute("aria-current");
  }
  for (const v of document.querySelectorAll(".view")) {
    v.classList.toggle("active", v.id === `view-${view}`);
  }
  // Loaded on first open: the room scans every monitored repo.
  if (view === "docs" && !$("docs-frame").src) {
    $("docs-frame").src = docsUrl();
  }
  // Every open reads the stores again: a flow's step changes as beads close.
  if (view === "flows") {
    $("flows-frame").src = `${REPORT}flows?t=${Date.now()}`;
  }
  if (view === "agents" && SWITCH_PANES.includes(pane)) showPane(pane);
  if (view === "agents") refreshOtherLive();
}

for (const tab of document.querySelectorAll(".app-tab")) {
  tab.addEventListener("click", () => openTab(tab.dataset.view, ""));
}

function openFromHash() {
  const h = new URLSearchParams(location.hash.slice(1));
  if (VIEWS.includes(h.get("tab"))) openTab(h.get("tab"), h.get("pane") ?? "");
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
  const tab = document.querySelector('.app-tab[aria-current="page"]').dataset.view;
  const pane = ui.pane === "role" ? "staff" : ui.pane === "run" ? ui.from : ui.pane;
  location.hash = `tab=${tab}&pane=${pane}`;
  location.reload();
}

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
// and its Hire buttons open the hire dialog with that brief action picked.
const reportState = { tab: "", y: 0 };
window.addEventListener("message", (ev) => {
  const frame = $("report").contentWindow;
  if (ev.source !== frame) return;
  const d = ev.data ?? {};
  if (d.type === "cos-report-ready") {
    frame.postMessage({ type: "cos-host", hire: true }, "*");
  } else if (d.type === "cos-report-state") {
    reportState.tab = /^[a-z]+$/.test(d.tab ?? "") ? d.tab : "";
    reportState.y = Number.isFinite(d.y) ? Math.max(0, Math.round(d.y)) : 0;
  } else if (d.type === "cos-hire" && Number.isInteger(d.brief) && d.brief > 0) {
    document.querySelector('.app-tab[data-view="agents"]').click();
    openHire({ brief: d.brief }).catch((e) => logDev("hire from report", e));
  }
});

function reloadReport() {
  const hash = reportState.tab ? `#${reportState.tab}` : "";
  $("report").src = `${REPORT}?t=${Date.now()}&y=${reportState.y}${hash}`;
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
    $("agents-error").hidden = true;
    render();
  } catch (e) {
    logDev("agents_list", e);
    const retry = el("button", { className: "btn", textContent: "Retry", type: "button" });
    retry.dataset.variant = "outline";
    retry.dataset.size = "sm";
    retry.addEventListener("click", refreshAgents);
    $("agents-error").replaceChildren(`Could not read the agents: ${e} `, retry);
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
  render();
}

// ---- staff

function renderStaff() {
  const head = ui.roles.find((r) => r.role.head)?.role;
  const card = $("head-card");
  if (!head) {
    card.replaceChildren(el("header", {}, el("h2", { textContent: "Chief of staff" }), el("p", { textContent: "Reading the hiring policy…" })));
  } else {
    const s = ui.summary;
    const facts = !s
      ? [el("p", { className: "muted", textContent: "The latest brief could not be read." })]
      : s.day
        ? [
            el(
              "ul",
              { className: "stats" },
              stat(s.day, "latest brief"),
              stat(s.actions, "actions for you"),
              stat(s.prompts, "handoffs written"),
              stat(s.openDeliverables, "open deliverables"),
            ),
          ]
        : [
            el("p", { className: "muted", textContent: `No brief saved yet: run cos brief. ${s.openDeliverables} open deliverables in the workspace.` }),
          ];
    card.replaceChildren(
      el(
        "header",
        {},
        el("h2", {}, el("span", { className: "role-kicker", textContent: "Head" }), head.title),
        el("p", { textContent: head.mandate }),
      ),
      el("section", {}, ...facts, el("p", { className: "muted small", textContent: `${head.sandbox} ${head.judged}` })),
    );
  }

  const cards = ui.roles.filter((r) => !r.role.head).map(({ role }) => roleCard(role.id, role.title, role.mandate));
  const unassigned = ui.agents.filter((a) => a.spec.role == null).length;
  const legacy = ui.scores.roles.find((r) => r.role == null);
  if (unassigned || legacy) cards.push(roleCard(null, "Unassigned", "Agents hired before roles, or without one."));
  $("roles").replaceChildren(...cards);
}

function stat(value, label) {
  return el("li", {}, el("b", { className: "num", textContent: String(value ?? "—") }), el("span", { textContent: label }));
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
        stat(sc.runs, "runs"),
        stat(pct(sc.accepted, sc.runs), "accepted"),
        stat(sc.needs_you, "need you"),
        stat(money(sc.median_cost_usd), "median cost"),
      )
    : el("p", { className: "muted", textContent: mine.length ? "No verdict yet." : "Not hired yet." });
  const hire = id
    ? el("button", { className: "btn", type: "button", textContent: "Hire", onclick: (ev) => { ev.stopPropagation(); openHire({ role: id }); } })
    : null;
  if (hire) {
    hire.dataset.variant = "outline";
    hire.dataset.size = "sm";
  }
  const card = el(
    "article",
    { className: "card role", tabIndex: 0 },
    el("header", {}, el("h3", { textContent: title }), el("p", { className: "clamp", textContent: mandate }), hire ? el("div", { className: "card-action" }, hire) : null),
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
  const back = el("button", { className: "btn", type: "button", textContent: "← Staff", onclick: () => showPane("staff") });
  back.dataset.variant = "ghost";
  back.dataset.size = "sm";
  const title = role?.title ?? "Unassigned";
  const head = el("div", { className: "pane-head" }, back, el("h2", { textContent: title }));
  if (role) {
    const hire = el("button", { className: "btn", type: "button", textContent: "Hire", onclick: () => openHire({ role: role.id }) });
    hire.dataset.variant = "primary";
    hire.dataset.size = "sm";
    head.append(el("div", { className: "actions" }, hire));
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
    : emptyNote("No verdicts in this role yet.");

  const liveList = live.length ? runTable(live) : emptyNote("Nobody on this right now.");
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
    : emptyNote("No finished work in this role yet.");

  const waiting = role
    ? ui.deliverables.length
      ? el(
          "ul",
          { className: "waiting" },
          ...ui.deliverables.map((d) => {
            const go = el("button", { className: "btn", type: "button", textContent: "Hire for this", onclick: () => openHire({ role: role.id, deliverable: d.id }) });
            go.dataset.variant = "outline";
            go.dataset.size = "sm";
            return el("li", {}, el("code", { textContent: d.id }), el("span", { className: "title", textContent: d.title }), el("span", { className: "muted", textContent: d.goal ?? d.goalId ?? "" }), go);
          }),
        )
      : emptyNote("No open deliverables in the workspace.")
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

function emptyNote(text) {
  return el("p", { className: "muted empty-note", textContent: text });
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
    $("d-feed").replaceChildren();
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
    const note = list.querySelector(".empty-entry");
    if (this.entries.size) {
      note?.remove();
      return;
    }
    if (!this.pulled) return;
    const text = live ? "Nothing yet: the agent has not called a tool or said anything." : "No activity recorded.";
    if (note) note.textContent = text;
    else list.append(el("li", { className: "empty-entry muted", textContent: text }));
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
      : [el("p", { className: "muted", textContent: "Nothing recorded yet." })]),
  );
  $("r-save").textContent = r ? "Replace result" : "Record result";
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
  if (!v) return [el("p", { className: "muted", textContent: live ? "Judged when it ends." : "Being judged: the checks are running." })];
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
  $("d-verdict").replaceChildren(el("p", { className: "muted", textContent: "Re-running the checks…" }));
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
}

// ---- hire

const hire = { tasks: { brief: [], goals: [] }, edited: false, seq: 0, plan: null };

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
  return document.querySelector('input[name="h-work"]:checked').value;
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
  if (w === "deliverable") {
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
    const b = el("button", { className: "btn", type: "button", textContent: `Hire the ${title} instead`, onclick: () => {
      $("h-role").value = f.suggest;
      fillRole();
    } });
    b.dataset.variant = "outline";
    b.dataset.size = "sm";
    actions.append(b);
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

async function openHire({ role, deliverable, brief } = {}) {
  $("h-msg").textContent = "";
  $("h-role").replaceChildren(...ui.roles.filter((r) => !r.role.head).map((r) => el("option", { value: r.role.id, textContent: r.role.title })));
  if (role) $("h-role").value = role;
  $("h-deliverable").replaceChildren(
    ...(ui.deliverables.length
      ? ui.deliverables.map((d) => el("option", { value: d.id, textContent: `${d.id} · ${d.title}` }))
      : [el("option", { value: "", textContent: "no open deliverables" })]),
  );
  if (deliverable) $("h-deliverable").value = deliverable;
  $("h-repo").value = "";
  const pick = brief ? "brief" : deliverable || ui.deliverables.length ? "deliverable" : "own";
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
    msg.textContent = "No cos binary found: build it (cargo build) or set COS_BIN.";
    return;
  }
  const { args, missing } = hireArgs();
  if (!args) return void (msg.textContent = missing);
  msg.textContent = "Starting…";
  $("h-go").disabled = true;
  try {
    const specs = await invoke("agents_spawn", { business: ui.business, args });
    $("hire-dialog").close();
    await refreshAgents();
    ui.from = "runs";
    openRun(specs[0].id);
    document.querySelector('.app-tab[data-view="agents"]').click();
  } catch (e) {
    msg.textContent = String(e);
  } finally {
    $("h-go").disabled = Boolean(hire.plan?.refused);
  }
});

// ---- other businesses

function showOtherLive(rows) {
  const n = rows.filter((r) => !r.app).reduce((sum, r) => sum + (r.live ?? 0), 0);
  const badge = $("other-live");
  badge.textContent = `${n} live in other businesses`;
  badge.hidden = !n;
}

async function refreshOtherLive() {
  try {
    showOtherLive(await invoke("businesses"));
  } catch (e) {
    logDev("businesses", e);
    $("other-live").hidden = true;
  }
}

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
$("biz-open").addEventListener("click", () => openSwitcher(switchTo));
$("other-live").addEventListener("click", () => openSwitcher(switchTo));
document.addEventListener("businesses-loaded", (e) => showOtherLive(e.detail));
listen("switch-business", () => openSwitcher(switchTo));
loadPaths().then(loadRepos).then(openFromHash).catch((e) => logDev("paths", e));
reloadReport();
refreshAgents();
refreshStaff();
