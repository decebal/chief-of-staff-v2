// The business picker. Names and homes are set with textContent.
(() => {
  const { invoke } = window.__TAURI__.core;
  const dialog = document.getElementById("switch-dialog");
  const filter = document.getElementById("switch-filter");
  const list = document.getElementById("switch-list");
  const msg = document.getElementById("switch-state");

  let rows = [];
  let shown = [];
  let at = 0;
  let pick = null;
  let returnTo = null;
  let read = false;

  // One state under the list: reading, nothing found, or why a pick failed.
  const say = (view) => {
    msg.replaceChildren(...(view ? [view] : []));
    msg.hidden = !view;
  };

  const matches = (q) => {
    const byName = rows.filter((r) => r.name.toLowerCase().includes(q));
    const byHome = rows.filter((r) => !byName.includes(r) && String(r.triple?.home ?? "").toLowerCase().includes(q));
    return [...byName, ...byHome];
  };

  function row(r, i) {
    const li = document.createElement("li");
    li.id = `switch-row-${i}`;
    li.setAttribute("role", "option");
    li.setAttribute("aria-selected", String(i === at));
    const top = document.createElement("span");
    top.className = "top";
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = r.name;
    top.append(name);
    const tag = (text, tone) => {
      const mark = document.createElement("span");
      mark.className = "badge";
      mark.dataset.tone = tone;
      mark.textContent = text;
      top.append(mark);
    };
    if (r.app) tag("This window", "muted");
    if (r.active) tag("MCP default", "info");
    const home = document.createElement("span");
    home.className = "home";
    const full = String(r.triple?.home ?? "");
    home.title = full;
    // LRM marks keep `~/` and `/` at the start under the right-to-left ellipsis.
    home.textContent = `‎${full.replace(/^\/Users\/[^/]+(?=\/|$)/, "~")}‎`;
    li.append(top, home);
    if (r.missing) {
      const gone = document.createElement("span");
      gone.className = "muted small";
      gone.textContent = "home is missing";
      li.append(gone);
    }
    const counts = [];
    if (r.live) counts.push(`${r.live} live`);
    if (r.lost) counts.push(`${r.lost} lost`);
    if (r.unread) counts.push("unread brief");
    if (r.scheduled) counts.push("schedule installed");
    if (counts.length) {
      const count = document.createElement("span");
      count.className = "muted small";
      count.textContent = counts.join(" · ");
      li.append(count);
    }
    li.addEventListener("click", () => choose(i));
    return li;
  }

  function render() {
    shown = matches(filter.value.trim().toLowerCase());
    at = Math.min(at, Math.max(shown.length - 1, 0));
    list.replaceChildren(...shown.map(row));
    list.setAttribute("aria-activedescendant", shown.length ? `switch-row-${at}` : "");
    footer();
    if (!read) return;
    say(
      shown.length
        ? null
        : stateView("empty", {
            title: rows.length ? "No business matches that name or folder." : "No business is registered yet.",
            detail: rows.length ? "" : "cos business add <name> <home> <workspace> <projects> registers one.",
            line: true,
          }),
    );
  }

  function highlight(to) {
    if (!shown.length) return;
    at = (to + shown.length) % shown.length;
    for (const [i, li] of [...list.children].entries()) li.setAttribute("aria-selected", String(i === at));
    list.setAttribute("aria-activedescendant", `switch-row-${at}`);
    list.children[at]?.scrollIntoView({ block: "nearest" });
    footer();
  }

  // The MCP default is the registry's active business; a window switch never moves it.
  const now = document.getElementById("mcp-default-now");
  const make = document.getElementById("mcp-make-default");
  const makeLabel = document.getElementById("mcp-make-default-label");

  function footer() {
    const def = rows.find((r) => r.active);
    now.textContent = !read
      ? ""
      : def
        ? `Claude Desktop and Codex use ${def.name} when a call names no business.`
        : "Claude Desktop and Codex have no default business.";
    const r = shown[at];
    make.hidden = !read || !r || r.active;
    make.disabled = !r || r.active;
    makeLabel.textContent = r ? `Make ${r.name} the default` : "Make default";
  }

  async function makeDefault() {
    const r = shown[at];
    if (!r || r.active) return;
    make.disabled = true;
    say(null);
    try {
      await invoke("business_make_default", { name: r.name });
      rows = await invoke("businesses");
      render();
    } catch (e) {
      say(
        stateView("unreachable", {
          title: `${r.name} is not the default`,
          detail: e instanceof Error ? e.message : String(e),
          line: true,
          action: { icon: "rotate-ccw", label: "Try again", onClick: makeDefault },
        }),
      );
      footer();
    }
  }
  make.addEventListener("click", makeDefault);

  async function choose(i) {
    const r = shown[i];
    if (!r) return;
    say(null);
    try {
      await pick(r.name);
      dialog.close();
    } catch (e) {
      say(stateView("unreachable", { title: e instanceof Error ? e.message : String(e), line: true }));
    }
  }

  filter.addEventListener("input", () => {
    at = 0;
    render();
  });

  filter.addEventListener("keydown", (ev) => {
    if (ev.key === "ArrowDown") highlight(at + 1);
    else if (ev.key === "ArrowUp") highlight(at - 1);
    else if (ev.key === "Enter") {
      ev.preventDefault();
      choose(at);
    }
  });

  dialog.addEventListener("close", () => {
    returnTo?.focus?.();
    returnTo = null;
  });

  async function loadRows() {
    say(stateView("reading", { title: "Reading the businesses…", line: true }));
    try {
      rows = await invoke("businesses");
      read = true;
      // Open on this window's business, so Enter and Make default start from where the window is.
      at = Math.max(matches("").findIndex((r) => r.app), 0);
      render();
    } catch (e) {
      say(
        stateView("unreachable", {
          title: "Could not read the businesses",
          detail: String(e),
          line: true,
          action: { icon: "rotate-ccw", label: "Retry", onClick: loadRows },
        }),
      );
    }
  }

  // The picker button's dot: another business has an unread brief or lost hires.
  // A pushed payload wins over a first read that may predate it.
  const attention = document.getElementById("biz-attention");
  let pushed = false;
  const mark = (all) => {
    attention.hidden = !all.some((r) => !r.app && (r.unread || r.lost));
  };
  window.__TAURI__.event
    .listen("businesses-changed", (ev) => {
      pushed = true;
      mark(ev.payload);
    })
    .then(() => invoke("businesses"))
    .then((first) => pushed || mark(first))
    .catch((e) => console.debug("[cos] business attention", e));

  // A rejected pick keeps the picker open with the reason shown.
  window.openSwitcher = (onPick) => {
    if (dialog.open) return;
    pick = onPick;
    returnTo = document.activeElement;
    filter.value = "";
    at = 0;
    read = false;
    say(null);
    list.replaceChildren();
    footer();
    dialog.showModal();
    filter.focus();
    loadRows();
  };
})();
