// The kit's window builders: DOM from plain data and callbacks, over states.js
// el(). app.js fetches and wires; it does not build these itself.

// ---- clocks: a live, unpaused clock counts once a second; a held one stays.

const pad2 = (n) => String(n).padStart(2, "0");
function clockText(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h ? `${h}:${pad2(m)}:${pad2(s % 60)}` : `${m}:${pad2(s % 60)}`;
}

function tickClocks(now = Date.now()) {
  for (const c of document.querySelectorAll("[data-clock-live]")) {
    c.textContent = clockText(Number(c.dataset.activeMs) + now - Number(c.dataset.renderedAt));
  }
}

let clockTimer = null;
function clock(activeMs, live) {
  const props = { className: "kit-clock", textContent: clockText(activeMs), dataset: { activeMs: String(activeMs), renderedAt: String(Date.now()) } };
  if (live) {
    props.dataset.clockLive = "true";
    clockTimer ??= setInterval(() => tickClocks(), 1000);
  }
  return el("time", props);
}

// ---- fleet

const FLEET_STATES = {
  starting: ["starting", "info"],
  working: ["working", "info"],
  "needs-you": ["needs you", "warn"],
  paused: ["paused", "muted"],
  done: ["done", "ok"],
  failed: ["failed", "bad"],
  lost: ["lost", "bad"],
  "timed-out": ["timed out", "bad"],
  stopped: ["stopped", "muted"],
};

function fleetStep(len, current, key) {
  if (!len) return null;
  switch (key) {
    case "ArrowDown":
    case "ArrowRight":
      return Math.min(current + 1, len - 1);
    case "ArrowUp":
    case "ArrowLeft":
      return Math.max(current - 1, 0);
    case "Home":
      return 0;
    case "End":
      return len - 1;
    default:
      return null;
  }
}

const FLEET_PHASE = {
  starting: "starting",
  queued: "starting",
  seeding: "starting",
  done: "done",
  failed: "failed",
  killed: "stopped",
  timed_out: "timed-out",
  lost: "lost",
};
const LIVE_STATUS = ["starting", "queued", "seeding", "running"];
const isToday = (at) => Boolean(at) && new Date(at).toDateString() === new Date().toDateString();

function fleetRow(a, roleName) {
  const { spec, state } = a;
  const phase = state.status === "running"
    ? a.paused_now ? "paused" : a.needs || state.herdr?.word === "blocked" ? "needs-you" : "working"
    : (FLEET_PHASE[state.status] ?? state.status);
  return {
    id: spec.id,
    title: spec.title?.trim() || "Untitled hire",
    roleName,
    identity: a.identity,
    state: phase,
    latest: state.error ?? state.herdr?.msg ?? "",
    activeMs: a.active_ms,
    live: LIVE_STATUS.includes(state.status),
    paused: a.paused_now,
    doneToday: state.status === "done" && isToday(state.ended),
  };
}

// `focusId`: the hire whose name had focus; a redraw focuses it again.
function fleet(rows, { onOpen, onReply, onOpenHerdr, onOpenRun, focusId = null }) {
  const opens = [];
  const stat = (n, label) => el("span", { className: "kit-fleet__stat", textContent: `${n} ${label}` });
  const needs = rows.filter((r) => r.state === "needs-you").length;
  const head = el(
    "header",
    { className: "kit-fleet__head" },
    el("h3", { className: "kit-fleet__heading", textContent: "Fleet" }),
    stat(rows.length, rows.length === 1 ? "hire" : "hires"),
    stat(rows.filter((r) => r.live).length, "live"),
    stat(needs, "need you"),
    stat(rows.filter((r) => r.doneToday).length, "done today"),
  );

  const card = (r, i) => {
    const [text, tone] = FLEET_STATES[r.state] ?? [r.state, "muted"];
    const open = el("button", {
      className: "kit-fleet__open",
      type: "button",
      tabIndex: i === 0 ? 0 : -1,
      ariaLabel: `${r.title}, ${text}`,
      textContent: r.title,
      dataset: { id: r.id },
      onClick: () => onOpen(r.id),
    });
    opens.push(open);
    const actions = [];
    if (r.state === "needs-you") {
      actions.push(iconButton("message-square", "Reply", { variant: "primary", size: "sm", onClick: () => onReply(r.id) }));
      actions.push(iconButton("external-link", "Open in Herdr", { size: "sm", onClick: () => onOpenHerdr(r.id) }));
    }
    if (r.state === "failed" || r.state === "lost") {
      actions.push(iconButton("eye", "Open run", { size: "sm", onClick: () => onOpenRun(r.id) }));
    }
    return el(
      "li",
      { className: "kit-fleet__slot" },
      el(
        "article",
        { className: "kit-fleet__card", tone, dataset: { state: r.state } },
        el(
          "header",
          { className: "kit-fleet__top" },
          el("span", { className: `kit-fleet__mark kit-id-${r.identity}`, textContent: (r.roleName || "?").charAt(0).toUpperCase(), ariaHidden: "true" }),
          el("div", { className: "kit-fleet__who" }, open, el("p", { className: "kit-fleet__role", textContent: r.roleName })),
          el("span", { className: "kit-fleet__pill", tone, textContent: text }),
        ),
        el("p", { className: "kit-fleet__latest", textContent: r.latest ?? "" }),
        el("footer", { className: "kit-fleet__foot" }, clock(r.activeMs, r.live && !r.paused), el("span", { className: "kit-fleet__actions" }, ...actions)),
      ),
    );
  };

  const list = el("ol", { className: "kit-fleet__column", ariaLabel: "Hires" }, ...rows.map(card));
  list.addEventListener("keydown", (e) => {
    const cur = opens.indexOf(document.activeElement);
    const next = fleetStep(opens.length, cur, e.key);
    if (cur < 0 || next == null) return;
    e.preventDefault();
    opens.forEach((o, i) => (o.tabIndex = i === next ? 0 : -1));
    opens[next].focus();
  });
  const kept = opens.find((o) => o.dataset.id === focusId);
  if (kept) {
    opens.forEach((o) => (o.tabIndex = o === kept ? 0 : -1));
    queueMicrotask(() => kept.focus());
  }
  return el("section", { className: "kit-fleet", ariaLabel: "Fleet" }, head, list);
}

// ---- shared blocks: a data-variant node, a definition list, a titled section, a table

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

// ---- run card: where a run is, its clock, its strip and its actions

const RUN_PHASE = {
  starting: "standby",
  queued: "standby",
  seeding: "standby",
  done: "complete",
  failed: "errored",
  lost: "errored",
  timed_out: "errored",
  killed: "stopped",
};
const PHASE_LABEL = {
  standby: ["Starting", "info"],
  active: ["Working", "info"],
  needs_you: ["Needs you", "warn"],
  held: ["Held", "muted"],
  complete: ["Complete", "ok"],
  errored: ["Errored", "bad"],
  stopped: ["Stopped", "muted"],
};
const PHASE_ACTIONS = {
  standby: [],
  active: ["pause", "done"],
  held: ["reply", "done"],
  needs_you: ["reply", "herdr"],
  complete: [],
  errored: ["screen"],
  stopped: ["screen"],
};
const ACTION_LABEL = {
  pause: ["Pause", "hand", "outline"],
  done: ["Mark done", "circle-check", "outline"],
  reply: ["Reply", "message-square", "primary"],
  herdr: ["Open in Herdr", "external-link", "primary"],
  screen: ["Open screen", "terminal", "outline"],
};

function runPhase(a) {
  const s = a.state.status;
  if (s !== "running") return RUN_PHASE[s] ?? "standby";
  if (a.paused_now) return "held";
  const w = a.state.herdr?.word;
  return (w === "idle" || w === "blocked") && (a.state.herdr?.turn ?? 0) >= 1 ? "needs_you" : "active";
}

// `view`: { phase, activeMs, live, paused, line, verdict }. `spans` come from the
// run's timeline; `verdict` is the summary line once cos has judged the run.
function runCard(view, spans, { onAction }) {
  const [label, tone] = PHASE_LABEL[view.phase];
  const strip = el(
    "div",
    { className: "kit-run__strip", ariaHidden: "true" },
    ...spans.map((s) => {
      const seg = el("span", { className: "kit-run__seg", dataset: { mode: s.mode } });
      seg.style.flexGrow = String(Math.max(s.ms, 1));
      return seg;
    }),
  );
  const actions = PHASE_ACTIONS[view.phase].map((k) => {
    const [text, glyph, variant] = ACTION_LABEL[k] ?? [k, "circle", "outline"];
    return iconButton(glyph, text, { variant, size: "sm", onClick: () => onAction(k) });
  });
  return el(
    "section",
    { className: "kit-run", ariaLabel: "Run" },
    el(
      "header",
      { className: "kit-run__top" },
      el("span", { className: "kit-run__phase", tone, textContent: label }),
      clock(view.activeMs, view.live && !view.paused),
    ),
    strip,
    el("p", { className: "kit-run__line", textContent: view.line ?? "" }),
    view.verdict ? el("p", { className: "kit-run__verdict", textContent: view.verdict }) : null,
    el("footer", { className: "kit-run__foot" }, ...actions),
  );
}

// ---- timeline: the hire and you on one time axis

const MODE_LABEL = { working: "Working", waiting: "Waiting", paused: "Paused" };
const TIME = { hour: "2-digit", minute: "2-digit", second: "2-digit" };
const clockAt = (s) => new Date(s).toLocaleTimeString([], TIME);

function timeline(tl, { live }) {
  const now = Date.now();
  const stamps = [
    ...tl.spans.flatMap((s) => [Date.parse(s.from), Date.parse(s.to)]),
    ...tl.handoffs.map((h) => Date.parse(h.at)),
    ...tl.marks.map((m) => Date.parse(m.at)),
  ].filter(Number.isFinite);
  const t0 = Math.min(now, ...stamps);
  const t1 = Math.max(t0 + 1, now, ...stamps);
  const place = (node, at) => {
    node.style.left = `${((at - t0) / (t1 - t0)) * 100}%`;
    return node;
  };

  const detail = el("p", { className: "kit-tl__detail", role: "status", textContent: "Select a span for its detail." });
  let spanBtns = [];
  const show = (s, btn) => {
    detail.textContent = `${MODE_LABEL[s.mode]}, starts ${clockAt(s.from)}, length ${spanWords(s.ms / 1000)}, word ${s.word ?? "none"}, turn ${s.turn ?? "none"}.`;
    spanBtns.forEach((b) => (b.tabIndex = b === btn ? 0 : -1));
  };
  const mk = (s) => {
    const b = el("button", {
      className: "kit-tl__span",
      type: "button",
      tabIndex: -1,
      ariaLabel: `${MODE_LABEL[s.mode]} from ${clockAt(s.from)}`,
      onClick: () => show(s, b),
    });
    b.dataset.mode = s.mode;
    place(b, Date.parse(s.from));
    b.style.width = `${((Date.parse(s.to) - Date.parse(s.from)) / (t1 - t0)) * 100}%`;
    spanBtns.push(b);
    return b;
  };
  const hire = tl.spans.filter((s) => s.mode !== "paused");
  const you = tl.spans.filter((s) => s.mode === "paused");
  const hireRow = el("div", { className: "kit-tl__lane", role: "group", ariaLabel: "The hire" }, ...hire.map(mk));
  const youRow = el("div", { className: "kit-tl__lane", role: "group", ariaLabel: "You" }, ...you.map(mk));
  for (const h of tl.handoffs) {
    youRow.append(place(el("span", { className: "kit-tl__tick", ariaHidden: "true" }), Date.parse(h.at)));
  }
  for (const m of tl.marks) hireRow.append(place(el("span", { className: "kit-tl__mark", ariaHidden: "true" }), Date.parse(m.at)));
  if (live) hireRow.append(place(el("span", { className: "kit-tl__playhead", ariaHidden: "true" }), now));

  const root = el(
    "section",
    { className: "kit-tl", ariaLabel: "Timeline" },
    el("div", { className: "kit-tl__row" }, el("span", { className: "kit-tl__name", textContent: "You" }), youRow),
    el("div", { className: "kit-tl__row" }, el("span", { className: "kit-tl__name", textContent: "Hire" }), hireRow),
    ...tl.notes.map((n) => el("p", { className: "kit-tl__note", textContent: n })),
    detail,
  );
  spanBtns = [...root.querySelectorAll(".kit-tl__span")];
  spanBtns.forEach((b, i) => (b.tabIndex = i === 0 ? 0 : -1));
  root.addEventListener("keydown", (e) => {
    const cur = spanBtns.indexOf(document.activeElement);
    const next = fleetStep(spanBtns.length, cur, e.key === "ArrowRight" ? "ArrowDown" : e.key === "ArrowLeft" ? "ArrowUp" : e.key);
    if (cur < 0 || next == null) return;
    e.preventDefault();
    spanBtns[next].focus();
    spanBtns[next].click();
  });
  return root;
}

// ---- decisions: designs waiting on you, as a stack. The top card is the next
// decision and up to two peeks sit behind it; a decided card moves to the history.

// `items`: each design waiting at the gate, or approved this session (`done`).
// `cb`: { onOpen(item, path), onAsk(key), onApprove(key), onCancel(key), focusNext }.
// The page holds the decision state (`confirming`, `busy`, `error`); this draws it.
function decisions(items, cb) {
  const cards = items.map((g) => decisionCard(g, cb));
  let order = cards.filter((c) => !c.dataset.state);
  const decided = cards.filter((c) => c.dataset.state);
  const stack = el("div", { className: "kit-stack", role: "group", ariaLabel: "Designs waiting for your approval" });
  const place = () => {
    order.forEach((c, i) => {
      c.classList.add("kit-stack__card");
      c.dataset.position = String(i);
      c.hidden = i > 2;
    });
    stack.replaceChildren(...order);
  };
  const bring = (card) => {
    order = [card, ...order.filter((c) => c !== card)];
    place();
    order[0].focus();
  };
  for (const c of order) {
    c.tabIndex = -1;
    c.addEventListener("click", () => {
      if (c.dataset.position !== "0") bring(c);
    });
  }
  stack.addEventListener("keydown", (e) => {
    const forward = { ArrowDown: true, ArrowRight: true, ArrowUp: false, ArrowLeft: false }[e.key];
    if (forward === undefined || order.length < 2) return;
    e.preventDefault();
    bring(forward ? order[1] : order[order.length - 1]);
  });
  place();
  if (cb.focusNext) queueMicrotask(() => order[0]?.querySelector(".gate-actions button")?.focus());
  return el(
    "div",
    { className: "kit-decisions" },
    stack,
    decided.length ? el("section", { className: "kit-decisions__history", ariaLabel: "Approved this session" }, ...decided) : null,
  );
}

function decisionCard(g, cb) {
  const errBox = el("p", { className: "error small", role: "status", textContent: g.error ?? "" });
  const open = (path) => {
    errBox.textContent = "";
    Promise.resolve()
      .then(() => cb.onOpen(g, path))
      .catch((e) => {
        errBox.textContent = `Could not open ${fileName(path)}: ${e}`;
      });
  };
  const link = (label, path) => {
    const b = sm(el("button", { className: "btn", type: "button" }, icon("file-text"), el("span", { textContent: label })), "link");
    b.addEventListener("click", () => open(path));
    return b;
  };
  const r = g.review;
  const files = el(
    "div",
    { className: "gate-files" },
    g.brief ? link("Brief", g.brief) : null,
    link("Design", g.design),
    r ? link("Latest review", r.path) : null,
    el("span", { className: "sha", textContent: `sha256 ${sha12(g.design_sha256)}`, title: g.design_sha256 ?? g.design_error ?? "" }),
  );
  const verdict = r
    ? el(
        "p",
        { className: "gate-verdict" },
        badge(r.verdict, r.verdict === "clean" && r.current ? "ok" : "warn"),
        ` Latest review: ${counts(r)}${r.current ? "" : " · it read an earlier version of the design"}`,
      )
    : el("p", { className: "gate-verdict muted", textContent: "No review of this design yet." });
  const history = g.history?.length
    ? el(
        "details",
        { className: "small" },
        el("summary", { textContent: `Earlier reviews (${g.history.length})` }),
        el(
          "ul",
          { className: "rounds" },
          ...g.history.map((h) =>
            el("li", {}, sm(el("button", { className: "btn", type: "button", textContent: fileName(h.path), onClick: () => open(h.path) }), "link"), badge(h.verdict, "muted"), el("span", { className: "muted", textContent: counts(h) })),
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
    gateSteps(g),
    files,
    verdict,
    g.done ? el("p", { className: "gate-verdict" }, badge("approved", "ok"), ` ${g.done.check}`) : decisionActions(g, cb, open),
    errBox,
    history,
  );
  card.dataset.key = g.key;
  if (g.done) {
    card.dataset.state = "approved";
    card.dataset.status = g.clean ? "approved" : "approved-without-review";
  }
  return card;
}

function gateSteps(g) {
  const names = g.names ?? [];
  const flow = g.flow ?? { step: "spec", beads: [] };
  const beads = flow.beads ?? [];
  if (g.done) return steps(names, "plan", { state: "in_progress", beads });
  if (flow.step === "drift") return steps(names, "approve", { drift: "the design changed after approval", beads });
  return steps(names, "approve", { state: "awaiting_approval", beads });
}

function decisionActions(g, cb, open) {
  if (g.design_error) return el("p", { className: "error small", textContent: g.design_error });
  if (!g.by) return el("p", { className: "muted small", textContent: `No approver name: set "owner" in the workspace profile, or git user.name in ${g.repo}.` });
  if (g.confirming) return decisionConfirm(g, cb);
  const ask = (label, variant) => {
    const b = sm(el("button", { className: "btn", type: "button" }, icon("badge-check"), el("span", { textContent: label })), variant);
    b.addEventListener("click", () => cb.onAsk(g.key));
    return b;
  };
  const r = g.review;
  if (r && r.current && r.blocking > 0) {
    const n = r.blocking;
    const read = sm(el("button", { className: "btn", type: "button", onClick: () => open(r.path) }, icon("file-search"), el("span", { textContent: `Read ${n} blocking finding${n === 1 ? "" : "s"}` })), "primary");
    return el("div", { className: "gate-actions" }, read, ask("Approve anyway", "outline"));
  }
  if (g.clean) return el("div", { className: "gate-actions" }, ask("Approve and start Plan", "primary"));
  if (r && r.current) {
    const carried = [r.should && `${r.should} should`, r.nit && `${r.nit} nit`].filter(Boolean).join(", ");
    return el("div", { className: "gate-actions" }, ask(`Approve: ${carried} go to Plan`, "primary"));
  }
  return el("div", { className: "gate-actions" }, ask("Approve without a current review", "outline"));
}

function decisionConfirm(g, cb) {
  const busy = Boolean(g.busy);
  const yes = sm(el("button", { className: "btn", type: "button", disabled: busy }, icon(busy ? "loader-circle" : "badge-check", { className: busy ? "spin" : "" }), el("span", { textContent: busy ? "Approving…" : "Approve and start Plan" })), "primary");
  const no = sm(el("button", { className: "btn", type: "button", disabled: busy }, icon("x"), el("span", { textContent: "Cancel" })), "ghost");
  no.addEventListener("click", () => cb.onCancel(g.key));
  yes.addEventListener("click", () => cb.onApprove(g.key));
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
}

const sha12 = (sha) => (sha ? sha.slice(0, 12) : "—");
const fileName = (p) => p.split("/").pop();
const counts = (r) => `${r.blocking} blocking, ${r.should} should, ${r.nit} nit`;
const badge = (text, tone) => el("span", { className: "badge", textContent: text, tone });

// ---- steps: a ritual's steps as a progress list, with no times

const STEP_NAME = { think: "Think", spec: "Spec", approve: "Approve", plan: "Plan", build: "Build", review: "Review", ship: "Ship", reflect: "Reflect" };
const STEP_STATE = { finished: "finished", in_progress: "in progress", awaiting_approval: "awaiting your approval", errored: "drifted", queued: "queued" };

// `names`: the ritual's steps in order. `current` is where the flow stands, in
// `state`; `drift` replaces that step's state with the reason it drifted.
function steps(names, current, { state = "in_progress", drift = null, beads = [] } = {}) {
  const at = names.indexOf(current);
  return el(
    "ol",
    { className: "kit-steps", ariaLabel: "Ritual steps" },
    ...names.map((s, i) => {
      const st = i < at ? "finished" : i > at ? "queued" : drift ? "errored" : state;
      const li = el(
        "li",
        { className: "kit-steps__step", dataset: { state: st } },
        el("span", { className: "kit-steps__name", textContent: STEP_NAME[s] ?? s }),
        el("span", { className: "kit-steps__state", textContent: i === at && drift ? drift : STEP_STATE[st] }),
        s === "build" && beads.length ? beadList(beads) : null,
      );
      if (i === at) li.setAttribute("aria-current", "step");
      return li;
    }),
  );
}

const BEAD_STATE = { done: "finished", in_progress: "in progress" };
function beadList(beads) {
  const shown = beads.length > 6 ? beads.slice(0, 5) : beads;
  const more = beads.length - shown.length;
  return el(
    "ol",
    { className: "kit-steps__beads", ariaLabel: "Beads" },
    ...shown.map((b) => {
      const st = BEAD_STATE[b.status] ?? "queued";
      return el(
        "li",
        { dataset: { beadState: st } },
        el("span", { className: "kit-steps__bead", textContent: b.title?.trim() || "Untitled bead" }),
        el("span", { className: "kit-steps__state", textContent: st }),
      );
    }),
    more ? el("li", { className: "kit-steps__more", textContent: `${more} more beads` }) : null,
  );
}

// The run card above its timeline. `read()` resolves the run's Timeline; a read
// that fails shows an unreachable state whose Retry reads again.
function runPanel(view, read, { onAction }) {
  const box = el("div", { className: "kit-run-panel" });
  const shown = (tail, spans = []) => box.replaceChildren(runCard(view, spans, { onAction }), tail);
  const load = async () => {
    shown(stateView("reading", { title: "Reading the run's timeline…", line: true }));
    try {
      const tl = await read();
      box.replaceChildren(runCard(view, tl.spans, { onAction }), timeline(tl, { live: view.live && !view.paused }));
    } catch (e) {
      logDev("agents_timeline", e);
      shown(stateView("unreachable", { title: "The timeline could not be read", detail: "Its event log is unreadable or the business changed. Retry reads it again.", line: true, action: { icon: "rotate-ccw", label: "Retry", onClick: load } }));
    }
  };
  load();
  return box;
}
