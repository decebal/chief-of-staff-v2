# Markdy 1.5.7, vendored

[Markdy](https://markdy.com) (MIT, `LICENSE` here) renders the report's
diagrams. The npm packages ship ESM only and the CLI's HTML loads from a CDN,
so this is a single IIFE built once and inlined into every report.

| | |
|---|---|
| File | `markdy-1.5.7.iife.min.js`, global `Markdy` |
| Exports | `createDiagram`, `exportDiagramAsVectorSvg` |
| Packages | `@markdy/core@1.5.7`, `@markdy/renderer-dom@1.5.7` |
| Size | 313,607 B, 85,772 B gzip |
| sha256 | `43c8f93d5ab202f8d3e1f8dfec37c6a645e3d4cab7b60f89568c95c0f2a5ca17` |

The report uses `exportDiagramAsVectorSvg(code)` for a static, finished SVG;
`createDiagram` is the animated player. Verified rendering from `file://` in
Chrome on 2026-10-16: timeline, loop and architecture.

## Rebuild

Pinned on purpose: the grammar changes often (about 150 releases since
2026-04). Upgrade deliberately, then re-run the report tests and look at a
rendered report.

```bash
mkdir -p /tmp/markdy-build
cd /tmp/markdy-build
npm init -y
npm i @markdy/core@1.5.7 @markdy/renderer-dom@1.5.7 esbuild
printf 'export { createDiagram, exportDiagramAsVectorSvg } from "@markdy/renderer-dom";\n' > entry.js
npx esbuild entry.js --bundle --minify --format=iife --global-name=Markdy --platform=browser --outfile=markdy.iife.min.js
```

`html2canvas` is a declared dependency of renderer-dom but is never imported,
so it is not in the bundle.
