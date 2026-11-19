// State art for cos, drawn for it (see README.md beside this file): one
// isometric stack lit from the top left, and one sign above it per state.
// Every colour is a class that style.css (`.art`) takes from tokens.css, so
// light and dark follow the scheme. Decorative: each svg is aria-hidden, and
// the state's headline beside it says the same in words.
(() => {
  const n = (v) => Math.round(v * 10) / 10;
  const poly = (cls, pts) => `<path class="${cls}" d="M${pts.map(([x, y]) => `${n(x)} ${n(y)}`).join("L")}Z"/>`;
  // A block whose top face is centred at (cx, ty), half-width a, height h.
  const block = (cx, ty, a, h, top = "a-top") => {
    const b = a / 2;
    return (
      poly("a-left", [[cx - a, ty], [cx, ty + b], [cx, ty + b + h], [cx - a, ty + h]]) +
      poly("a-right", [[cx + a, ty], [cx, ty + b], [cx, ty + b + h], [cx + a, ty + h]]) +
      poly(top, [[cx, ty - b], [cx + a, ty], [cx, ty + b], [cx - a, ty]])
    );
  };
  const floor = '<ellipse class="a-floor" cx="80" cy="98" rx="60" ry="18"/>';
  const stack = (cls = "", lit = "a-top") =>
    `<g class="a-stack ${cls}">${block(80, 72, 36, 14)}${block(80, 60, 30, 12)}${block(80, 50, 24, 10, lit)}</g>`;
  const arc = (r, cy, cls) => {
    const d = r * Math.SQRT1_2;
    return `<path class="a-stroke ${cls}" d="M${n(80 - d)} ${n(cy - d)}A${r} ${r} 0 0 1 ${n(80 + d)} ${n(cy - d)}"/>`;
  };
  const svg = (kind, body) =>
    `<svg class="art" data-kind="${kind}" viewBox="0 0 160 120" aria-hidden="true" focusable="false">${floor}${body}</svg>`;

  window.ART = {
    // A signal going out from the stack and coming back: reading.
    reading: svg("reading", stack() + arc(8, 40, "a-wave w1") + arc(15, 40, "a-wave w2") + arc(22, 40, "a-wave w3") + '<circle class="a-accent" cx="80" cy="40" r="2.5"/>'),

    // The stack as last read, with the arrows of a read going round over it.
    refreshing: svg(
      "refreshing",
      stack("a-stale") +
        '<g class="a-spin"><path class="a-stroke" d="M67.5 19a13 13 0 0 1 22-6.5"/><path class="a-stroke" d="M90 6.5v6.5h-6.5"/><path class="a-stroke" d="M92.5 23a13 13 0 0 1-22 6.5"/><path class="a-stroke" d="M70 35.5V29h6.5"/></g>',
    ),

    // An hourglass on the stack: still going, and taking a while.
    slow: svg(
      "slow",
      stack() +
        '<path class="a-glass" d="M73 7c0 8 7 9 7 13s-7 5-7 13h14c0-8-7-9-7-13s7-5 7-13z"/><path class="a-stroke" d="M70 7h20M70 33h20"/><path class="a-accent" d="M75.6 11h8.8L80 17.5z"/><path class="a-accent" d="M74.6 32c1.5-4 4-5 5.4-5s3.9 1 5.4 5z"/><path class="a-stroke a-drip" d="M80 20v6"/>',
    ),

    // An open, empty box with a plus beside it: nothing here yet, add one.
    empty: svg(
      "empty",
      block(80, 72, 36, 14, "a-hole") +
        poly("a-left", [[44, 72], [80, 54], [74, 41], [38, 59]]) +
        poly("a-right", [[80, 54], [116, 72], [122, 59], [86, 41]]) +
        '<circle class="a-accent" cx="124" cy="26" r="11"/><path class="a-mark" d="M124 20.5v11M118.5 26h11"/>',
    ),

    // A cloud with a crack through it, over a stack gone dim: no answer.
    unreachable: svg(
      "unreachable",
      stack("a-dim") +
        '<path class="a-cloud" d="M65 33h30a8 8 0 0 0 1.5-15.9 11 11 0 0 0-21.3-3.4A8.5 8.5 0 0 0 65 33z"/><path class="a-stroke a-crack" d="M83 11l-5 9h6l-5 9"/>',
    ),

    // A crescent moon over the stack: scaled down for the night.
    asleep: svg(
      "asleep",
      stack("a-dim") +
        '<path class="a-accent" d="M117 8a12 12 0 1 0 9 19.5A10 10 0 0 1 117 8z"/><path class="a-stroke a-z" d="M93 17h6l-6 7h6M102 7h4l-4 5h4"/>',
    ),

    // The sun coming up behind the stack, its top lit: starting.
    starting: svg(
      "starting",
      '<g class="a-rays"><path class="a-stroke" d="M80 6v6M62 14l4 4M98 14l-4 4M54 30h6M100 30h6"/></g><path class="a-accent" d="M64 36a16 16 0 0 1 32 0z"/>' +
        stack("", "a-lit"),
    ),

    // A tidy stack under a check: all clear.
    clear: svg("clear", stack() + '<circle class="a-accent" cx="80" cy="20" r="12"/><path class="a-mark" d="M74.5 20.5l4 4 7.5-8"/>'),

    // A plug pulled from its socket over a dim stack: Herdr is not running.
    herdr_down: svg(
      "herdr_down",
      stack("a-dim") +
        '<path class="a-stroke" d="M58 21h12M58 27h12"/><rect class="a-accent" x="70" y="16" width="9" height="16" rx="2"/><rect class="a-glass" x="88" y="16" width="9" height="16" rx="2"/><path class="a-stroke" d="M97 24h11"/><path class="a-stroke a-crack" d="M83 14l2 5-2 5 2 5"/>',
    ),

    // A speech bubble asking over the stack: an agent waits on you in Herdr.
    herdr_waiting: svg(
      "herdr_waiting",
      stack() +
        '<path class="a-accent" d="M64 8h32a4 4 0 0 1 4 4v14a4 4 0 0 1-4 4H80l-7 6v-6h-9a4 4 0 0 1-4-4V12a4 4 0 0 1 4-4z"/><path class="a-mark" d="M76 15a4 4 0 1 1 5.5 3.7c-1 .5-1.5 1.3-1.5 2.3M80 25v.5"/>',
    ),

    // A bell over the stack: something needs you.
    needs_you: svg(
      "needs_you",
      stack() +
        '<g class="a-swing"><path class="a-accent" d="M70 28c2-2 2.5-5 2.5-9.5a7.5 7.5 0 0 1 15 0c0 4.5.5 7.5 2.5 9.5z"/><path class="a-stroke" d="M77 31.5a3 3 0 0 0 6 0"/></g><circle class="a-badge" cx="92" cy="10" r="3.5"/>',
    ),
  };
})();
