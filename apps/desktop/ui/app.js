// Every value from the backend reaches the DOM through textContent or a
// property, never innerHTML: titles and activity lines quote agent output.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const REPORT = navigator.userAgent.includes("Windows")
  ? "http://cosreport.localhost/"
  : "cosreport://localhost/";

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...kids) => {
  const n = Object.assign(document.createElement(tag), props);
  n.append(...kids);
  return n;
};

const EMPTY = $("no-agents").textContent;
const ui = { selected: null, tailFrom: 0, agents: [], inflight: false, again: false };

const logDev = (where, e) => console.debug(`[cos] ${where}`, e);

async function loadPaths() {
  const p = await invoke("paths");
  $("ws-path").textContent = p.workspace;
  $("ws-path").title = `home ${p.home}\nprojects ${p.projects}\ncos ${p.cos ?? "not found"}`;
  ui.slots = p.slots;
  ui.cos = p.cos;
  renderSlots();
}

function renderSlots() {
  const running = ui.agents.filter((a) => ["running", "seeding"].includes(a.state.status)).length;
  const queued = ui.agents.filter((a) => a.state.status === "queued").length;
  $("slots").textContent = `${running}/${ui.slots} running${queued ? ` · ${queued} queued` : ""}`;
  const live = running + queued;
  $("agent-count").textContent = live ? `(${live})` : "";
}

$("ws-path").addEventListener("click", () => {
  $("ws-input").value = $("ws-path").textContent;
  $("ws-error").hidden = true;
  $("ws-dialog").showModal();
});

$("ws-save").addEventListener("click", async (ev) => {
  ev.preventDefault();
  try {
    await invoke("set_workspace", { path: $("ws-input").value });
    $("ws-dialog").close();
    await loadPaths();
    reloadReport();
  } catch (e) {
    $("ws-error").textContent = String(e);
    $("ws-error").hidden = false;
  }
});

for (const tab of document.querySelectorAll(".tab")) {
  tab.addEventListener("click", () => {
    for (const t of document.querySelectorAll(".tab")) t.classList.toggle("active", t === tab);
    for (const v of document.querySelectorAll(".view")) {
      v.classList.toggle("active", v.id === `view-${tab.dataset.view}`);
    }
  });
}

function reloadReport() {
  $("report").src = `${REPORT}?t=${Date.now()}`;
}

async function loadRepos() {
  const select = $("repo");
  for (const r of await invoke("repos")) {
    select.append(el("option", { value: r, textContent: r.split("/").pop() }));
  }
}

async function refreshAgents() {
  if (ui.inflight) {
    ui.again = true;
    return;
  }
  ui.inflight = true;
  try {
    ui.agents = await invoke("agents_list");
    renderAgents();
    renderSlots();
    if (ui.selected) await renderDetail();
  } catch (e) {
    logDev("agents_list", e);
    const retry = el("button", { textContent: "Retry", type: "button" });
    retry.addEventListener("click", refreshAgents);
    $("no-agents").replaceChildren(`Could not read the agents in out/agents: ${e} `, retry);
    $("no-agents").hidden = false;
  } finally {
    ui.inflight = false;
    if (ui.again) {
      ui.again = false;
      refreshAgents();
    }
  }
}

const money = (st) => (st.cost_usd == null ? "" : `$${st.cost_usd.toFixed(2)}`);

function renderAgents() {
  const body = $("agents").tBodies[0];
  body.replaceChildren(
    ...ui.agents.map(({ spec, state }) => {
      const tr = el(
        "tr",
        { className: spec.id === ui.selected ? "selected" : "" },
        el("td", { className: "id", textContent: spec.id }),
        el("td", { textContent: spec.harness }),
        el("td", {}, el("span", { className: `status ${state.status}`, textContent: state.status })),
        el("td", { textContent: spec.title }),
        el("td", { textContent: String(state.tool_calls) }),
        el("td", { textContent: money(state) }),
        el("td", { className: "latest", textContent: state.error || state.last, title: state.error || state.last }),
      );
      tr.addEventListener("click", () => select(spec.id));
      return tr;
    }),
  );
  if (!ui.agents.length) $("no-agents").textContent = EMPTY;
  $("no-agents").hidden = ui.agents.length > 0;
}

function select(id) {
  ui.selected = id;
  ui.tailFrom = 0;
  $("d-log").textContent = "";
  $("detail").hidden = false;
  renderAgents();
  renderDetail();
}

async function renderDetail() {
  const row = ui.agents.find((a) => a.spec.id === ui.selected);
  if (!row) {
    $("detail").hidden = true;
    ui.selected = null;
    return;
  }
  const { spec, state } = row;
  $("d-title").textContent = spec.title || spec.id;
  const meta = [
    ["status", state.status],
    ["workdir", spec.workdir],
    ["repo", spec.repo ?? "none"],
    ["source", spec.source],
    ["seeded", (state.seeded ?? []).join(", ") || "nothing"],
    ["build slot", (state.shimmed ?? []).join(", ") || "no build tools shimmed"],
    ["session", state.session ?? ""],
    ["started", state.started ?? "waiting"],
    ["ended", state.ended ?? ""],
    ["resume", state.session ? `cd ${spec.workdir} && ${spec.harness === "codex" ? "codex resume" : "claude --resume"} ${state.session}` : ""],
  ];
  $("d-meta").replaceChildren(
    ...meta.filter(([, v]) => v).flatMap(([k, v]) => [el("dt", { textContent: k }), el("dd", { textContent: v })]),
  );
  $("d-error").textContent = state.error ?? "";
  $("d-error").hidden = !state.error;
  const terminal = !["starting", "queued", "seeding", "running"].includes(state.status);
  $("d-stop").disabled = terminal;
  $("d-clean").disabled = !terminal;

  try {
    const t = await invoke("agents_tail", { id: spec.id, from: ui.tailFrom });
    if (t.lines.length) {
      $("d-log").textContent += `${t.lines.join("\n")}\n`;
      $("d-log").scrollTop = $("d-log").scrollHeight;
    }
    ui.tailFrom = t.next;
    $("d-seed").hidden = !t.seedLog;
    $("d-seed-log").textContent = t.seedLog ?? "";
  } catch (e) {
    logDev("agents_tail", e);
  }
}

$("d-close").addEventListener("click", () => {
  ui.selected = null;
  $("detail").hidden = true;
  renderAgents();
});

$("d-stop").addEventListener("click", async () => {
  $("d-stop").disabled = true;
  try {
    await invoke("agents_stop", { id: ui.selected });
  } catch (e) {
    $("d-error").textContent = String(e);
    $("d-error").hidden = false;
  }
  refreshAgents();
});

$("d-clean").addEventListener("click", async () => {
  const id = ui.selected;
  try {
    await invoke("agents_clean", { id, force: false });
  } catch (e) {
    const force = confirm(`${e}\n\nRemove it anyway? The worktree's work exists nowhere else.`);
    if (!force) return;
    await invoke("agents_clean", { id, force: true }).catch((e2) => alert(String(e2)));
  }
  ui.selected = null;
  $("detail").hidden = true;
  refreshAgents();
});

async function spawn(prompt, title, source) {
  const msg = $("spawn-msg");
  if (!ui.cos) {
    msg.textContent = "No cos binary found: build it (cargo build) or set COS_BIN.";
    return;
  }
  msg.textContent = "starting…";
  try {
    const specs = await invoke("agents_spawn", {
      args: {
        harness: $("harness").value,
        repo: $("repo").value || null,
        prompt,
        title: title || null,
        model: $("model").value || null,
        worktree: $("worktree").checked,
        deadlineSecs: Number($("deadline").value) || 1800,
        budgetUsd: Number($("budget").value) || 5,
        count: Number($("count").value) || 1,
        source: source || null,
      },
    });
    msg.textContent = `started ${specs.map((s) => s.id).join(", ")}`;
    document.querySelector('.tab[data-view="agents"]').click();
    refreshAgents();
  } catch (e) {
    msg.textContent = String(e);
  }
}

$("spawn-form").addEventListener("submit", (ev) => {
  ev.preventDefault();
  spawn($("prompt").value, $("title").value, null);
});

for (const b of document.querySelectorAll("button.from")) {
  b.addEventListener("click", async () => {
    const list = $("tasks");
    try {
      const tasks = await invoke("agent_tasks", { from: b.dataset.from });
      list.replaceChildren(
        ...tasks.map((t) => {
          const go = el("button", { textContent: "spawn", type: "button" });
          go.addEventListener("click", () => spawn(t.prompt, t.title, t.source));
          const use = el("button", { textContent: "edit", type: "button" });
          use.addEventListener("click", () => {
            $("prompt").value = t.prompt;
            $("title").value = t.title;
          });
          return el("li", {}, t.title, go, use);
        }),
      );
      if (!tasks.length) list.replaceChildren(el("li", { className: "muted", textContent: "no agent prompts in it" }));
    } catch (e) {
      list.replaceChildren(el("li", { className: "error", textContent: String(e) }));
    }
  });
}

listen("report-changed", reloadReport);
listen("agents-changed", refreshAgents);
loadPaths().then(loadRepos);
reloadReport();
refreshAgents();
