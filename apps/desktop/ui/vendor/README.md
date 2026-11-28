# Vendored design assets

The design pack every `cos` page uses: Basecoat's prebuilt CSS (a vanilla
port of shadcn/ui, MIT) and the Geist Sans and Geist Mono variable fonts
(SIL OFL 1.1); and the docs room's markdown renderer (marked, MIT) and
sanitizer (DOMPurify, Apache 2.0). Nothing is fetched at runtime; the Tauri CSP stays
`default-src 'self'`. Licences sit beside each file.

The palette, type scale and radii are not here: they live in one file,
`../tokens.css`, which overrides Basecoat's neutral tokens with Geist
neutrals plus a blue primary and an amber accent. The blue and amber hex
values are hand-picked to pass the AA pairs in `tests/design.rs`; unlike the
neutrals below they were not read from the Geist stylesheet. `src/design.rs` inlines that file and the two fonts into the pages
`cos` serves itself (the Today report, the docs room, the MCP app view);
the desktop window links them directly.

| File | Source | Version | Size | sha256 |
|---|---|---|---|---|
| `basecoat/basecoat-1.1.0.min.css` | `dist/basecoat.cdn.min.css` (style `vega`, Tailwind 4.3.1 compiled in) from <https://registry.npmjs.org/basecoat-css/-/basecoat-css-1.1.0.tgz> | basecoat-css 1.1.0 | 236,766 B | `e6354ef1e99adc94257e579507f18cb00307a08cc2b976deb320368eb7c28e70` |
| `basecoat/LICENSE.md` | same tarball, `LICENSE.md` (MIT) | 1.1.0 | 1,068 B | `160a6cea26ff1931297957499ab739e94c4bc79ecea18b490c9f533fa76ace78` |
| `geist/Geist-Variable.woff2` | `dist/fonts/geist-sans/Geist-Variable.woff2` from <https://registry.npmjs.org/geist/-/geist-1.8.2.tgz> | geist 1.8.2 | 69,560 B | `206f01ed08e05c98a51daa7472331c3c71bbb8c1f9d55ea8c70074f3c528a706` |
| `geist/GeistMono-Variable.woff2` | `dist/fonts/geist-mono/GeistMono-Variable.woff2`, same tarball | geist 1.8.2 | 71,104 B | `55419aa06f58adcd7f145c6c920e56936416f17e528f60efc6032ffa4269972b` |
| `geist/LICENSE.txt` | same tarball, `LICENSE.txt` (SIL OFL 1.1) | 1.8.2 | 4,368 B | `930853ee1daa68554d9e35c8a9175affb74f699fad9a5da6ee5ebe76379d9137` |

| `marked/marked-18.1.2.umd.js` | `lib/marked.umd.js` from <https://registry.npmjs.org/marked/-/marked-18.1.2.tgz> | marked 18.1.2 | 49,207 B | `9eb730d2362ae1a8c6201698f17009e16671be6946d3d06503ddc0575283db7c` |
| `marked/LICENSE.md` | same tarball, `LICENSE` (MIT) | 18.1.2 | 2,942 B | `8e3a3f82f59a60958f56ca08f445647c32a4733dc7ca6c2c46f6eb898471ab9c` |
| `dompurify/purify-3.4.17.min.js` | `dist/purify.min.js` from <https://registry.npmjs.org/dompurify/-/dompurify-3.4.17.tgz> | dompurify 3.4.17 | 29,828 B | `cd37898f9782adf54820c413191b5691803f0f137c799d6edcfd38523b4c2c31` |
| `dompurify/LICENSE` | same tarball, `LICENSE` (Apache 2.0, used under that half of its MPL-2.0 OR Apache-2.0 dual licence) | 3.4.17 | 11,358 B | `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30` |
| `lucide/icons-0.556.0.js` | the `__iconNode` arrays of 81 icons in `dist/esm/icons/<name>.js` from lucide-react 0.556.0, React keys dropped, as `window.LUCIDE[name] = [[tag, attributes], …]` | lucide-react 0.556.0 | 16,411 B | `b0e5fe3007043791a1a19f58867a0363155530c01e0221a4357fb61e007ed62d` |
| `lucide/LICENSE` | same package, `LICENSE` (ISC, with the MIT notice for the Feather portions) | 0.556.0 | 2,004 B | `3c57143f22dac950efebf84002e360bba5458c7b90fe583b0667aaaef1a7256e` |
| `vgpu/vgpu-0.6.0.iife.js` | built from `vendor-build/vgpu/entry.js` by esbuild 0.28.2 (below) over <https://registry.npmjs.org/vgpu/-/vgpu-0.6.0.tgz>, <https://registry.npmjs.org/@vgpu/core/-/core-0.6.0.tgz>, <https://registry.npmjs.org/@vgpu/wgsl/-/wgsl-0.6.0.tgz>; ceiling 220,000 B | vgpu 0.6.0 | 210,078 B | `91f741c74137c73784a5e829d3cacbf8dc2e367b81f21068e1af905c00ec34d3` |
| `vgpu/LICENSE` | the `vgpu` tarball, `LICENSE` (MIT) | 0.6.0 | 1,069 B | `779258e329008bdb9330e6c1daad644ff867f164d11c4cc4404350479f3e92ee` |

Lucide is the window's one icon set. `states.js` draws an icon with
`icon(name)` as inline SVG at one stroke width (2), sized by the `--icon-*`
tokens and coloured by `currentColor`, so no colour is written for it; a
decorative icon is `aria-hidden`, an icon-only button carries its label as
`aria-label` (`tests/design.rs`). The pages `cos` serves (the Today report)
take the same icons as inline SVG markup from `cos::design::icon`, which reads
this file at compile time.

Tarball sha256, as downloaded on 2026-10-28: `basecoat-css-1.1.0.tgz`
`46eb1cedcd3845013d03a2f46341c63ec51f97185cef932eeb79a332a7c12b48`,
`geist-1.8.2.tgz` `560bbdc10dc5803412f9573519053ad4f533757798fc059e209d86bc0a219800`,
`marked-18.1.2.tgz` `9e3e0f5beb9c15a671025eb588c64dc8078a529121d1de809f8752bc422ad09d`,
`dompurify-3.4.17.tgz` `056cbdae276897208b88c21d1d779adfa09ad7be488a2f510a28e900d3bbffe7`.

marked and DOMPurify are the docs room's markdown reader. `src/docs.rs`
inlines both into the page's one `<script>` (`docs::markdown_js`), dropping
the source-map comment and writing `<!--`, `<script` and `</script` as
`\x3C…` so the HTML parser cannot end or derail the script early;
`tests/docs.rs` checks the result. The desktop window loads both files as
they are, for the run panel's answers (`app.js`, `markdown`), with the docs
room's DOMPurify settings.

## Geist neutrals in `tokens.css`

Surfaces, text and borders take Vercel's Geist gray scale, read on
2026-10-28 from the stylesheet behind <https://vercel.com/geist/colors>
(`--ds-*-value` HSL triples under `:root` for light and `.dark` for dark;
the page itself shows only `var(--ds-*)` names). Rounded to hex:

| Step | Light | Dark | Token |
|---|---|---|---|
| background-100 | `#ffffff` (100%) | `#0a0a0a` (4%) | `--background` |
| background-200 | `#fafafa` (98%) | `#000000` (0%) | `--sidebar` |
| gray-100 | `#f2f2f2` (95%) | `#1a1a1a` (10%) | `--muted`, `--popover` dark, `--card` dark, `--secondary` light |
| gray-200 | `#ebebeb` (92%) | `#1f1f1f` (12%) | `--ramp-0`, `--secondary` dark |
| gray-300 | `#e6e6e6` (90%) | `#292929` (16%) | `--border` light, `--chart-grid` |
| gray-400 | `#ebebeb` (92%) | `#2e2e2e` (18%) | `--border` dark |
| gray-500 | `#c9c9c9` (79%) | `#454545` (27%) | `--input`, `--chart-axis` |
| gray-700 | `#8f8f8f` (56%) | `#8f8f8f` (56%) | `--chart-5` |
| gray-900 | `#4d4d4d` (30%) | `#a1a1a1` (63%) | `--muted-foreground` |
| gray-1000 | `#171717` (9%) | `#ededed` (93%) | `--foreground` |

`tests/design.rs` holds every text token to WCAG AA on these surfaces.

Checked on the vendored CSS: no `@import`, no `@font-face`, no `http(s)` URL
(only `data:` SVG icons). Basecoat's JavaScript is not vendored: the window
uses native `<dialog>` and `<details>`, and `app.js` keeps writing backend
values through `textContent` only.

## Upgrade

```bash
mkdir -p /tmp/vendor-dl
curl -sSfL -o /tmp/vendor-dl/basecoat.tgz https://registry.npmjs.org/basecoat-css/-/basecoat-css-<v>.tgz
curl -sSfL -o /tmp/vendor-dl/geist.tgz https://registry.npmjs.org/geist/-/geist-<v>.tgz
curl -sSfL -o /tmp/vendor-dl/marked.tgz https://registry.npmjs.org/marked/-/marked-<v>.tgz
curl -sSfL -o /tmp/vendor-dl/dompurify.tgz https://registry.npmjs.org/dompurify/-/dompurify-<v>.tgz
```

A new marked or DOMPurify version also changes the file names in
`src/docs.rs` and the version banners `tests/docs.rs` counts.

## vgpu

vgpu is the Flows Map's GPU engine (WebGPU layout and drawing); the Map has
an SVG engine beside it for a machine without a GPU. Unlike the files above,
copied out of tarballs, it is built: esbuild bundles the exports
`vendor-build/vgpu/entry.js` names into one IIFE that sets
`globalThis.vgpu`, and tree-shakes out the rest of vgpu's dependencies (its
MCP server, zod, the Node adapter). `src/flows.rs` inlines the file into the
Flows page only, through `design::script_safe`. `tests/design.rs`
(`the_vendored_vgpu_is_pinned_and_within_its_ceiling`) checks its sha256
against the table above, the 220,000 B ceiling (the measured size plus under
5%; raising it needs the reason in the table row) and the absence of every
network and code-loading API.

Inputs, on 2026-11-12: `vgpu-0.6.0.tgz`
`eac147d66620a28e7f72cd20b6802854a2797410f8d11545b9d6ba0c2c9e59ce`,
`core-0.6.0.tgz` `be9341a33b7cbb55c87ee9062c1d7ab442dde36cf0c083785462d79f92eb9910`,
`wgsl-0.6.0.tgz` `304411c7f143658eeb41f7b9adde3cd3f97e45e6fefc4f3f8b5510a50576dd1e`;
`vendor-build/vgpu/package-lock.json`
`678893efd2658e9b3a4d3dd7eed8bbb22f9e6eeb8705ec30354e7dae88beacf5`; esbuild 0.28.2.

Build (from the repo root; needs Node and the network for `npm ci`):

```bash
npm ci --prefix vendor-build/vgpu
vendor-build/vgpu/node_modules/.bin/esbuild vendor-build/vgpu/entry.js --bundle --minify --format=iife --platform=browser --target=es2022 --legal-comments=none "--banner:js=/*! vgpu 0.6.0 (MIT), vendored for cos */" --outfile=apps/desktop/ui/vendor/vgpu/vgpu-0.6.0.iife.js
shasum -a 256 apps/desktop/ui/vendor/vgpu/vgpu-0.6.0.iife.js
```

Upgrade: change the three versions in `vendor-build/vgpu/package.json`,
`npm install --prefix vendor-build/vgpu`, rebuild with the new version in the
banner and the file name, copy `LICENSE` from the `vgpu` tarball, update the
rows and digests above, the file name in `src/design.rs`, and run
`task test` and `task test-e2e`.

Unpack, copy the files above, update this table (`shasum -a 256`), then run
`task test` (the design gates in `tests/design.rs`) and look at every page
in light and dark.
