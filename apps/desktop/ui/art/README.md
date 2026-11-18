# State art

Original pictures drawn for `cos` (no pack, no copy), one per state a view
can be in. Every region-level state in the window (a tab, a panel, a frame
that is loading, empty, slow or out of reach) shows its picture beside an
icon, a one-line headline and the next action; a single row shows the icon
only. `stateView(kind, …)` in `../app.js` is the one place either is built,
and `tests/design.rs` (`every_state_renders_through_state_view_with_an_icon_and_art`)
fails a kind without a picture here or an icon in the vendored set.

`art.js` sets `window.ART[kind]` to an SVG string. Each is one isometric
stack of three blocks lit from the top left (top face lightest, right face
darkest) with one sign above it. Colours are classes that `../style.css`
(`.art`) takes from the `--art-*` tokens and the state colours in
`../tokens.css`, so light and dark follow the scheme and no colour is written
here. Every picture is `aria-hidden`: the headline says the state in words.

| Kind | Picture | Meaning |
|---|---|---|
| `reading` | a signal going out from the stack | a first read: nothing to show yet |
| `refreshing` | the stack, faded, under turning arrows | the last read is shown while a new one runs |
| `slow` | an hourglass on the stack | still reading; the line says what it waits on |
| `empty` | an open, empty box and a plus | nothing configured yet; the action adds one |
| `unreachable` | a cracked cloud over a dimmed stack | the source did not answer; the action retries it |
| `asleep` | a crescent moon over a dimmed stack | scaled down for the night |
| `starting` | the sun rising behind the stack, its top lit | coming up |
| `clear` | a check over a tidy stack | nothing needs you |
| `needs_you` | a bell over the stack | something waits on you |

Motion: the signal pulses, the arrows turn, the sand drips, the bell swings,
the rays glow; all of it only under `prefers-reduced-motion: no-preference`
(`tests/design.rs`, `every_animation_respects_reduced_motion`), so a reduced
setting shows the still frame. The file holds all nine in 4,429 B; the gate
allows 6 KB per picture.
