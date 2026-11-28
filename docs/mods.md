# Local mods

A local mod is a page of your own that the desktop app shows in a tab after the
built-in tabs. It lives on your machine, outside the repo, and the app never
writes into it. The app reads the mod's files, gives it its own data per
business, and keeps it in a sandbox with no network.

## Folder

The mods folder is `mods/` beside the business registry: `$COS_BUSINESSES`'s
directory, else `$XDG_CONFIG_HOME/cos/`, else `~/.config/cos/`. `COS_MODS`
overrides it (tests use this). Each subfolder whose name is lowercase letters,
digits and dashes is one mod. Other names and loose files are ignored.

The app writes `mods/AGENTS.md` on every start, describing the folder for an
agent or a person. It never writes anything else there, so a file of your own
such as `AGENTS.local.md` is never touched.

```
~/.config/cos/mods/
  AGENTS.md        written by the app
  notes/
    mod.json
    index.html
    app.js
```

## mod.json

```json
{ "title": "Notes", "icon": "file-text", "order": 10 }
```

- `title`: required. The tab's label.
- `icon`: optional, a Lucide name from the app's vendored set. Unknown or absent
  names fall back to `package`.
- `order`: optional number. Tabs sort by it, lowest first, then by folder name.

Unknown keys, a missing `index.html`, or a bad value list the mod with its error,
shown in its tab through the app's standard error state. The window keeps running.

## The page

`index.html` is the entry. Scripts and styles load from the mod's own folder with
relative paths. Inline `<script>` and `style` attributes are blocked by the
policy below, so set styles from script or from a `.css` file.

`tokens.css` is served to every mod. Link it to match the window's colours,
type scale and light or dark theme:

```html
<link rel="stylesheet" href="tokens.css">
```

`tokens.css` and `vendor/geist/*` are the app's files. A file of the same name
in a mod's folder is never served.

## Messages

The page runs in a sandboxed frame (`sandbox="allow-scripts allow-popups"`, no
same-origin) and talks to the window through `window.parent.postMessage`. The
window answers with `"*"` because the frame's origin is opaque, so a mod should
check `event.source === window.parent` on every reply it receives.

Each showing of a mod gets a random load id, in the page's URL as `?load=<id>`.
The page reads it from `location.search` and sends it with every message. The
window answers only the id it issued for the showing on screen, so a page left
over from an earlier showing gets nothing.

| Sent by the mod | Window replies |
|---|---|
| `{type: "cos-mod-ready", load}` | `{type: "cos-mod-host", business, theme}`: whose data this is, and `"light"` or `"dark"` |
| `{type: "cos-mod-read", load, seq}` | `{type: "cos-mod-data", seq, ok, data}`: `data` is `null` before the first save |
| `{type: "cos-mod-save", load, seq, data}` | `{type: "cos-mod-saved", seq, ok, error?}` |

`seq` is optional and echoed back so a mod can match replies to requests. `data`
must be one JSON object of at most 256 KiB; anything else is refused with an
`error` and nothing is written.

A mod is one page. The frame may load another page of the same mod, but without
its `load` id nothing it sends is answered, and it may never load another mod's
page: the app refuses that navigation.

## Data

Saved data is per business, in the workspace's runtime directory:

```
<workspace>/.chief-of-staff/mods/<name>.json
```

The runtime directory is gitignored, and writes are atomic. A save needs a
workspace that is already set up (`cos` has created its `.chief-of-staff/`).
A business switch reloads the window, so a mod always shows the business it was
opened for. Selecting a mod's tab again re-reads `mods/` and reloads its frame,
so an edit to `index.html` shows on the next selection with no restart.

## Security model

- Origin: the page is served on `cosmod://localhost/<mod>/<file>` (on Windows,
  `http://cosmod.localhost/...`), with an opaque origin in a sandboxed frame.
- Reads: a request is served only from inside its mod's own folder. `..`,
  absolute paths, percent-encoded paths and a symlink whose target lies outside
  the folder are refused (400 or 403). Only GET is served. Percent-encoded
  paths are refused, so file names must need no escaping: use letters, digits,
  dashes, dots and underscores.
- Navigation: the frame may load only the shown mod's pages. A link or script
  that points at another mod is refused by the window's navigation hook.
- Policy, sent with every response:
  `default-src 'self' cosmod:; script-src 'self' cosmod:; style-src 'self' cosmod:;
  img-src 'self' cosmod: data:; font-src 'self' cosmod:; connect-src 'none';
  frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`.
  A mod cannot reach the network, load another frame, or run inline script.
- Data: the window reads and writes only the app business's workspace and only
  the named mod's file. A mod cannot name another business or another mod's data.
- Links: `http`, `https` and `mailto` links open in the default browser; the
  app's own schemes stay in place; other schemes are refused. This is the same
  hook the report and the Flows tab use (`apps/desktop/src/lib.rs`).

A mod is code you wrote and run on your machine. The sandbox keeps it from the
network and from the other windows' data; it does not make an untrusted mod safe
to install.

## Example

A counter in a folder named `counter`:

`counter/mod.json`

```json
{ "title": "Counter", "icon": "plus", "order": 20 }
```

`counter/index.html`

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="stylesheet" href="tokens.css">
  <link rel="stylesheet" href="counter.css">
</head>
<body>
  <main>
    <h1>Counter</h1>
    <p id="count" class="num">…</p>
    <button id="up" class="btn" type="button">Add one</button>
  </main>
  <script src="counter.js"></script>
</body>
</html>
```

`counter/counter.js`

```js
let count = 0;
const show = () => (document.getElementById("count").textContent = String(count));
const load = new URLSearchParams(location.search).get("load");
let seq = 0;
window.addEventListener("message", (ev) => {
  if (ev.source !== window.parent) return;
  if (ev.data?.type === "cos-mod-data" && typeof ev.data.data?.count === "number") {
    count = ev.data.data.count;
    show();
  }
});
document.getElementById("up").addEventListener("click", () => {
  count += 1;
  show();
  window.parent.postMessage({ type: "cos-mod-save", load, seq: ++seq, data: { count } }, "*");
});
window.parent.postMessage({ type: "cos-mod-ready", load }, "*");
window.parent.postMessage({ type: "cos-mod-read", load, seq: ++seq }, "*");
```

`counter/counter.css` is empty or holds your own rules.

## Checking it

The mod's frame and the window's replies run in the app, so a mod is checked by
opening its tab. The Rust side is covered by `apps/desktop/tests/mods.rs`
(list, scheme policy and refusals, data round trip and limits) and the frame's
markup by the same file. The window's own script is covered by
`tests/design.rs`. Nothing here drives a live window headless: the frame's
rendering and the browser link hook need a run of the app.
