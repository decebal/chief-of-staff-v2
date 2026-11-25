// Dashboard tabs and pane. Names and server text reach the DOM through textContent.
// `ui` and `openTab` belong to app.js and are read only from calls made after startup.
(() => {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  const tabsEl = document.getElementById("dash-tabs");
  const notice = document.getElementById("dash-notice");
  const state = document.getElementById("dash-state");
  const frame = document.getElementById("dash-frame");

  let tabs = [];
  let loaded = false;

  const node = (tag, className, text) => Object.assign(document.createElement(tag), { className, textContent: text ?? "" });

  // The dashboard tab the dash view shows, or null when another view is open.
  function selected() {
    if (!document.getElementById("view-dash").classList.contains("active")) return null;
    return tabsEl.querySelector('.app-tab[aria-current="page"]')?.dataset.dash ?? null;
  }

  function idle(name) {
    state.replaceChildren(
      stateView("empty", {
        title: name ? `${name} is not open` : "No dashboard picked",
        detail: name ? "Click its tab to open it." : "Pick a dashboard tab to open it.",
      }),
    );
    state.hidden = false;
  }

  function hideDash() {
    frame.hidden = true;
    frame.removeAttribute("src");
    idle(selected());
  }

  function renderTabs() {
    const current = selected();
    tabsEl.replaceChildren(
      ...tabs.map((t) => {
        const b = node("button", "app-tab", t.name);
        b.prepend(icon("activity"));
        b.type = "button";
        b.dataset.view = "dash";
        b.dataset.dash = t.name;
        if (t.name === current) b.setAttribute("aria-current", "page");
        b.addEventListener("click", () => {
          openTab("dash", "", t.name);
          hideDash();
        });
        return b;
      }),
    );
  }

  async function loadDashTabs() {
    try {
      const rows = await invoke("dashboards", { business: ui.business });
      tabs = rows.map(({ name, url, managed }) => ({ name, url, managed }));
      notice.replaceChildren();
      notice.hidden = true;
    } catch (e) {
      tabs = [];
      notice.replaceChildren(
        stateView("unreachable", {
          title: "Dashboards not read",
          detail: String(e),
          line: true,
          action: { icon: "rotate-ccw", label: "Retry", onClick: loadDashTabs },
        }),
      );
      notice.hidden = false;
    }
    renderTabs();
    loaded = true;
  }

  // Before the first loadDashTabs has resolved, ui.business may be unset.
  listen("dashboards-changed", async () => {
    if (!loaded) return;
    const open = selected();
    await loadDashTabs();
    if (open && !tabs.some((t) => t.name === open)) hideDash();
  }).catch((e) => console.debug("[cos] dashboards-changed", e));

  window.loadDashTabs = loadDashTabs;
  window.hideDash = hideDash;
})();
