# P0 Evidence Baseline and TraceDraw Adoption Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish an honest post-sync evidence baseline, restore generated MCP documentation, separate VectorCraft and CorelDRAW parity documents, and make source/test-backed TraceDraw adoption decisions before further V4.3 feature work.

**Architecture:** Keep VectorCraft as the only application base. Treat CoralCraft's CorelDRAW matrix and VectorCraft's Illustrator matrix as separate documents. Audit TraceDraw at a pinned, licence-compatible revision and adopt only modules whose source, tests, and VectorCraft integration boundary are understood. CDR evidence distinguishes current-tree results, historical results, and unverified prerequisites.

**Tech Stack:** Rust workspace tests, `vectorcraft-mcp`, `corel-import`, GitHub CLI, Git, Bash, Markdown documentation, GitNexus change analysis, TraceDraw and libcdr reference checkouts.

---

## Mandatory rules for every task

- Work from the repository root. Do not stage or revert the existing unrelated changes in `AGENTS.md`, `CLAUDE.md`, or `.claude/`.
- Do not read anything under `refs/uniconvertor/`. Do not launch or automate proprietary Corel software.
- Do not commit CDR/CMX sample files, fonts, screenshots, or reference-repository contents. `refs/` remains local-only except `refs/REFS.md`.
- A missing CDR oracle or sample corpus is **UNVERIFIED**, never passing. Do not check S2 or P0 as complete while that condition remains.
- Before every commit, run:

```bash
node .gitnexus/run.cjs detect-changes --scope all --repo .
```

Expected: `partial: false` and `truncated: false`, with only the current task's files changed. If either is true, rerun the analysis and resolve the incomplete result before committing. Review the listed processes and risk; stop on unexpected HIGH/CRITICAL impact.

- Stage only the files named in the current task. Commit messages use English and contain no AI attribution.

## File map

- `docs/cdr-verification.md`: current CDR oracle/sample evidence and explicit unverified states.
- `crates/mcp/tests/docs_sync.rs`: generated MCP tool-list and protocol-surface regression tests.
- `docs/mcp.md`: current MCP protocol and generated tool reference.
- `docs/vectorcraft-target-app-parity.md`: upstream Illustrator parity assessment, kept separate from CoralDRAW measurement.
- `docs/target-app-parity.md`: CorelDRAW D1-D6 evidence matrix using levels 0-5.
- `docs/coreldraw-reference-adoption.md`: module-by-module TraceDraw adoption decisions.
- `docs/ai-memory/05-ref-docs/07-tracedraw.md`: pinned TraceDraw architecture, licence, tests, and caveats.
- `refs/REFS.md`, `scripts/bootstrap-refs.sh`: reproducible TraceDraw and libcdr pins.
- `ROADMAP.md`, `docs/gaps.md`, `README.md`, `docs/upstream-vectorcraft.md`, `docs/upstream-divergences.tsv`: roadmap and divergence records.
- `docs/ai-memory/01-commands.md`, `docs/ai-memory/03-current.md`, `docs/ai-memory/04-known-issues.md`: current commands, state, and limitations.

### Task 1: Record the approved umbrella design and P0 plan

**Files:**
- Create: `docs/superpowers/specs/2026-11-09-coreldraw-main-application-roadmap-design.md`
- Create: `docs/superpowers/plans/2026-11-09-p0-evidence-and-tracedraw-audit.md`

- [ ] **Step 1: Verify the working tree and the two documents**

Run:

```bash
git status --short
test -f docs/superpowers/specs/2026-11-09-coreldraw-main-application-roadmap-design.md
test -f docs/superpowers/plans/2026-11-09-p0-evidence-and-tracedraw-audit.md
git diff --check
```

Expected: both files exist and `git diff --check` prints nothing. Existing `AGENTS.md`, `CLAUDE.md`, and `.claude/` changes remain untouched.

- [ ] **Step 2: Run the mandatory change analysis and commit only the roadmap documents**

```bash
git add docs/superpowers/specs/2026-11-09-coreldraw-main-application-roadmap-design.md docs/superpowers/plans/2026-11-09-p0-evidence-and-tracedraw-audit.md
git commit -m "docs: define the CorelDRAW main application roadmap"
```

Expected: one commit containing only the specification and this P0 plan.

### Task 2: Record CDR oracle and real-sample evidence without hiding skips

**Files:**
- Create: `docs/cdr-verification.md`

- [ ] **Step 1: Probe the actual prerequisites**

Run:

```bash
for tool in cdr2raw cmx2raw; do
  if command -v "$tool" >/dev/null 2>&1; then
    printf '%s=%s\n' "$tool" "$(command -v "$tool")"
  else
    printf 'UNVERIFIED: %s is not installed\n' "$tool"
  fi
done
if [ -d refs/cdr-samples ]; then
  printf 'samples=%s\n' "$PWD/refs/cdr-samples"
else
  printf 'UNVERIFIED: refs/cdr-samples is absent\n'
fi
```

Expected on the current development machine: both tools and the sample directory are reported unverified. On an equipped machine, the output names the actual executables and sample directory. Do not create replacement samples or mark the checks optional-pass.

- [ ] **Step 2: Run the real checks only when every prerequisite exists**

If and only if Step 1 printed both tool paths and a sample path, run:

```bash
cargo test -p corel-import --test oracle_and_samples -- --nocapture
cargo test -p vectorcraft-engine cdr_and_cmx_engine_pipeline_passes_libcdr_oracle_and_real_samples -- --nocapture
```

Expected on an equipped machine: both commands exit 0; the first runs `write_cdr7_cdrx_and_cmx1_pass_libcdr_oracle_on_vectorcraft_document` and `real_world_cdr_samples_open_into_vectorcraft_doc_with_exact_counts`; the second runs the named engine test. Copy the test counts and platform into `docs/cdr-verification.md`.

If any prerequisite is absent, do not interpret the zero-exit self-skipping tests as evidence. Record S2 as open and P0 as blocked for real CDR certification; all other P0 tasks may proceed.

- [ ] **Step 3: Write the evidence ledger**

Create `docs/cdr-verification.md` with this structure and fill the result cells from Steps 1-2, using `PASS`, `FAIL`, or `UNVERIFIED` exactly:

```markdown
# CDR/CMX Verification Ledger

> Current-tree evidence for the post-2026-11-08 VectorCraft sync. Self-skipping tests are not passes.

| Check | Result | Evidence |
|---|---|---|
| `cdr2raw` availability | record the actual result | executable path or `not installed` |
| `cmx2raw` availability | record the actual result | executable path or `not installed` |
| Operator-owned sample corpus | record the actual result | absolute local path or `refs/cdr-samples absent`; no sample files committed |
| `corel-import` writer oracle | record the actual result | exact cargo command and test count, or `UNVERIFIED` |
| Real-sample structural import | record the actual result | exact cargo command, sample count, and platform, or `UNVERIFIED` |
| Engine CDR/CMX pipeline | record the actual result | exact cargo command and test count, or `UNVERIFIED` |

## Interpretation

- `PASS` requires a real command that exercised the named oracle or corpus on the current tree.
- `UNVERIFIED` leaves sync follow-up S2 open and prevents P0 CDR certification.
- Historical 65/65 results remain historical and are not copied into the current-tree result column.
- Structural counts do not alone prove visual or editable fidelity; later phases add independent rendering and semantic checks.
```

Expected: every result cell contains one of the three explicit states and a reproducible command or missing-prerequisite reason.

- [ ] **Step 4: Validate and commit the ledger**

```bash
git diff --check -- docs/cdr-verification.md
git add docs/cdr-verification.md
git commit -m "docs: record current CDR verification evidence"
```

Expected: the commit records actual evidence and does not mark missing checks as passing.

### Task 3: Restore generated MCP documentation on the synced registry

**Files:**
- Create: `crates/mcp/tests/docs_sync.rs`
- Modify: `docs/mcp.md`
- Modify: `docs/ai-memory/01-commands.md:62-64`

- [ ] **Step 1: Write the failing documentation-sync tests**

Create `crates/mcp/tests/docs_sync.rs` with this complete content:

```rust
//! Keeps `docs/mcp.md` in sync with the live MCP registry and protocol surface.
//!
//! Regenerate with `CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::fmt::Write as _;
use std::path::PathBuf;

use serde_json::Value;
use vectorcraft_mcp::{Headless, Server};

const BEGIN: &str = "<!-- BEGIN GENERATED TOOL LIST (cargo test -p vectorcraft-mcp --test docs_sync) -->";
const LEGACY_BEGIN: &str = "<!-- BEGIN GENERATED TOOL LIST (cargo test -p coralcraft-mcp --test docs_sync) -->";
const END: &str = "<!-- END GENERATED TOOL LIST -->";

fn live_tools() -> Vec<(String, String)> {
    let mut server = Server::new(Box::new(Headless::with_document()));
    let response = server
        .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
        .expect("tools/list returns a response");
    let value: Value = serde_json::from_str(&response).unwrap();
    let mut tools: Vec<(String, String)> = value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| {
            let name = tool["name"].as_str().unwrap().to_owned();
            let description = tool["description"]
                .as_str()
                .unwrap_or_default()
                .replace('|', "\\|")
                .replace('\n', " ");
            (name, description)
        })
        .collect();
    tools.sort();
    tools
}

fn render_block(tools: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push_str(BEGIN);
    out.push('\n');
    let _ = write!(out, "\n{} tools, sorted by name.\n\n| Tool | Description |\n|---|---|\n", tools.len());
    for (name, description) in tools {
        let _ = writeln!(out, "| `{name}` | {description} |");
    }
    out.push('\n');
    out.push_str(END);
    out
}

fn document() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/mcp.md");
    std::fs::read_to_string(path).expect("docs/mcp.md must exist").replace("\r\n", "\n")
}

#[test]
fn docs_mcp_md_tool_list_matches_live_registry() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/mcp.md");
    let doc = document();
    let start = doc
        .find(BEGIN)
        .or_else(|| doc.find(LEGACY_BEGIN))
        .expect("docs/mcp.md is missing the BEGIN marker");
    let end = doc.find(END).expect("docs/mcp.md is missing the END marker") + END.len();
    let expected = render_block(&live_tools());
    let current = &doc[start..end];
    if current != expected {
        if std::env::var("CORALCRAFT_BLESS").is_ok_and(|value| value == "1") {
            let updated = format!("{}{}{}", &doc[..start], expected, &doc[end..]);
            std::fs::write(&path, updated).unwrap();
        } else {
            panic!("docs/mcp.md is stale; run `CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`");
        }
    }
}

#[test]
fn docs_mcp_md_protocol_surface_is_current() {
    let doc = document();
    for required in [
        "2026-07-28",
        "2025-06-18",
        "resources/list",
        "resources/templates/list",
        "resources/read",
        "prompts/list",
        "prompts/get",
        "completion/complete",
        "logging/setLevel",
    ] {
        assert!(doc.contains(required), "docs/mcp.md does not document {required}");
    }
    assert!(!doc.contains("246 tools, sorted by name"), "legacy tool count remains");
}
```

- [ ] **Step 2: Run the tests and confirm the stale-document failure**

```bash
cargo test -p vectorcraft-mcp --test docs_sync -- --nocapture
```

Expected: FAIL. The tool-list test reports that `docs/mcp.md` is stale; the protocol test reports a missing current protocol item. This proves the new tests cover the legacy document.

- [ ] **Step 3: Generate the live tool list and update the protocol prose**

Run:

```bash
CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync docs_mcp_md_tool_list_matches_live_registry -- --exact --nocapture
```

Expected: the test rewrites only the generated block and exits 0.

In `docs/mcp.md`, replace the protocol and known-limitations sections with:

```markdown
## Protocol

- Requests: `initialize`, `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/templates/list`, `resources/read`, `prompts/list`, `prompts/get`, `completion/complete`, and `logging/setLevel`.
- Notifications (messages without an `id`, including `notifications/initialized`, `notifications/cancelled`, and `notifications/progress`) follow the server's notification rules; client notifications do not receive responses.
- Supported protocol versions are `2026-07-28`, `2025-06-18` (the fallback), `2025-03-26`, and `2024-11-05`.
- Every tool publishes an `inputSchema`. Tool failures and engine command rejections return tool results with `isError: true`.
- The `2026-07-28` revision adds completion metadata to list and read results. The server enforces line, batch, queue, and output limits covered by `crates/mcp/tests/protocol_props.rs`.

## Verification

- `cargo test -p vectorcraft-mcp` covers protocol behaviour, tool schemas, headless commands, and file I/O.
- `cargo test -p vectorcraft-mcp --test docs_sync` fails when this page's generated tool list or documented protocol surface is stale.
```

Delete the obsolete boolean-geometry and CoralCraft-only CDR-writer limitation paragraphs; they describe the retired pre-fork implementation, not the synced VectorCraft engine. Update the generated-block command in `docs/mcp.md` and `docs/ai-memory/01-commands.md` to:

```bash
CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync
```

- [ ] **Step 4: Run the focused and full MCP suites**

```bash
cargo test -p vectorcraft-mcp --test docs_sync
cargo test -p vectorcraft-mcp
```

Expected: both exit 0. The generated count comes from the live registry, not a hard-coded number.

- [ ] **Step 5: Commit the generator, document, and command correction**

```bash
git add crates/mcp/tests/docs_sync.rs docs/mcp.md docs/ai-memory/01-commands.md
git commit -m "test: keep MCP documentation in sync with the registry"
```

Expected: S4 is now eligible to be checked in the roadmap task; no legacy `coralcraft-mcp --test docs_sync` command remains in these files.

### Task 4: Separate VectorCraft's parity document and fold its technical notes

**Files:**
- Create: `docs/vectorcraft-target-app-parity.md`
- Modify: `docs/effects-parity.md`
- Modify: `docs/localization-parity.md`
- Modify: `docs/hardware-parity.md`
- Modify: `docs/type-parity.md`
- Modify: `docs/ui-parity.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/image-trace-quality.md`
- Modify: `docs/image-trace-adjustments.md`
- Modify: `docs/three-d.md`
- Modify: `docs/plugins.md`
- Modify: `docs/upstream-divergences.tsv`
- Modify: `docs/upstream-vectorcraft.md`

- [ ] **Step 1: Prove the separation is missing**

```bash
test -f docs/vectorcraft-target-app-parity.md
```

Expected: FAIL with exit 1. The upstream Illustrator assessment has no separate home, while upstream documents still link to the CoralCraft-owned filename.

- [ ] **Step 2: Preserve the upstream document under an explicit name**

```bash
cp refs/vectorcraft/docs/target-app-parity.md docs/vectorcraft-target-app-parity.md
```

Add this divergence row to `docs/upstream-divergences.tsv`, keeping the existing tab-separated format:

```text
docs/vectorcraft-target-app-parity.md	vectorcraft-owned	Upstream Illustrator parity assessment preserved separately from the CoralCraft matrix
```

Append to `docs/upstream-vectorcraft.md`:

```markdown
### P0 documentation separation (2026-11-09)

- Preserved the synced Illustrator assessment as `docs/vectorcraft-target-app-parity.md`.
- VectorCraft-derived `effects`, `localization`, `hardware`, `type`, `ui`, and `roadmap` documents link to that file.
- `docs/target-app-parity.md` is reserved for CoralCraft's CorelDRAW D1-D6 evidence matrix.
```

- [ ] **Step 3: Retarget only VectorCraft-derived links**

Run this exact script; it deliberately excludes CoralCraft's `docs/gaps.md`:

```bash
python3 - <<'PY'
from pathlib import Path

for name in [
    "effects-parity.md",
    "localization-parity.md",
    "hardware-parity.md",
    "type-parity.md",
    "ui-parity.md",
    "roadmap.md",
]:
    path = Path("docs") / name
    text = path.read_text()
    updated = text.replace("(target-app-parity.md", "(vectorcraft-target-app-parity.md")
    if updated == text:
        raise SystemExit(f"no upstream parity link found in {path}")
    path.write_text(updated)
PY
```

- [ ] **Step 4: Fold the formats, tracing, 3D, and plug-in documentation into CoralCraft's roadmap**

Review `docs/file-format-parity.md` against the current `FORMATS` matrix. Keep its post-sync PCX, Pixar, and JPEG 2000 entries; no format claim needs to change unless a registry comparison fails.

Add this note near the top of both `docs/image-trace-quality.md` and `docs/image-trace-adjustments.md`:

```markdown
> CoralCraft: This VectorCraft-derived Image Trace documentation describes the implementation base for the P3 tracing workflow. It is not evidence of PowerTRACE parity; see `coreldraw-reference-adoption.md` and `target-app-parity.md`.
```

Add this note near the top of `docs/three-d.md`:

```markdown
> CoralCraft: These VectorCraft 3D effects are reusable infrastructure, not evidence of CorelDRAW Extrude parity. Corel-specific live-effect acceptance is handled in P3.
```

Add this note near the top of `docs/plugins.md`:

```markdown
> CoralCraft: The WebAssembly plug-in sandbox is the D6 scripting/extension base. It is not a VBA or COM compatibility layer.
```

Append the S5 outcome to the P0 entry in `docs/upstream-vectorcraft.md`:

```markdown
- Reviewed formats, Image Trace, 3D, and plug-in documentation after the sync. Kept the measured format matrix, identified Image Trace as the P3 base, marked 3D as infrastructure rather than CorelDRAW Extrude evidence, and scoped the WASM plug-in API to D6 rather than VBA/COM.
```

- [ ] **Step 5: Verify link ownership and Markdown hygiene**

```bash
rg -n '\(target-app-parity\.md' docs/effects-parity.md docs/localization-parity.md docs/hardware-parity.md docs/type-parity.md docs/ui-parity.md docs/roadmap.md || true
rg -n '\(vectorcraft-target-app-parity\.md' docs/effects-parity.md docs/localization-parity.md docs/hardware-parity.md docs/type-parity.md docs/ui-parity.md docs/roadmap.md
rg -n 'PCX|Pixar|JPEG 2000' docs/file-format-parity.md
git diff --check
```

Expected: the first search prints nothing; the second shows the retargeted links; `docs/gaps.md` still links to CoralCraft's `target-app-parity.md`; the format search finds the synced additions.

- [ ] **Step 6: Commit the separation and technical-note fold**

```bash
git add docs/vectorcraft-target-app-parity.md docs/effects-parity.md docs/localization-parity.md docs/hardware-parity.md docs/type-parity.md docs/ui-parity.md docs/roadmap.md docs/image-trace-quality.md docs/image-trace-adjustments.md docs/three-d.md docs/plugins.md docs/upstream-divergences.tsv docs/upstream-vectorcraft.md
git commit -m "docs: separate parity records and fold upstream notes"
```

Expected: S5 is resolved without dropping upstream technical history or presenting Illustrator facilities as CorelDRAW parity evidence.

### Task 5: Pin TraceDraw and refresh libcdr reproducibly

**Files:**
- Modify: `refs/REFS.md`
- Modify: `scripts/bootstrap-refs.sh`

- [ ] **Step 1: Verify the exact audited revisions and licences**

```bash
gh api repos/tracedraw/tracedraw/commits/aac8563a5b4eb8399d86d285f8aa69a6ffc0deee --jq '.sha'
gh api repos/tracedraw/tracedraw/contents/Cargo.toml?ref=aac8563a5b4eb8399d86d285f8aa69a6ffc0deee --jq '.content' | base64 --decode | rg '^license = '
gh api repos/LibreOffice/libcdr/commits/f4cd1f9fa64a312190357bfa33ca80c51512bb2b --jq '.sha'
```

Expected: both full SHAs match the command arguments; TraceDraw reports `license = "MIT OR Apache-2.0"`. libcdr remains MPL-2.0 and read-only as a semantic reference.

- [ ] **Step 2: Add and update the bootstrap pins**

In `scripts/bootstrap-refs.sh`, add this entry after `vectorcraft`:

```bash
  "tracedraw|https://github.com/tracedraw/tracedraw.git|aac8563a5b4eb8399d86d285f8aa69a6ffc0deee"
```

Replace the `libcdr` entry with:

```bash
  "libcdr|https://github.com/LibreOffice/libcdr.git|f4cd1f9fa64a312190357bfa33ca80c51512bb2b"
```

In `refs/REFS.md` section A, add:

```markdown
| `tracedraw/` | MIT OR Apache-2.0 | **Primary CorelDRAW-extension reference**: independent CDR format layer, CDR reader/writer, PowerClip, live effects, typography, tracing, colour, print, and acceptance evidence | Source may be adapted with notices after module review; never import assets wholesale; record adaptations in `NOTICE` and `docs/upstream-vectorcraft.md`; feature claims require local test evidence |
```

In section D, add:

```markdown
| `tracedraw/` | `github.com/tracedraw/tracedraw` | `aac8563a5b4eb8399d86d285f8aa69a6ffc0deee` |
```

Replace the libcdr pin with `f4cd1f9fa64a312190357bfa33ca80c51512bb2b` and update the section-D note to record that TraceDraw was added and libcdr refreshed on 2026-11-09.

- [ ] **Step 3: Restore only the new/changed local references**

```bash
bash -n scripts/bootstrap-refs.sh
./scripts/bootstrap-refs.sh
git -C refs/libcdr fetch --depth 1 origin f4cd1f9fa64a312190357bfa33ca80c51512bb2b
git -C refs/libcdr checkout --detach f4cd1f9fa64a312190357bfa33ca80c51512bb2b
```

Expected: existing references are skipped, `tracedraw` is restored at the pin, and `libcdr` moves to the refreshed pin. If TraceDraw already exists, do not delete it; fetch and checkout the same pin instead.

- [ ] **Step 4: Verify exact checkouts and no tracked reference content**

```bash
test "$(git -C refs/tracedraw rev-parse HEAD)" = aac8563a5b4eb8399d86d285f8aa69a6ffc0deee
test "$(git -C refs/libcdr rev-parse HEAD)" = f4cd1f9fa64a312190357bfa33ca80c51512bb2b
git status --short
```

Expected: both tests exit 0 and `git status` shows only the two tracked reference files plus earlier task files already committed. No `refs/tracedraw` contents appear as untracked repository files.

- [ ] **Step 5: Commit the reference ledger**

```bash
git add refs/REFS.md scripts/bootstrap-refs.sh
git commit -m "chore: pin TraceDraw and refresh libcdr references"
```

Expected: a fresh workspace can reconstruct the same audited references.

### Task 6: Run TraceDraw tests and write the source-backed audit

**Files:**
- Create: `docs/ai-memory/05-ref-docs/07-tracedraw.md`

- [ ] **Step 1: Review the pinned architecture and evidence documents before reading implementation details**

Read these local files:

```text
refs/tracedraw/AGENTS.md
refs/tracedraw/CLEANROOM.md
refs/tracedraw/docs/architecture.md
refs/tracedraw/docs/features.md
refs/tracedraw/docs/blueprint-gaps.md
refs/tracedraw/docs/cdr-format.md
refs/tracedraw/docs/acceptance.md
refs/tracedraw/crates/cdr-format/Cargo.toml
refs/tracedraw/crates/cdr/Cargo.toml
```

Expected: record any local instructions and all stated limitations. Treat `docs/architecture.md`'s baked-effect description and newer live-effect claims as a documentation conflict to resolve from source/tests, not as proof either way.

- [ ] **Step 2: Run the relevant pinned test suites**

```bash
cargo test --manifest-path refs/tracedraw/Cargo.toml -p tracedraw-cdr-format
cargo test --manifest-path refs/tracedraw/Cargo.toml -p tracedraw-cdr
cargo test --manifest-path refs/tracedraw/Cargo.toml -p tracedraw-core --lib
cargo test --manifest-path refs/tracedraw/Cargo.toml -p tracedraw --lib
```

Expected: record each exact command, platform, exit status, and test count in the audit. A failing suite blocks adoption of its owning module; it does not block a behaviour-only reference decision for that module. Do not edit TraceDraw to make its tests pass inside this P0 task.

- [ ] **Step 3: Inspect the adoption boundaries**

Read the module roots and relevant implementations under:

```text
refs/tracedraw/crates/cdr-format/src/
refs/tracedraw/crates/cdr/src/
refs/tracedraw/crates/core/src/
refs/tracedraw/crates/render/src/
refs/tracedraw/crates/text/src/
refs/tracedraw/apps/tracedraw/src/ui/
```

Identify the document types, coordinate system, effect representation, clip-frame behaviour, text engine, colour/print ownership, and test files for each candidate. Confirm that `cdr-format` has no dependency on TraceDraw's application document model, while `cdr` maps into that model.

- [ ] **Step 4: Write the audit**

Create `docs/ai-memory/05-ref-docs/07-tracedraw.md` containing:

- The full pin, repository URL, MIT OR Apache-2.0 licence, attribution requirement, and review date.
- The dependency summary: `core` owns documents/commands; `cdr-format` is document-model independent; `cdr` maps to `core`; `render` uses tiny-skia; CoralCraft uses VectorCraft documents, commands, and vello_cpu.
- The four test commands from Step 2 and their verbatim result summaries.
- The source files inspected for CDR, clip frames, live effects, typography, tracing, colour, print, and Property Bar behaviour.
- The documentation conflict about effects and the source/test conclusion.
- A statement that TraceDraw feature counts are upstream self-reports, not CoralCraft verification.
- A list of open limitations from `blueprint-gaps.md` that affect adoption, including CDR effects/lenses gaps and any failed local tests.

- [ ] **Step 5: Validate and commit the audit**

```bash
git diff --check -- docs/ai-memory/05-ref-docs/07-tracedraw.md
git add docs/ai-memory/05-ref-docs/07-tracedraw.md
git commit -m "docs: audit the pinned TraceDraw implementation"
```

Expected: another engineer can reproduce the checkout, tests, and conclusions without relying on TraceDraw's README claims.

### Task 7: Record module adoption decisions and replace the parity matrix

**Files:**
- Create: `docs/coreldraw-reference-adoption.md`
- Modify: `docs/target-app-parity.md`

- [ ] **Step 1: Write the adoption table**

Create `docs/coreldraw-reference-adoption.md` with one row per module. Start with these architecture constraints and change a decision only when Task 6 provides explicit source/test evidence for the change:

| Module | Initial decision | Required integration boundary |
|---|---|---|
| TraceDraw `cdr-format` containers/RIFF/redirects | Adapt after tests | Keep `coralcraft-cdr` safe-Rust errors/budgets; port missing semantics with notices; compare against libcdr and current CDR tests |
| TraceDraw `wl` and old-version readers | Adapt per version | Add synthetic and operator-owned-sample evidence; do not weaken existing hostile-input limits |
| TraceDraw `cdr` document mapping | Behaviour and selective adaptation | Map through `corel-import` into `vectorcraft-doc`; do not introduce TraceDraw `Document` into CoralCraft |
| Text records and old code pages | Selective adaptation | Reuse VectorCraft text shaping; add only missing CDR decoding semantics and tests |
| Fills, outlines, arrowheads, transparency | Selective adaptation | Reuse VectorCraft appearance/render types; preserve editable semantics and independent export checks |
| PowerClip / clip frame | Adapt behaviour and tests | Reuse VectorCraft clipping/groups/commands; implement full container editing rather than import-only clipping |
| Live effects | Adapt behaviour and tests per effect | Reuse VectorCraft live-effect/effect-stack infrastructure; do not replace the renderer |
| Typography | VectorCraft first | TraceDraw fills only measured Corel-specific gaps; no second text engine |
| Bitmap tracing | VectorCraft first | Use `vectorcraft-trace`; TraceDraw is a preset/behaviour reference only |
| Colour management and PDF/X | VectorCraft first | Reuse VectorCraft ICC/PDF/print; adopt only evidenced Corel-specific behaviour |
| Property Bar and dockers | Behaviour reference | Use `vectorcraft-tools`, `tool.select`, `tool.setOption`, and shared engine commands |
| MCP and scripting | Behaviour reference | Keep VectorCraft command registry/MCP/plugins; no second automation engine |

Each row must link to the Task 6 audit section that supports its decision. Every adopted-code candidate names its licence/notice obligation and the tests required in the receiving phase. This table is the P0 adoption decision record, not permission to land all modules in P0.

- [ ] **Step 2: Replace legacy 100% claims with the D1-D6 evidence matrix**

Rewrite `docs/target-app-parity.md` with:

- A link to the umbrella specification and `docs/vectorcraft-target-app-parity.md`, explaining that the latter is a different target.
- The level definitions copied exactly from the umbrella specification.
- Separate `Current-tree evidence` and `Historical evidence` columns.
- Rows for tool mapping, selection/node editing, shaping, pages/master layers, fills/outlines, text, PowerClip, live effects, tracing, colour/prepress, CDR/CMX, VectorCraft formats, MCP/CLI, and scripting.
- Links to `docs/cdr-verification.md`, `docs/coreldraw-reference-adoption.md`, relevant source tests, and `docs/gaps.md`.
- No aggregate percentage and no claim that a whole domain is complete when only one layer exists.

Use these conservative current-tree levels unless a cited test demonstrates a higher level:

| Capability | Level | Current-tree evidence basis |
|---|---:|---|
| CorelDRAW tool mapping and Property Bar | 2 | Shared VectorCraft commands exist; V4.3 mapping is open |
| Selection and node interaction | 3 | Synced VectorCraft interactive tools and tests |
| Shaping and path operations | 3 | VectorCraft command/UI coverage; Corel-specific mapping remains open |
| Pages, master layers, and page navigator | 4 | V2.2 native persistence plus V3 command/UI tests |
| Fills, outlines, palettes, and transparency | 3 | VectorCraft UI/render plus CoralCraft palette bar; domain-specific interoperability is incomplete |
| Artistic/paragraph text and typography | 3 | Synced VectorCraft text engine and UI; Corel CDR semantic coverage is incomplete |
| PowerClip container workflow | 2 | Import/render/native residual support exists; full edit/extract/lock workflow is open |
| CorelDRAW live-effect workflow | 2 | VectorCraft effects exist; CorelDRAW effect mapping and handles are open |
| Bitmap tracing | 3 | Synced VectorCraft tracing and UI tests |
| Colour management and prepress | 3 | Synced VectorCraft ICC/PDF/print infrastructure; Corel workflow acceptance is open |
| CDR/CMX read/write | 2 | Engine read/write commands and regression tests exist; current independent oracle state is recorded separately in `docs/cdr-verification.md`; interactive and independent evidence must raise this conservatively |
| VectorCraft native/file compatibility | 4 | V2.1-V2.2 compatibility and lossless extension tests |
| MCP and CLI | 2 | Synced headless command/protocol tests; regenerated docs prove registry coverage |
| Sandboxed scripting/plugins | 2 | Synced VectorCraft infrastructure; CorelDRAW object-model coverage is open |

Historical results may mention V1.4 and the 65/65 historical sample run only with an explicit link to the older milestone; they must not upgrade current-tree levels.

- [ ] **Step 3: Check consistency and commit**

```bash
rg -n '100%|coralcraft-mcp --test docs_sync' docs/target-app-parity.md docs/coreldraw-reference-adoption.md || true
git diff --check
```

Expected: the search prints nothing. Every level has an evidence link, and CDR level 4 is not claimed while `docs/cdr-verification.md` contains `UNVERIFIED`.

```bash
git add docs/coreldraw-reference-adoption.md docs/target-app-parity.md
git commit -m "docs: adopt evidence-based CorelDRAW capability tracking"
```

Expected: the old 100% matrix is gone and every module has an explicit adoption boundary.

### Task 8: Synchronize the public roadmap, task queue, README, and memory

**Files:**
- Modify: `ROADMAP.md`
- Modify: `docs/gaps.md`
- Modify: `docs/superpowers/plans/2026-10-21-vectorcraft-fork.md`
- Modify: `README.md`
- Modify: `docs/ai-memory/03-current.md`
- Modify: `docs/ai-memory/04-known-issues.md`

- [ ] **Step 1: Link the umbrella roadmap from the migration plan and root roadmap**

In `docs/superpowers/plans/2026-10-21-vectorcraft-fork.md`, add near its status block:

```markdown
> Product roadmap beyond the migration: `docs/superpowers/specs/2026-11-09-coreldraw-main-application-roadmap-design.md`.
> Execute P0 before further large V4.3 work: `docs/superpowers/plans/2026-11-09-p0-evidence-and-tracedraw-audit.md`.
```

In `ROADMAP.md`, keep V0-V4.2 and the full sync in the completed migration record. Add a new active section containing the P0-P5 phase names and exit criteria from the umbrella specification, with P0 linked to this implementation plan. State explicitly that V7 is migration hygiene, not completion of the CorelDRAW product target.

- [ ] **Step 2: Replace the active queue in `docs/gaps.md`**

Keep completed V0-V4.2 entries checked. Put P0 tasks first in this order:

```markdown
- [ ] P0.1 Record current CDR oracle/sample evidence; S2 remains open while any prerequisite is absent.
- [ ] P0.2 Restore generated MCP docs on `vectorcraft-mcp` (S4).
- [ ] P0.3 Separate VectorCraft and CorelDRAW parity records and fold relevant upstream doc updates (S5).
- [ ] P0.4 Pin TraceDraw and refresh libcdr.
- [ ] P0.5 Run pinned TraceDraw tests and record the source audit.
- [ ] P0.6 Record module adoption decisions and the D1-D6 evidence matrix.
- [ ] P0.7 Synchronize roadmap, README, memory, and the P1 handoff.
- [ ] V4.3 Resume CorelDRAW tool mapping and the context-sensitive Property Bar after P0 adoption decisions.
```

When Tasks 2-7 and this task are complete, check only the entries whose evidence exists. P0.1 stays unchecked if `docs/cdr-verification.md` contains `UNVERIFIED`. Keep S3 and V7.1 open as continuous hygiene; do not schedule V6 as a late MCP-only phase.

- [ ] **Step 3: Update README's remaining-roadmap summary**

Replace the V4.3-V7.1-only summary with:

```markdown
**Active product roadmap (P0–P5; see [`ROADMAP.md`](ROADMAP.md)):**

- **P0:** Evidence baseline, current CDR verification status, and TraceDraw adoption audit.
- **P1:** Professional core vector workflow, expanding V4.3/V4.4 after adoption decisions.
- **P2:** Professional text and multipage layout.
- **P3:** PowerClip, live effects, tracing, and image workflows.
- **P4:** Colour management, prepress, and production output.
- **P5:** Automation, performance, and product hardening.

CDR/CMX interoperability, MCP/CLI, upstream syncs, and legacy retirement advance throughout these phases. The migration's V7 is not the end of the CorelDRAW product roadmap.
```

Do not change the README's verified-current-feature claims unless their evidence changed in Tasks 2-7. Add a link to `docs/cdr-verification.md` beside the CDR sample claim and distinguish historical from current-tree verification if the current ledger is unverified.

- [ ] **Step 4: Update AI memory**

At the top of `docs/ai-memory/03-current.md`, record:

- The approved umbrella specification and P0 plan paths.
- P0 as the active direction before further large V4.3 work.
- TraceDraw and libcdr full pins.
- The actual Task 2-7 test results and adoption-document paths.
- The next unchecked P0 item, or the P1 handoff when all required evidence exists.

In `docs/ai-memory/04-known-issues.md`, add:

- `docs/vectorcraft-target-app-parity.md` is upstream Illustrator evidence; `docs/target-app-parity.md` is CoralDRAW evidence.
- The MCP docs generator is `CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`.
- Current CDR oracle/sample prerequisites and the exact unverified state from `docs/cdr-verification.md`.
- TraceDraw's baked/live-effect documentation conflict and where the source/test conclusion is recorded.

- [ ] **Step 5: Refresh measured statistics and align the roadmap**

Run:

```bash
cargo xtask stats
```

Expected: the command exits 0 and prints current per-crate file, line, and test counts. Update the post-sync metrics in `ROADMAP.md` from this output, including the current test count and desktop-layer totals. Record the command and date beside the metrics. Do not reintroduce a hard-coded tool count into README; keep `the complete VectorCraft toolbox` unless a generated catalog measurement is added. Copy the exact stats command and resulting totals into `docs/ai-memory/03-current.md`.

- [ ] **Step 6: Run documentation consistency checks**

```bash
rg -n 'coralcraft-mcp --test docs_sync|Remaining Phase V roadmap|100%.*delivered' README.md ROADMAP.md docs/gaps.md docs/target-app-parity.md docs/ai-memory/01-commands.md docs/ai-memory/03-current.md docs/ai-memory/04-known-issues.md || true
git diff --check
```

Expected: no stale generator command, old remaining-roadmap heading, or delivered-100% wording is found.

- [ ] **Step 7: Commit the synchronized roadmap**

```bash
git add ROADMAP.md docs/gaps.md docs/superpowers/plans/2026-10-21-vectorcraft-fork.md README.md docs/ai-memory/03-current.md docs/ai-memory/04-known-issues.md
git commit -m "docs: activate the evidence and adoption roadmap"
```

Expected: all public entry points select P0 before V4.3 and report the same verification state.

### Task 9: Run the complete P0 gate and prepare the P1 handoff

**Files:**
- Modify only if verification finds stale generated output: `docs/mcp.md`
- Create after the adoption review: `docs/superpowers/plans/2026-11-09-p1-core-vector-workflow.md`

- [ ] **Step 1: Run focused checks and repository hygiene gates**

```bash
cargo test -p vectorcraft-mcp --test docs_sync
cargo test -p corel-import --test oracle_and_samples -- --nocapture
bash -n scripts/bootstrap-refs.sh
git diff --check
cargo fmt --all -- --check
```

Expected: all commands exit 0. The CDR command is only a regression check here; if its prerequisites are absent, it does not change the ledger's `UNVERIFIED` state or close S2.

- [ ] **Step 2: Run the full project CI gate**

```bash
cargo xtask ci
```

Expected: exit 0. If a known upstream/platform-specific failure occurs, reproduce it against the documented baseline and record the exact result; do not claim a clean P0 gate or alter unrelated code.

- [ ] **Step 3: Audit the final documentation and reference state**

```bash
test "$(git -C refs/tracedraw rev-parse HEAD)" = aac8563a5b4eb8399d86d285f8aa69a6ffc0deee
test "$(git -C refs/libcdr rev-parse HEAD)" = f4cd1f9fa64a312190357bfa33ca80c51512bb2b
rg -n 'TBD|TODO|implement later|coralcraft-mcp --test docs_sync|100%.*delivered' docs/cdr-verification.md docs/target-app-parity.md docs/coreldraw-reference-adoption.md docs/ai-memory/05-ref-docs/07-tracedraw.md README.md ROADMAP.md docs/gaps.md || true
git status --short
```

Expected: both pins match; the search prints nothing; only intentional task files and the pre-existing unrelated changes are present.

- [ ] **Step 4: Review the P0 exit criterion explicitly**

Confirm all of the following from the files:

- Current-tree CDR results are distinguished from historical results and `UNVERIFIED` states.
- MCP documentation is generated and its sync test passes.
- VectorCraft and CorelDRAW parity records are separate.
- TraceDraw test results and source audit exist.
- Every P1 candidate has an adoption decision in `docs/coreldraw-reference-adoption.md`.
- Roadmap, gaps, README, and memory select the same next task.

If any prerequisite for real CDR certification is absent, report **P0 documentation/audit complete, S2 real CDR certification open** rather than marking all of P0 complete.

- [ ] **Step 5: Write the P1 implementation plan from the completed adoption record**

Invoke `superpowers:writing-plans` with:

```text
Write docs/superpowers/plans/2026-11-09-p1-core-vector-workflow.md from the approved umbrella specification and the completed P0 evidence/adoption documents. Preserve VectorCraft as the only application base. Expand the existing V4.3/V4.4 work into small vertical slices through model, commands, UI, native persistence, CDR mapping, MCP, and tests. Use the decisions in docs/coreldraw-reference-adoption.md and do not restart CDR certification as a self-round-trip exercise.
```

The P1 plan must name concrete files, tests, commands, and commits derived from the actual P0 decisions. It is a separate implementation plan; do not treat its existence as P1 completion.

- [ ] **Step 6: Run final graph analysis and commit any verification/handoff changes**

```bash
node .gitnexus/run.cjs detect-changes --scope all --repo .
```

Expected: `partial: false`, `truncated: false`, and no unexpected affected processes. If Step 5 created the P1 plan, commit it with:

```bash
git add docs/superpowers/plans/2026-11-09-p1-core-vector-workflow.md
git commit -m "docs: plan the professional core vector workflow"
```

If no file changed in this task, do not create an empty commit. Report the exact P0/S2 state and the P1 plan path.
