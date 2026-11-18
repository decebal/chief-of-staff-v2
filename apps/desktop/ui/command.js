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
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = r.app ? `${r.name} (app business)` : r.name;
    const home = document.createElement("span");
    home.className = "home";
    home.textContent = r.triple?.home ?? "";
    li.append(name, home);
    if (r.missing) {
      const gone = document.createElement("span");
      gone.className = "muted small";
      gone.textContent = "home is missing";
      li.append(gone);
    }
    const counts = [`${r.live ?? 0} live`];
    if (r.lost) counts.push(`${r.lost} lost`);
    for (const text of counts) {
      const count = document.createElement("span");
      count.className = "muted small";
      count.textContent = text;
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
  }

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
      at = 0;
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
    dialog.showModal();
    filter.focus();
    loadRows();
  };
})();
