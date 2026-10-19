# REFS — External Reference Repository Index & Clean-Room Boundaries

> Contents under `refs/` are never committed (`.gitignore`, except for this file). On a new machine, run `scripts/bootstrap-refs.sh` to restore all pinned commits from §D (~200 MB).

## A. Application & Ecosystem Conventions (ArtCraft / storytold & Format References)

| Directory | License | How We Use It | Clean-Room & License Boundary |
| --- | --- | --- | --- |
| `vectorcraft/` | MIT OR Apache-2.0 | **Core engine foundation**: reuse and adapt geometry (`geom`), boolean ops (`pathops`), color/CMS (`color`), rendering (`render`), text (`text`), effects (`effects`), bitmap tracing (`trace` -> PowerTRACE), SVG/PDF/EPS/CAD/Metafile I/O, and `egui` canvas/docking primitives, while reshaping the document model, tools, and UI into a pixel-faithful CorelDRAW workspace | License-compatible (`MIT OR Apache-2.0`); attributed in [`NOTICE`](../NOTICE) |
| `designcraft/` | Apache-2.0 | **Multi-page layout foundation**: reuse and adapt multi-page spreads, master pages, guides, and page-panel mechanics for CorelDRAW's multi-page publication model | License-compatible (`Apache-2.0`); attributed in [`NOTICE`](../NOTICE) |
| `cadcraft/` | Apache-2.0 | **Clean-room binary format & CAD precedent**: DXF/DWG handling and clean-room binary format test patterns | License-compatible (`Apache-2.0`); attributed in [`NOTICE`](../NOTICE) |
| `craft-libs/` | Apache-2.0 | Shared `craft-pdf` / `craft-print` / `craft-docpath` / `craft-ui` libraries | License-compatible (`Apache-2.0`); attributed in [`NOTICE`](../NOTICE) |
| `craft-common-actions/` | See repo | Shared GitHub Actions workflows | Verify license before reuse |
| `craft-fonts/` | Apache-2.0 | Separate font repository pattern (`CRAFT_FONTS_DIR` optional build input; fonts never committed to main repo) | Follow pattern |
| `libcdr/` (GitHub mirror) | **MPL-2.0** | CDR format structural semantics and version differences | **Read-only reference, never copy code**: MPL-2.0 is weak copyleft; document structural findings in `docs/reverse/` with citations and implement independently |
| `cdr-spec/` (Photopea CDR-specification) | Public specification | **Primary CDR format specification**: RIFF containers, chunk tables, coordinate encoding, version differences | Factual format knowledge; cite sections and URLs |
| `uniconvertor/` | **AGPL-3.0** | Format support matrix existence proof only | **Strict prohibition**: AGPL-3.0. **Never** open, read, copy, link, or adapt its source code |
| `rea/` (`cyberkaida/reverse-engineering-assistant`) | See repo | Optional Ghidra RE MCP | Only for inspecting operator-owned sample files; **never** point at proprietary binaries |
| `morluto-rea/` (`REA: Reverse Engineer Anything`) | MIT | Primary RE assistant (`npx rea-agents`) | Same as above — only inspect operator-owned sample files |

## B. Public Behavioral Documentation Sources (Cite URLs)

| Source | Purpose |
| --- | --- |
| CorelDRAW Online Help (Windows): `https://product.corel.com/help/CorelDRAW/Documentation-Windows/index.html` | Primary public reference for tool, command, property bar, docker, and workspace layout behavior |
| CorelDRAW 2022 Public User Guide PDF: `https://learn.corel.com/wp-content/uploads/2022/08/CorelDRAW-UserGuide-Mar22.pdf` | Publicly distributed user manual |
| CorelDRAW Learning Center: `https://www.coreldraw.com/en/learn/` | Public tutorials and workflow documentation |
| DLP `libcdr` Wiki: `https://wiki.documentfoundation.org/DLP/Libraries/libcdr` | Format support scope reference |
| Photopea CDR Specification: `https://github.com/photopea/CDR-specification` | Format structure specification |
| Kaitai Struct `coreldraw_cdr.ksy`: `https://formats.kaitai.io/coreldraw_cdr/` | Declarative container and chunk specification |

## C. Reverse-Engineering Tooling Red Lines

1. Tools such as REA (`morluto-rea`) may **only** inspect operator-owned `.cdr` sample files, synthetic fixtures, or our own output.
2. **Never** inspect a Corel installation directory, running process, or any proprietary binary (including trial installers).
3. Findings recorded in `docs/reverse/` must include the sample SHA-256 hash, public specification citations, and structural conclusions.

## D. Pinned Commits (`scripts/bootstrap-refs.sh`)

Pinned on 2026-10-19 (keep `scripts/bootstrap-refs.sh` and this table synchronized):

| Directory | Upstream | Pinned Commit |
| --- | --- | --- |
| `vectorcraft/` | `github.com/storytold/vectorcraft` | `a466625c8987550635cda951ebf36e08e1e80a56` |
| `designcraft/` | `github.com/storytold/designcraft` | `443ea4640ca368fcf0241b04813bd3817acd5df8` |
| `cadcraft/` | `github.com/storytold/cadcraft` | `0b32c2e6a14c761835ea62c01e7b20845971d781` |
| `craft-libs/` | `github.com/storytold/craft-libs` | `ae2f796b673f29cf3161f36eb6a8933f0a3fe028` |
| `craft-common-actions/` | `github.com/storytold/craft-common-actions` | `d7bf8f06e138618890d0ebe8a84a18b010bf3f66` |
| `craft-fonts/` | `github.com/storytold/craft-fonts` | `d1cf47d05e04426615fbda1d637b9ca6b1e7af35` |
| `libcdr/` | `github.com/LibreOffice/libcdr` | `22c396810e72d40b94f78c192eb06e6d09bc34a0` |
| `cdr-spec/` | `github.com/photopea/CDR-specification` | `f91b3c7e8c812b28473f6399e95a624a72a3d6b8` |
| `uniconvertor/` | `github.com/sk1project/uniconvertor` | `973d5b6f8fccce7e7bf9bc88e91bc80f9f9d9472` (AGPL — do not read source) |
| `rea/` | `github.com/cyberkaida/reverse-engineering-assistant` | `a05a6b0a18649efd6d74ec8447bdadf8d468c365` |
| `morluto-rea/` | `github.com/morluto/rea` | `ba866a26ecbe179a717e746cbc0d1605ae616ad5` |
