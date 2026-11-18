// The business sidebar. Names reach the DOM through textContent.
(() => {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  const list = document.getElementById("sidebar-rows");
  const status = document.getElementById("sidebar-status");

  let rows = [];
  let pinned = null;
  let pushed = false;
  let read = false;

  const span = (className, text) => Object.assign(document.createElement("span"), { className, textContent: text });

  const glyph = (className, shown, said, title) => {
    const g = span(className, shown);
    g.setAttribute("aria-hidden", "true");
    if (title) g.title = title;
    return [g, span("sr-only", said)];
  };

  function row(r) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "biz-row";
    if (r.app) b.setAttribute("aria-current", "true");
    b.append(span("name", r.name));
    if (r.missing) {
      b.append(span("biz-missing", "home is missing"));
    } else {
      if (r.unread) b.append(...glyph("biz-dot", "", "unread brief"));
      if (r.lost) b.append(...glyph("biz-lost", `⚠ ${r.lost}`, `${r.lost} lost`));
      if (r.live) b.append(...glyph("count", String(r.live), `${r.live} live`));
    }
    if (r.scheduled) {
      const installed = `installed for ${r.name}`;
      b.append(...glyph("biz-clock", "◷", installed, installed));
    }
    if (pinned) {
      b.disabled = true;
      b.title = `pinned by ${pinned}`;
    }
    b.addEventListener("click", () => choose(r.name));
    const li = document.createElement("li");
    li.append(b);
    return li;
  }

  function render() {
    list.replaceChildren(...rows.map(row));
    if (!rows.length && (pushed || read)) say(stateView("empty", { title: "No business is registered yet.", line: true }));
    else if (status.firstElementChild?.dataset.kind === "empty") say(null);
  }

  const say = (view) => {
    status.replaceChildren(...(view ? [view] : []));
    status.hidden = !view;
  };

  async function choose(name) {
    say(null);
    try {
      await window.switchTo(name);
    } catch (e) {
      say(stateView("unreachable", { title: e instanceof Error ? e.message : String(e), line: true }));
    }
  }

  // The listener is registered before the first `businesses` call, and a result
  // computed before an applied payload is dropped: it can predate a cleared dot.
  async function start() {
    await listen("businesses-changed", (ev) => {
      pushed = true;
      rows = ev.payload;
      render();
    });
    invoke("paths")
      .then((p) => {
        pinned = p.pinned ?? null;
        render();
      })
      .catch((e) => console.debug("[cos] sidebar paths", e));
    await readRows();
  }

  async function readRows() {
    if (!pushed && !rows.length) say(stateView("reading", { title: "Reading the businesses…", line: true }));
    try {
      const first = await invoke("businesses");
      read = true;
      say(null);
      if (pushed) return;
      rows = first;
      render();
    } catch (e) {
      say(
        stateView("unreachable", {
          title: "Could not read the businesses",
          detail: String(e),
          line: true,
          action: { icon: "rotate-ccw", label: "Retry", onClick: readRows },
        }),
      );
    }
  }

  start().catch((e) => console.debug("[cos] sidebar", e));
})();
