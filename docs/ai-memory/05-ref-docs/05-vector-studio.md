# Vector Studio (HeyPuter/everything) — Reference Notes

> Registered 2026-10-21. Local checkout: `refs/vector-studio/` (sparse checkout of
> `src/vector-studio` from the `HeyPuter/everything` monorepo, pinned at
> `75fec282dd56aa107eff8150cfcd63d2c3149192`). Restore with `scripts/bootstrap-refs.sh`.
> Upstream: <https://github.com/HeyPuter/everything/tree/main/src/vector-studio>

## What it is

A professional web vector illustration editor ("alternative to Adobe Illustrator,
CorelDRAW and Affinity Designer") from the HeyPuter "everything" project, which rebuilds
proprietary software as MIT-licensed open source. Plain ES-module JavaScript (no UI
framework), Canvas 2D rendering, Vite build, Vitest tests. Document storage and live
collaboration are tied to the Puter cloud (`worker/`), which is irrelevant to CoralCraft.

## License position

- Monorepo root `LICENSE` is MIT (Puter Technologies Inc. 2026); vector-studio has no
  overriding license file, so MIT applies.
- MIT is compatible with CoralCraft (`MIT OR Apache-2.0`) and passes the no-GPL/AGPL/MPL
  rule. If any code is ever adapted, keep the copyright notice and add attribution to
  `NOTICE`.
- Third-party npm dependencies carry their own licenses; the repo has a
  `tools/check-licenses.mjs` gate worth a look as a pattern.

## Why it is reference-only (operator decision 2026-10-21)

CoralCraft is a Rust/egui desktop application re-based on the VectorCraft fork (Phase V).
Vector Studio is a pure-JS Canvas 2D web app whose feature set (Illustrator parity)
overlaps almost entirely with what VectorCraft already provides in Rust. Integrating it
would mean a second, parallel technology stack — prohibited by the "reuse VectorCraft,
never reimplement" rule. Its only Rust is a tiny `wasm/geom` helper (4 files), not worth
adopting. Role assigned: **read-only behavioral and algorithmic reference**, same
category as `libcdr`.

## Verified structure (not README claims)

- `src/model/` — flat immutable node store with transactions/undo (`document.js`),
  Bezier math (`bezier.js`), shape generators, bounds/transforms, live artwork
  (`live.js`) and live geometry effects (`geom-effects.js`, `corners.js`, `warp.js`).
- `src/render/` — Canvas 2D renderer with cached `Path2D`, viewport culling, overlay
  canvas, bitmap fast path for pan/zoom, hit testing (`hit.js`), raster effects.
- `src/tools/`, `src/ops/`, `src/commands/` — one class per tool, editing ops, command
  registry (menus/shortcuts/buttons run commands by id — same pattern as our command
  registry).
- `src/features/` — one module per feature, each registering tools/panels/commands:
  `envelope`, `liquify`, `dimension`, `mesh`, `perspective`, `repeat`, `regions`
  (paint regions / Smart Fill analog), `interlace`, `graphs`, `three-d`, `symbols`,
  `brushes`, `patterns`, `trace`, `corners`, `live-shapes`, `paint`, `icc`,
  `interop` (PDF/EPS/DXF/TIFF/PSD), `prepress`, `slices`, `collab`, `shortcuts`,
  `workspaces`, and more.
- Scale: ~785 JS files, ~9.1 MB under `src/`; `wasm/geom` is 4 small Rust files.

## Where to read first for Phase V tasks

| CoralCraft task | Vector Studio reading |
| --- | --- |
| V5.2 interactive effects (Blend, Envelope, Distort) | `src/features/envelope/`, `src/features/liquify/`, `src/model/warp.js`, `src/model/live.js` |
| V5.4 Smart Fill | `src/features/regions/` (paint regions + region selection) |
| V5.4 Dimension tools | `src/features/dimension/` (linear/angular/radial, attached to measured objects, units/precision/scale) |
| V4.x workspace / shortcuts | `src/ui/`, `src/features/shortcuts/`, `src/features/workspaces/` |
| V5.3 palettes/swatches | `src/features/paint/`, `src/features/libraries/` |

## Caveats

- README feature list is marketing-level and unverified; confirm behavior in code before
  citing it as a reference.
- Collaboration (`worker/`, `src/features/collab/`) and Puter-cloud file storage are
  platform-specific; ignore.
- Do not copy its UI phrasing or assets blindly into CoralCraft — it is a clean-room
  Illustrator rebuild, but our target UX is CorelDRAW (per `docs/target-app-parity.md`).
