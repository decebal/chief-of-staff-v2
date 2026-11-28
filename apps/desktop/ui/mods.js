// Local mods: one tab per folder of the config directory's mods/. Each showing
// carries a random load id in the page URL; the window answers only messages
// that echo it. Globals `openTab`, `ui`, `frameLoad` and `stateView` come from
// app.js, states.js and kit.js.
(() => {
  const { invoke } = window.__TAURI__.core;
  const nav = document.getElementById("mod-tabs");
  const notice = document.getElementById("mod-notice");
  const state = document.getElementById("mod-state");
  const frame = document.getElementById("mod-frame");

  let mods = [];
  // The showing the frame holds: its mod and load id. Null while none is shown.
  let current = null;
  let opens = 0;

  const base = navigator.userAgent.includes("Windows") ? "http://cosmod.localhost/" : "cosmod://localhost/";
  const modUrl = (name) => `${base}${name}/index.html`;
  const dark = () => matchMedia("(prefers-color-scheme: dark)").matches;
  const iconFor = (name) => (name && Object.hasOwn(window.LUCIDE ?? {}, name) ? name : "package");

  function selected() {
    if (!document.getElementById("view-mod").classList.contains("active")) return null;
    return nav.querySelector('.app-tab[aria-current="page"]')?.dataset.mod ?? null;
  }

  function renderTabs() {
    const current = selected();
    nav.replaceChildren(
      ...mods.map((m) => {
        const b = Object.assign(document.createElement("button"), { className: "app-tab", type: "button", textContent: m.title });
        b.prepend(icon(iconFor(m.icon)));
        b.dataset.view = "mod";
        b.dataset.mod = m.name;
        if (m.name === current) b.setAttribute("aria-current", "page");
        b.addEventListener("click", () => openTab("mod", "", m.name));
        return b;
      }),
    );
  }

  async function loadModTabs() {
    try {
      mods = await invoke("mods_list");
      notice.replaceChildren();
      notice.hidden = true;
    } catch (e) {
      mods = [];
      notice.replaceChildren(
        stateView("unreachable", {
          title: "Local mods not read",
          detail: String(e),
          line: true,
          action: { icon: "rotate-ccw", label: "Retry", onClick: loadModTabs },
        }),
      );
      notice.hidden = false;
    }
    renderTabs();
  }

  // Each selection reads the list again and reloads the frame: a mod added or
  // fixed on disk shows at once, and a page always starts from its own files.
  async function showMod(name) {
    const open = ++opens;
    await loadModTabs();
    if (open !== opens) return;
    const m = mods.find((x) => x.name === name);
    current = null;
    if (!m) {
      frame.hidden = true;
      state.hidden = false;
      state.replaceChildren(
        stateView("empty", { title: "No mod by that name", detail: "Its folder is not in the mods folder." }),
      );
      return;
    }
    if (m.error) {
      frame.hidden = true;
      state.hidden = false;
      state.replaceChildren(
        stateView("unreachable", {
          title: `${m.title} is not loaded`,
          detail: m.error,
          action: { icon: "rotate-ccw", label: "Try again", onClick: () => showMod(name) },
        }),
      );
      return;
    }
    state.hidden = true;
    state.replaceChildren();
    frame.hidden = false;
    try {
      await invoke("mod_open", { name });
    } catch (e) {
      frame.hidden = true;
      state.hidden = false;
      state.replaceChildren(
        stateView("unreachable", {
          title: `${m.title} is not loaded`,
          detail: String(e),
          action: { icon: "rotate-ccw", label: "Try again", onClick: () => showMod(name) },
        }),
      );
      return;
    }
    if (open !== opens) return;
    const load = crypto.randomUUID();
    current = { name, load };
    frame.setAttribute("aria-label", m.title);
    frameLoad("mod-frame", `${modUrl(name)}?load=${load}`, {
      title: `Opening ${m.title}`,
      waiting: () => "It runs from its own folder in the config directory.",
    });
  }

  window.addEventListener("message", async (ev) => {
    const cur = current;
    if (!cur || ev.source !== frame.contentWindow) return;
    const d = ev.data ?? {};
    // A reply goes out only while the showing it answers is still the one shown.
    const reply = (msg) => {
      if (current === cur) frame.contentWindow?.postMessage(msg, "*");
    };
    if (d.type === "cos-mod-ready") {
      if (d.load === cur.load) reply({ type: "cos-mod-host", business: ui.business, theme: dark() ? "dark" : "light" });
      return;
    }
    if (d.load !== cur.load) return;
    const seq = Number.isInteger(d.seq) ? d.seq : null;
    if (d.type === "cos-mod-read") {
      try {
        reply({ type: "cos-mod-data", seq, ok: true, data: (await invoke("mod_read", { name: cur.name })) ?? null });
      } catch (e) {
        reply({ type: "cos-mod-data", seq, ok: false, data: null, error: String(e) });
      }
    } else if (d.type === "cos-mod-save") {
      try {
        await invoke("mod_save", { name: cur.name, data: d.data });
        reply({ type: "cos-mod-saved", seq, ok: true });
      } catch (e) {
        reply({ type: "cos-mod-saved", seq, ok: false, error: String(e) });
      }
    }
  });

  window.loadModTabs = loadModTabs;
  window.showMod = showMod;
})();
