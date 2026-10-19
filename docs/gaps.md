# CoralCraft Prioritized Gap List

> For agent task selection: pick the first unchecked item from top to bottom. Update this file and [`ROADMAP.md`](../ROADMAP.md) as each item completes.
> Ordering principle: vertical slices first (probe -> document model -> render -> interactive tools), horizontal breadth second.

- [x] 1. **[M1] RIFF + ZIP Container Walker** (Task 9 / 9.5): Synthetic fixtures green + hostile-input error paths + 10,000-iteration mutation test.
- [x] 2. **[M1] `coralcraft-cdr::probe` Green** (Task 10): 22 unit tests green; first entry `REV-0001` in [`docs/reverse/log-format.md`](reverse/log-format.md).
- [x] 3. **[M1] CLI `probe` Subcommand** (Task 11): Human-readable + `--json` output, exit codes (`0/2/3/4/5`), 4 integration tests green.
- [x] 4. **[M1] `xtask cleanroom` / `assets` Gate Tests** (Task 12): Violation-driven unit tests and symlink traversal hardening.
- [x] 5. **[M1] Format & Workflow Parity Baseline Docs** (Task 13): [`docs/file-format-parity.md`](file-format-parity.md) and [`docs/target-app-parity.md`](target-app-parity.md).
- [x] 6. **[M2] Multi-Page Document Model (`coralcraft-doc`)**: `Document` / `Page` / document-level `Layer` (`Normal`, `MasterAllPages`, `MasterOddPages`, `MasterEvenPages`, `Desktop`) / `ObjectId` / `ObjectKind::Unsupported` lossless passthrough node + `coralcraft-geom` & `coralcraft-color`.
- [x] 7. **[M2] Native `.coralcraft` Format**: Deterministic Stored ZIP (`manifest.json` + `document.json` + `blobs/<sha256>.bin`) and single-file JSON, schema versioning, tamper detection, and 100-iteration random document round-trip property tests.
- [x] 8. **[M2] First Rendered Frame (`coralcraft-render`)**: Clean-room software rasterizer (4×4 MSAA, Linear/Radial Fountain, PowerClip mask stack, `behind_fill`, `scale_with_object`, Hairline) producing static page PNGs + `coralcraft-cli render` & `inspect-palette`.
- [ ] 9. **[M3] CDR Parser (Modern RIFF Band) & Vector Export**: `LIST cmpr` + ZIP Deflate decompression, `page`/`layr`/`obj ` parsing with `ObjectKind::Unsupported` fallback, and clean-room SVG 1.1 / PDF 1.4 export (`coralcraft-cli export`).
- [ ] 10. **[M4] `egui` Desktop Shell + Interactive Tool Slice**: Classic CorelDRAW workspace layout (`coralcraft-tools`, `coralcraft-engine`, `coralcraft-ui-egui`, `coralcraft-desktop`).
- [ ] 11. **[M5] MCP Server + CMX + CDR Writer Evaluation**.
