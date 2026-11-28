// Settings: the Assistants card. Each client green or red with the failing
// field named, Register / Repair / Remove through connect_apply, and the
// version each configured cos and the daemon run against this app's.
(() => {
  const { invoke } = window.__TAURI__.core;
  const dialog = $("settings-dialog");
  const body = $("assistants-body");
  const note = $("assistants-note");
  let returnTo = null;
  let busy = false;
  // Claude Desktop reads its config only at launch; the note stays until the dialog closes.
  let restart = false;

  const PROBLEM = {
    config: "its config does not parse",
    registered: "no cos entry in its config",
    command: "the configured cos is missing or not executable",
    version: "the configured cos did not print this app's version",
    timeout: "tool_timeout_sec is under 320",
  };

  function headline(c) {
    if (c.ok) return `Registered · ${c.version ?? "cos"}`;
    if (!c.installed && !c.registered) return "Not installed";
    const named = c.problems.filter((p) => !(p === "version" && c.skew));
    return named.length ? named.map((p) => PROBLEM[p] ?? p).join("; ") : "Another version of cos";
  }

  function clientRow(c, cos) {
    const tone = c.ok ? "ok" : !c.installed && !c.registered ? "muted" : "bad";
    const mark = icon(c.ok ? "circle-check" : tone === "muted" ? "circle-dashed" : "circle-x", {
      label: c.ok ? "working" : tone === "muted" ? "not installed" : "needs repair",
    });
    const actions = el("div", { className: "assist-actions" });
    const act = (name, label, action, variant = "outline") =>
      actions.append(
        iconButton(name, label, { variant, disabled: busy || (action !== "remove" && !cos), onClick: () => apply(c, action) }),
      );
    if (!c.registered && c.problems.includes("registered")) act("plus", "Register", "register", "primary");
    if (c.registered && !c.ok) act("rotate-ccw", "Repair", "repair", "primary");
    if (c.registered) act("trash-2", "Remove", "remove");
    const where = el(
      "dl",
      { className: "kv" },
      el("dt", { textContent: "config" }),
      el("dd", { className: "mono", textContent: c.config }),
      c.command ? el("dt", { textContent: "command" }) : null,
      c.command ? el("dd", { className: "mono", textContent: c.command }) : null,
      c.timeout_sec != null ? el("dt", { textContent: "timeout" }) : null,
      c.timeout_sec != null ? el("dd", { textContent: `${c.timeout_sec} s` }) : null,
    );
    return el(
      "li",
      { className: "assist-row", tone },
      el(
        "div",
        { className: "assist-top" },
        mark,
        el("span", { className: "assist-name", textContent: c.label }),
        el("span", { className: "assist-line", textContent: headline(c) }),
        actions,
      ),
      c.skew ? stateView("needs_you", { title: c.skew, line: true }) : null,
      c.error && !c.ok ? stateView("unreachable", { title: c.error, line: true }) : null,
      el("details", { className: "assist-where" }, el("summary", { textContent: c.command ? "Config and command" : "Config" }), where),
    );
  }

  function daemonRow(d, appVersion) {
    const up = d.state === "up";
    const tone = up ? (d.skew ? "warn" : "ok") : d.state === "down" ? "muted" : "bad";
    const line = up
      ? `Running cos ${d.version}`
      : d.state === "down"
        ? "Not running: each client's cos mcp starts paid runs itself"
        : "Daemon socket unreadable";
    return el(
      "li",
      { className: "assist-row", tone },
      el(
        "div",
        { className: "assist-top" },
        icon(up ? "server" : d.state === "down" ? "moon" : "circle-x", { label: d.state }),
        el("span", { className: "assist-name", textContent: "cos daemon" }),
        el("span", { className: "assist-line", textContent: line }),
      ),
      d.skew ? stateView("needs_you", { title: d.skew, line: true }) : null,
      d.error ? stateView("unreachable", { title: d.error, line: true }) : null,
      el("p", { className: "muted small", textContent: `This app is cos ${appVersion}.` }),
    );
  }

  function render(s) {
    const rows = s.clients.map((c) => clientRow(c, s.cos));
    const list = el("ul", { className: "assist-list" }, ...rows, daemonRow(s.daemon, s.app_version));
    const allOk = s.clients.every((c) => c.ok) && !s.daemon.skew;
    fill(
      body,
      s.cos
        ? null
        : stateView("unreachable", {
            title: "No cos binary to register: set COS_BIN, build it (cargo build), or task install.",
            line: true,
          }),
      list,
      allOk ? stateView("clear", { title: "Claude Desktop and Codex both reach this cos.", line: true }) : null,
    );
  }

  function showRestart() {
    fill(note, restart ? stateView("needs_you", { title: "Restart Claude Desktop to load it.", line: true }) : null);
  }

  async function load() {
    clearTimeout(load.slow);
    fill(body, stateView("reading", { title: "Checking Claude Desktop and Codex…", detail: "Reading each config and asking its cos for a version." }));
    load.slow = setTimeout(() => {
      fill(
        body,
        stateView("slow", {
          title: "Still checking the assistants",
          detail: "A configured cos has 5 s to print its version.",
          action: { icon: "rotate-ccw", label: "Try again", onClick: load },
        }),
      );
    }, SLOW_AFTER_MS);
    try {
      const s = await invoke("connect_status");
      clearTimeout(load.slow);
      render(s);
    } catch (e) {
      clearTimeout(load.slow);
      fill(
        body,
        stateView("unreachable", {
          title: "Could not read the assistants' configs",
          detail: e instanceof Error ? e.message : String(e),
          action: { icon: "rotate-ccw", label: "Try again", onClick: load },
        }),
      );
    }
  }

  async function apply(c, action) {
    if (busy) return;
    busy = true;
    fill(note, stateView("refreshing", { title: `${c.label}: ${action}…`, line: true }));
    try {
      const r = await invoke("connect_apply", { client: c.client, action });
      restart = restart || r.restart;
      busy = false;
      render(r.status);
      showRestart();
    } catch (e) {
      busy = false;
      fill(
        note,
        stateView("unreachable", {
          title: `${c.label}: ${action} failed: ${e instanceof Error ? e.message : String(e)}`,
          line: true,
          action: { icon: "rotate-ccw", label: "Try again", onClick: () => apply(c, action) },
        }),
      );
      load();
    }
  }

  $("assistants-refresh").addEventListener("click", () => {
    if (!busy) load();
  });
  $("settings-close").addEventListener("click", () => dialog.close());
  dialog.addEventListener("close", () => {
    restart = false;
    fill(note);
    returnTo?.focus?.();
    returnTo = null;
  });

  window.openSettings = () => {
    if (dialog.open) return;
    returnTo = document.activeElement;
    showRestart();
    dialog.showModal();
    load();
  };
  $("settings-open").addEventListener("click", () => window.openSettings());
})();
