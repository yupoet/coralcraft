# CoralCraft Prioritized Gap List

> For agent task selection: pick the first unchecked item from top to bottom. Update this file and [`ROADMAP.md`](../ROADMAP.md) as each item completes.
> Ordering principle: vertical slices first (probe -> document model -> render -> interactive tools), horizontal breadth second.

- [x] 1. **[M1] RIFF + ZIP Container Walker** (Task 9 / 9.5): Synthetic fixtures green + hostile-input error paths + 10,000-iteration mutation test.
- [x] 2. **[M1] `coralcraft-cdr::probe` Green** (Task 10): 22 unit tests green; first entry `REV-0001` in [`docs/reverse/log-format.md`](reverse/log-format.md).
- [x] 3. **[M1] CLI `probe` Subcommand** (Task 11): Human-readable + `--json` output, exit codes (`0/2/3/4/5`), 4 integration tests green.
- [x] 4. **[M1] `xtask cleanroom` / `assets` Gate Tests** (Task 12): Violation-driven unit tests and symlink traversal hardening.
- [x] 5. **[M1] Format & Workflow Parity Baseline Docs** (Task 13): [`docs/file-format-parity.md`](file-format-parity.md) and [`docs/target-app-parity.md`](target-app-parity.md).
- [ ] 6. **[M2] Multi-Page Document Model (`coralcraft-doc`)**: `Document` / `Spread` / `Page` / document-level `Layer` + `Desktop` layer / `ObjectId` / `Object::Unsupported` lossless passthrough node.
- [ ] 7. **[M2] Native `.coralcraft` Format**: ZIP (`manifest.json` + `document.json` + `blobs/`) and JSON, schema versioning, graceful downgrade, and `proptest` round-trip property tests.
- [ ] 8. **[M2] First Rendered Frame (`coralcraft-render`)**: `tiny-skia` software rasterizer producing a static page PNG + `coralcraft-cli render`.
- [ ] 9. **[M3] CDR Parser (Modern RIFF Band)**: `LIST cmpr` decompression + `page`/`layr`/`obj ` parsing + pre-baked geometry fallback.
- [ ] 10. **[M4] `egui` Desktop Shell + Interactive Tool Slice**.
- [ ] 11. **[M5] MCP Server + CMX + CDR Writer Evaluation**.
