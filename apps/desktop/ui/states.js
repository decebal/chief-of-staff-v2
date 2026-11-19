// What every script of the window shares: element helpers, the icon set, and
// every loading, empty, slow and error state. Loaded before command.js,
// sidebar.js, dash.js and app.js, so each can build a state as it loads.
// Values reach the DOM through textContent or a property, never innerHTML;
// the one markup sink is the vendored state art, which carries no data.

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

const logDev = (where, e) => console.debug(`[cos] ${where}`, e);

// ---- icons: one vendored set (vendor/lucide), inline SVG coloured by the
// text around it; decorative unless given a label.

const SVG_NS = "http://www.w3.org/2000/svg";
function icon(name, { label, tone, className = "" } = {}) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("class", `icon ${className}`.trim());
  if (tone) svg.dataset.tone = tone;
  if (label) {
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", label);
  } else svg.setAttribute("aria-hidden", "true");
  const nodes = window.LUCIDE?.[name];
  if (!nodes) logDev("no icon", name);
  for (const [tag, attrs] of nodes ?? []) {
    const n = document.createElementNS(SVG_NS, tag);
    for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
    svg.append(n);
  }
  return svg;
}

// `<i data-icon="name">` in index.html becomes the icon.
function hydrateIcons(root = document) {
  for (const i of root.querySelectorAll("i[data-icon]")) i.replaceWith(icon(i.dataset.icon));
}
hydrateIcons();

// A button with an icon and its label; an icon-only button carries the label
// as its accessible name.
function iconButton(name, label, { variant = "ghost", size = "sm", iconOnly = false, title, onClick, disabled = false } = {}) {
  const b = el("button", { className: "btn", type: "button", disabled }, icon(name), iconOnly ? null : el("span", { textContent: label }));
  b.dataset.variant = variant;
  b.dataset.size = iconOnly ? (size === "sm" ? "icon-sm" : "icon") : size;
  if (iconOnly) b.setAttribute("aria-label", label);
  if (title) b.title = title;
  if (onClick) b.addEventListener("click", onClick);
  return b;
}

function setLabel(button, text) {
  const span = button.querySelector("span");
  if (span) span.textContent = text;
  else button.append(el("span", { textContent: text }));
}

// ---- states: every loading, empty, slow and error state is built here.
// A region (a tab, a panel, a frame) gets the kind's art from art/art.js, an
// icon, a headline, a line and the next action; a row gets the icon and a
// line. tests/design.rs holds every kind to an icon here and a picture in
// ART, and every state container in the window to these two functions.

const STATE = {
  reading: ["loader-circle", "info"],
  refreshing: ["refresh-cw", "info"],
  slow: ["hourglass", "warn"],
  empty: ["inbox", "muted"],
  unreachable: ["cloud-off", "bad"],
  asleep: ["moon", "muted"],
  starting: ["sunrise", "info"],
  clear: ["circle-check", "ok"],
  herdr_down: ["power", "bad"],
  herdr_waiting: ["message-square", "warn"],
  needs_you: ["bell", "warn"],
};
const MOVING = ["reading", "refreshing"];
// How long a read may run before its state says what it is waiting on.
const SLOW_AFTER_MS = 6000;

function stateIcon(kind) {
  return icon(STATE[kind][0], { className: MOVING.includes(kind) ? "spin" : "" });
}

function stateArt(kind) {
  const t = document.createElement("template");
  t.innerHTML = window.ART?.[kind] ?? "";
  return t.content.firstElementChild;
}

// `action`: { icon, label, onClick, variant } — a retry re-fetches what
// failed; nothing here reloads the window.
function stateView(kind, { title, detail, action, line = false, over = false } = {}) {
  const tone = STATE[kind][1];
  const act = action ? iconButton(action.icon, action.label, { variant: action.variant ?? "outline", onClick: action.onClick }) : null;
  const box = line
    ? el("p", { className: "state-line" }, stateIcon(kind), el("span", { textContent: title, title: detail ?? "" }), act)
    : el(
        "div",
        { className: `state${over ? " over" : ""}` },
        stateArt(kind),
        el(
          "div",
          { className: "state-text" },
          el("p", { className: "state-title" }, stateIcon(kind), el("span", { textContent: title })),
          detail ? el("p", { className: "state-detail", textContent: detail }) : null,
          act,
        ),
      );
  box.setAttribute("role", "status");
  box.setAttribute("aria-live", "polite");
  box.dataset.kind = kind;
  box.dataset.tone = tone;
  return box;
}

function spanWords(secs) {
  const s = Math.max(0, Math.round(secs));
  if (s < 90) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min`;
  if (s < 86400) {
    const [h, m] = [Math.floor(s / 3600), Math.floor((s % 3600) / 60)];
    return m ? `${h} h ${m} min` : `${h} h`;
  }
  const [d, h] = [Math.floor(s / 86400), Math.floor((s % 86400) / 3600)];
  return h ? `${d} d ${h} h` : `${d} d`;
}
const nowSecs = () => Date.now() / 1000;

// A frame shows a state over itself until its page has loaded: nothing for
// the first moments (a held page lands at once), then reading, then slow with
// what it waits on and a Try again that loads it again.
const framing = {};
function frameLoad(id, src, { title, waiting, quiet = false } = {}) {
  const frame = $(id);
  const box = frame.parentElement;
  const f = (framing[id] ??= { loaded: false });
  clearTimeout(f.soon);
  clearTimeout(f.slow);
  const show = (kind, headline, action) => {
    f.over?.remove();
    f.over = stateView(kind, { title: headline, detail: waiting?.(), action, over: true });
    box.append(f.over);
  };
  const again = { icon: "rotate-ccw", label: "Try again", onClick: () => frameLoad(id, src, { title, waiting }) };
  if (!(quiet && f.loaded)) {
    f.soon = setTimeout(() => show("reading", title), 150);
    f.slow = setTimeout(() => show("slow", `${title}: still waiting`, again), SLOW_AFTER_MS);
  }
  f.waiting = waiting;
  frame.addEventListener(
    "load",
    () => {
      clearTimeout(f.soon);
      clearTimeout(f.slow);
      f.over?.remove();
      f.over = null;
      f.loaded = true;
    },
    { once: true },
  );
  frame.src = src;
}

// What a frame's state says it waits on, told again as it changes.
function frameWaiting(id) {
  const f = framing[id];
  const p = f?.over?.querySelector(".state-detail");
  const text = f?.waiting?.();
  if (!f?.over || !text) return;
  if (p) p.textContent = text;
  else f.over.querySelector(".state-title")?.after(el("p", { className: "state-detail", textContent: text }));
}

// A pill in a frame's corner: what runs behind the page it shows.
function framePill(boxId, id) {
  let pill = $(id);
  if (!pill) {
    pill = el("span", { id, className: "frame-status", role: "status", hidden: true });
    $(boxId).append(pill);
  }
  return pill;
}
