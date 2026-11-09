# CDR/CMX Verification Ledger

> Current-tree evidence for the post-2026-11-08 VectorCraft sync. Self-skipping tests are not passes.

| Check | Result | Evidence |
|---|---|---|
| `cdr2raw` availability | UNVERIFIED | not installed |
| `cmx2raw` availability | UNVERIFIED | not installed |
| Operator-owned sample corpus | UNVERIFIED | `refs/cdr-samples absent`; no sample files committed |
| `corel-import` writer oracle | UNVERIFIED | prerequisite tools absent; `cargo test -p corel-import --test oracle_and_samples -- --nocapture` not run on this tree |
| Real-sample structural import | UNVERIFIED | prerequisite sample corpus absent; `cargo test -p vectorcraft-engine cdr_and_cmx_engine_pipeline_passes_libcdr_oracle_and_real_samples -- --nocapture` not run on this tree |
| Engine CDR/CMX pipeline | UNVERIFIED | prerequisite sample corpus absent; `cargo test -p vectorcraft-engine cdr_and_cmx_engine_pipeline_passes_libcdr_oracle_and_real_samples -- --nocapture` not run on this tree |

## Interpretation

- `PASS` requires a real command that exercised the named oracle or corpus on the current tree.
- `UNVERIFIED` leaves sync follow-up S2 open and prevents P0 CDR certification.
- Historical 65/65 results remain historical and are not copied into the current-tree result column.
- Structural counts do not alone prove visual or editable fidelity; later phases add independent rendering and semantic checks.
