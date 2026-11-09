# CoralCraft CorelDRAW Main-Application Roadmap Design

> Status: direction approved by the operator on 2026-11-09; written specification awaiting operator review.
> Scope: the complete CorelDRAW main application, built on the VectorCraft fork.
> This is an umbrella roadmap. Each delivery phase requires its own specification and implementation plan; only P0 is planned first.

## 1. Context and Problems in the Current Plan

CoralCraft completed the VectorCraft full source sync and the V0-V4.2 migration work. The active Phase V plan remains useful as a migration plan, but it is not a complete product roadmap for the CorelDRAW main application.

The current plan has these limitations:

1. V4-V7 primarily finish workspace wiring, a small set of signature features, MCP consolidation, and legacy removal. They do not cover the full CorelDRAW main-application scope.
2. V5 groups too many semantically different features into four tasks and omits important capability domains such as professional typography, object and colour styles, symbols, advanced fills, production output, and data-driven workflows.
3. Leaving MCP integration to V6 would repeat the mistake of adding automation after features have already taken shape. Every feature must enter the shared command and automation path as it lands.
4. Legacy removal is engineering hygiene, not a product-parity milestone. It should proceed continuously when safe.
5. `docs/target-app-parity.md` contains legacy 100% claims that are inconsistent with the current pre-alpha state and the post-sync evidence base.
6. Object counts, tool counts, and self-round trips are insufficient acceptance criteria. They do not prove editable semantics, interaction correctness, independent interoperability, or production readiness.
7. The reference set predates important 2026 activity, especially TraceDraw and recent libcdr format and fuzzing work.

## 2. Product Boundary

The target is the complete CorelDRAW main application:

- Vector drawing, node editing, shaping, snapping, transformation, alignment, and distribution.
- Artistic and paragraph text, professional typography, tables, and multipage layout.
- Pages, master layers, the desktop/pasteboard, guides, symbols, clones, and object/data management.
- Fills, outlines, transparency, colour and object styles, palettes, and colour management.
- PowerClip and live, editable CorelDRAW-class effects.
- Bitmap placement, tracing, common adjustments, and bitmap effects used in vector workflows.
- Prepress, separations, PDF/X, imposition, print preview, data merge, and asset export.
- CDR/CMX interoperability, native VectorCraft compatibility, CLI/MCP, and sandboxed scripting.

The following are outside this roadmap:

- A complete PHOTO-PAINT-class raster editor.
- Microsoft VBA or COM runtime compatibility.
- Proprietary user-interface assets, presets, icons, fonts, or other protected content.
- Mandatory cloud services or telemetry.

## 3. Architecture and Delivery Principles

### 3.1 One Application Base

VectorCraft remains CoralCraft's only application base. There will be no second whole-application rebase onto another editor.

Existing VectorCraft subsystems remain mandatory reuse candidates: geometry, path operations, rendering, text, effects, tracing, file I/O, colour management, docking, widgets, command dispatch, MCP, and packaging.

TraceDraw becomes a primary reference and possible donor for CoralCraft-owned CorelDRAW extensions. Adoption is module by module after source and test review; it is not a wholesale replacement of the VectorCraft document, renderer, tools, or engine.

### 3.2 Editable Vector Semantics

"Real vector" means that paths, nodes, text, parametric shapes, PowerClip containers, and effects remain editable through the complete lifecycle:

`document model -> command/history -> tool interaction -> render -> native persistence -> interchange -> reopen/edit`

Rasterization is acceptable only for bitmap content and inherently raster effects, or as an explicitly reported export fallback. A preview bitmap or a flattened path must not be presented as preserving an editable vector feature.

### 3.3 Vertical Slices

Every feature lands as a thin vertical slice through all applicable layers:

1. Document semantics and invariants.
2. Shared engine command, selection behaviour, and undo/redo.
3. Rendering and export semantics.
4. Tool, Property Bar, Docker, menu, and shortcut interaction.
5. Native `.vectorcraft`/`.drawcraft`/`.vctemplate` persistence, including namespaced CoralCraft data.
6. CDR/CMX mapping or an explicit lossless residual and warning when the format cannot represent the feature.
7. CLI/MCP exposure through the shared command registry.
8. Unit, integration, UI, interchange, and scenario tests appropriate to the feature.

### 3.4 Continuous Workstreams

These are not terminal phases:

- CDR/CMX interoperability advances with every feature.
- MCP and CLI support advance with every feature.
- Unique legacy behaviour is ported and `legacy/` shrinks continuously.
- VectorCraft upstream fixes are synced in small, audited increments.
- Performance, hostile-input handling, accessibility, and recovery are checked in every phase.

## 4. Capability Domains

The full target is tracked by capability, not by the number of menu entries.

### D1. Objects, Geometry, and Interaction

- Complete tool mapping and context-sensitive Property Bar modes.
- Pick and Shape interactions; node types and handles; open/closed subpaths.
- Weld, trim, intersect, simplify, boundary, combine/break apart, outline-to-object, offset, and shape building.
- Knife, eraser, crop, virtual segment delete, smear, twirl, attract/repel, roughen, and variable outlines.
- Rectangles, ellipses, polygons/stars, spirals, common shapes, graph paper, tables, dimensions, and connectors.
- Snapping, dynamic guides, ruler guidelines, grids, alignment, distribution, transformations, and step-and-repeat.
- Groups, symbols, clones, object naming, locking, visibility, search, and object data.

### D2. Appearance, Colour, and Styles

- Uniform, fountain, two-colour, bitmap/vector pattern, texture, and mesh fills.
- Outline width, caps, joins, dashes, arrowheads, calligraphic nibs, scaling, and paint order.
- Uniform and graded transparency, merge modes, and opacity masks.
- RGB, CMYK, grey, Lab, spot, registration, palettes, colour styles, harmonies, and document palettes.
- Object styles, style inheritance, find/replace, and attribute copying.

### D3. Text and Multipage Layout

- Artistic text, paragraph frames, linked frames, text on a path, and text wrapped around objects.
- OpenType shaping, bidirectional text, character and paragraph styles, font fallback, and glyph browsing.
- Columns, justification, indents, tabs, bullets, drop caps, hyphenation, leading, and tracking.
- Tables, page numbering, facing pages, per-page setup, master layers, desktop objects, and page overview.
- Templates, page/asset reuse, and long-document recovery.

### D4. PowerClip, Live Effects, and Imaging

- PowerClip place-inside, edit contents, extract, lock, nesting, and selection behaviour.
- Blend, Contour, Distort, Envelope, Extrude, Drop/Block Shadow, Transparency, Lens, Perspective, Bevel, and Symmetry.
- Ordered live-effect stacks with edit, hide, remove, clone, and expand operations.
- Bitmap placement, cropping, resampling, colour adjustment, common bitmap effects, and external-editor round trips where appropriate.
- Outline and centreline tracing, colour reduction, smoothing, and production presets.

### D5. Print, Prepress, and Production

- ICC colour management, rendering intents, proof colours, overprint preview, and separations.
- Spot inks through rendering and PDF output; PDF/X-1a, X-3, and X-4.
- Media/trim/bleed boxes, crop and registration marks, imposition, and print preview.
- Data/print merge, barcodes where appropriate, batch export, and reusable export presets.
- Cutting/signage workflows, including HPGL and nesting where supported by verified use cases.

### D6. Interchange, Automation, and Ecosystem

- CDR/CMX feature coverage by format generation and version, with explicit warnings and residual preservation.
- Every VectorCraft format retained through upstream syncs and compatibility-matrix tests.
- Native CoralCraft extension data preserved losslessly in VectorCraft documents and templates.
- Shared command coverage in the desktop, CLI, MCP, and sandboxed scripting.
- Templates, symbol/asset libraries, palettes, and user-generated presets with auditable licences.

## 5. Phased Delivery Roadmap

### P0. Evidence Baseline and TraceDraw Adoption Audit

This phase precedes further large V4.3 implementation work.

Deliverables:

1. Close the post-sync S2, S4, and S5 documentation/evidence items. Run the libcdr differential oracle and real-sample checks on a machine with their prerequisites; skipped runs remain explicitly unverified.
2. Refresh measured statistics without restoring fragile hard-coded tool counts to product copy unless they are generated.
3. Replace the legacy CorelDRAW parity matrix with the evidence levels in section 6. Record current-tree evidence separately from historical results.
4. Pin TraceDraw and update the relevant libcdr reference after reviewing their current revisions and licences.
5. Audit TraceDraw's `cdr-format`, `cdr`, PowerClip/clip-frame, live effects, typography, tracing, colour, and print implementations. Run the relevant upstream tests where practical.
6. Produce a module adoption table with one decision per candidate: adopt unchanged where feasible, adapt to VectorCraft, behaviour-only reference, defer with a reason, or reject with a reason.
7. Write the P1 implementation plan only after the adoption decisions are recorded.

Exit criterion: the current evidence baseline is honest, the external-reference ledger is reproducible, and every P1 candidate has an explicit reuse/adoption decision.

### P1. Professional Core Vector Workflow

This expands the existing V4.3/V4.4 work rather than restarting it.

Deliver the D1 core and the daily-use parts of D2 and D3: complete selection/node workflows, shaping, snapping, transforms, fills/outlines, core dockers, styles, pages, master layers, symbols, and native persistence.

Exit criterion: a user can create and revise a multi-object logo or icon document from scratch; all supported objects remain editable after save/reopen; command, UI, and independent export checks pass.

### P2. Professional Text and Multipage Layout

Complete the D3 domain and the remaining layout-dependent object semantics.

Exit criterion: create a multipage brochure with styled artistic and paragraph text, linked frames, master-page content, page numbering, and mixed vector/bitmap content. Editing and pagination remain stable, and independent PDF checks confirm the output.

### P3. Signature Effects and Image Workflows

Complete D4 and the remaining advanced fills and transparency semantics from D2.

Exit criterion: create a poster and a traced-image composition using PowerClip and multiple live effects; save and reopen the document; effect parameters and source content remain editable; export fallbacks are explicit and tested.

### P4. Colour, Prepress, and Production Output

Complete D5 and the remaining production-oriented D2 features.

Exit criterion: produce independently validated PDF/X and print output with correct page boxes, fonts, spot plates, overprint behaviour, output intents, marks, imposition, and data-driven content.

### P5. Automation, Performance, and Product Hardening

Complete the remaining D6 ecosystem features and production hardening across all domains.

Exit criterion: representative real projects pass sustained editing, automation, save/reopen, crash-recovery, export, and platform-install scenarios within measured performance budgets. Release evidence names the actual platforms and scenarios run.

## 6. Evidence Model and Acceptance Rules

### 6.1 Evidence Levels

Each capability row has a current level and links to its evidence:

| Level | Name | Minimum meaning |
|---|---|---|
| 0 | Absent | No supported implementation. |
| 1 | Model | Document semantics and native serialization exist. |
| 2 | Command | Shared engine command, undo/redo, and headless behaviour tests pass. |
| 3 | Interactive | Real tool/Property Bar/Docker interaction and UI tests pass. |
| 4 | Interoperable | Native reopen preserves editability; applicable independent import/export checks pass. |
| 5 | Production | The feature passes its real workflow, target-platform, and performance acceptance scenario. |

A menu entry, command registration, file extension, or self-round trip alone is never completion. Feature counts are derived from the matrix, not manually estimated percentages.

### 6.2 CDR/CMX Acceptance

For each supported feature and format generation, verify as applicable:

- Page, layer, object, and group structure.
- Geometry, transforms, parametric semantics, and editable nodes.
- Text content, styling, layout, and font behaviour.
- Fills, outlines, colours, transparency, effects, and bitmaps.
- Native and independent rendering/structure comparisons.
- Unknown or unsupported content preserved as residuals where possible, with a user-visible warning ledger.
- Hostile-input limits and no-panic behaviour.

A skipped oracle or sample test is unverified, not passing. Self-round trips are supplementary regression tests only. libcdr and TraceDraw may serve as independent readers/oracles; their own limitations must be recorded rather than treated as absolute ground truth.

### 6.3 Scenario Gates

Each phase defines at least one end-to-end document scenario, the exact commands and UI interactions exercised, the independent tools used, and the platforms on which the scenario ran. Test evidence is attached to the corresponding milestone record.

Performance budgets are measured against representative documents before they become release gates. No unmeasured performance number is presented as an existing result.

## 7. External Reference Assessment

### 7.1 Primary: TraceDraw

Repository: <https://github.com/tracedraw/tracedraw>

Why it matters:

- Created in October 2026 and actively developed; v0.43.0 was published on 2026-11-08.
- MIT OR Apache-2.0, compatible with CoralCraft's licensing when notices and attribution are preserved.
- Directly targets CorelDRAW-class vector, layout, print, CDR, effects, tracing, and automation workflows.
- Its `cdr-format` crate is independent of the application document model; `cdr` maps that format layer to TraceDraw documents.
- The repository contains approximately 700 Rust files, including 25 under `crates/cdr` and 7 under `crates/cdr-format` at the reviewed revision.
- Its feature matrix, CDR evidence log, behaviour documents, and acceptance criteria provide useful review structure.

Evidence links:

- Features: <https://github.com/tracedraw/tracedraw/blob/main/docs/features.md>
- CDR notes: <https://github.com/tracedraw/tracedraw/blob/main/docs/cdr-format.md>
- Acceptance: <https://github.com/tracedraw/tracedraw/blob/main/docs/acceptance.md>
- Clean-room record: <https://github.com/tracedraw/tracedraw/blob/main/CLEANROOM.md>
- Architecture: <https://github.com/tracedraw/tracedraw/blob/main/docs/architecture.md>

Cautions:

- Repository feature counts are self-reported and are not CoralCraft verification.
- The reviewed architecture document says some effects are baked while newer product documents describe live effects. Source and tests, not any single document, decide what is reusable.
- UI assets and presets are not copied wholesale. Every adapted code or asset contribution follows CoralCraft's attribution, clean-room, and asset-registration rules.

### 7.2 Format References

- libcdr: <https://github.com/LibreOffice/libcdr>. Active in November 2026, including CDR/CMX fuzz fixes and new format notes. MPL-2.0: read-only semantic reference and differential oracle; never copy line by line.
- gereoffy/libcdr format notes: <https://github.com/gereoffy/libcdr/blob/master/libcdr-fixes.md>. Useful for older-version text, arrowheads, fills, transparency, and fuzzing findings; the same MPL boundary applies to source.
- Photopea CDR specification: <https://github.com/photopea/CDR-specification>. Structural baseline, but old and incomplete for modern versions.
- Kaitai CDR definition: <https://formats.kaitai.io/coreldraw_cdr/>. Declarative structural reference, not complete application semantics.

### 7.3 Secondary References

- jpeg-nitro: <https://github.com/gereoffy/jpeg-nitro>. Useful performance and large-file viewer reference; not an editor architecture source.
- go-libcdr: <https://github.com/nobbs/go-libcdr>. Useful WASM sandbox and resource-limit design; it wraps libcdr rather than providing independent CDR semantics.
- Vector Studio in `HeyPuter/everything`: retain the existing interaction/geometry reference for effects and dimension tools, subject to its current licence and review.
- craft-corel: <https://github.com/ecomyseo/craft-corel>. A new TraceDraw-derived variant; track TraceDraw upstream rather than maintaining a second derivative reference unless it develops unique verified changes.
- CloneDRAW: <https://github.com/gladir/CloneDRAW>. Pascal educational/retro experiments; not a primary implementation reference.

Inkscape, UniConvertor, sK1, and other GPL/AGPL projects remain excluded as code sources under CoralCraft's licensing rules.

## 8. Documentation Changes After Specification Approval

After the operator reviews this specification:

1. Keep `docs/superpowers/plans/2026-10-21-vectorcraft-fork.md` as the migration plan and link this umbrella product roadmap from it.
2. Add the P0-P5 roadmap to `ROADMAP.md`, replacing the impression that V7 is the end of product development.
3. Replace the post-V4 queue in `docs/gaps.md` with P0 and its evidence/adoption tasks; derive later queues from phase-specific plans.
4. Rewrite `docs/target-app-parity.md` using D1-D6 and evidence levels 0-5, removing legacy 100% claims.
5. Add or update pinned references in `refs/REFS.md` and `scripts/bootstrap-refs.sh` only after licence and revision review.
6. Record the new direction and unresolved verification limits in `docs/ai-memory/03-current.md` and `docs/ai-memory/04-known-issues.md`.
7. Update README roadmap summaries only after the underlying roadmap and evidence matrix agree.

## 9. Immediate Next Step

Write a detailed implementation plan for P0 only. P0 must not become a disguised feature-development phase: it ends with an honest evidence matrix, tested reference audit, explicit adoption decisions, and a P1 plan grounded in those decisions.
