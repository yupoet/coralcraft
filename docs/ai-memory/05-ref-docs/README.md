# 05-ref-docs — External Reference & Domain Knowledge Notes

Index of external domain knowledge and review syntheses persisted for CoralCraft development:

1. [`01-artcraft-ecosystem.md`](01-artcraft-ecosystem.md) — Architecture, workspace layering (`L0..L7`), and quality gate conventions across the `storytold` ArtCraft family (`vectorcraft`, `designcraft`, `cadcraft`, `craft-libs`, `craft-fonts`).
2. [`02-cdr-open-source-ecosystem.md`](02-cdr-open-source-ecosystem.md) — Survey of open-source CDR/CMX projects (`libcdr`, Photopea `CDR-specification`, Kaitai Struct `coreldraw_cdr.ksy`, `morluto/rea`) and licensing boundaries.
3. [`03-cdr-format-primer.md`](03-cdr-format-primer.md) — Clean-room CDR format primer: three container generations (`WL`, bare `RIFF`/`RIFX`, X4+ `ZIP+RIFF`), ASCII version byte mapping table, coordinate system (`0.1 μm`), and chunk hierarchy.
4. [`04-m1-plan-four-way-review.md`](04-m1-plan-four-way-review.md) — Synthesis of the 5-way independent review (`GLM-5.3`, `Kimi K3`, `Grok 4.6`, `DeepSeek V4.1 Flash`, `Claude Opus 5.5`) across M1–M9.
5. [`05-vector-studio.md`](05-vector-studio.md) — HeyPuter/everything Vector Studio (`refs/vector-studio`, MIT): read-only behavioral/algorithmic reference for Phase V5 CorelDRAW features (envelope, liquify, Smart Fill regions, dimension tools); not code-portable (JS/Canvas vs our Rust/egui).
