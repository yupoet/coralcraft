# Reverse-Engineering Log Format (`docs/reverse/`)

All clean-room format discoveries for `.cdr`, `.cmx`, and `.cpl` are documented in `docs/reverse/` using the structured template below.

## Clean-Room Evidence Rules

1. **Allowed Sources**:
   - Public format specifications (Photopea CDR specification, Kaitai Struct `coreldraw_cdr.ksy`, RIFF / ZIP / ICC / PDF specifications).
   - Public architecture documentation andMPL-2.0 reference reading (`libcdr` chunk structural semantics — never copy code).
   - Synthetic byte sequences handcrafted from public specifications.
   - Operator-owned local test files inspected via `coralcraft-cli probe` (log only SHA-256 hashes and structural chunk observations; **never** commit `.cdr`/`.cmx` binaries into the repository).
2. **Forbidden**:
   - Never run, automate, or screenshot any Corel application.
   - Never include forbidden copyright strings or proprietary asset tables in the repository.

## Entry Template

```markdown
## REV-XXXX: <Short Title>
- **Date**: YYYY-MM-DD
- **Status**: Verified | Provisional | Superseded
- **Public Sources**:
  - <URL or specification section>
- **Evidence / Synthetic Test**:
  - <Test function link or operator file SHA-256>
- **Structural Findings**:
  - <Byte offsets, FourCC tags, endianness, invariants>
- **Implementation Impact**:
  - <Crate / module updated>
```

---

## REV-0001: CDR/CMX Three Container Generations, ASCII Version Map, and Opaque `LIST cmpr`/`stlt`
- **Date**: 2026-10-19
- **Status**: Verified
- **Public Sources**:
  - [Photopea CDR Format Specification](https://www.photopea.com/tuts/cdr-format-specification/)
  - [Kaitai Struct `coreldraw_cdr.ksy`](https://formats.kaitai.io/coreldraw_cdr/)
  - [LibreOffice `libcdr` Public Wiki](https://wiki.documentfoundation.org/DLP/Libraries/libcdr)
- **Evidence / Synthetic Test**:
  - [`crates/cdr/src/lib.rs`](../crates/cdr/src/lib.rs) unit test suite (22 tests + 10,000-iteration hostile fuzzer)
- **Structural Findings**:
  1. **Three Container Generations**:
     - **v1–v2 (`CdrWl`)**: Starts with `b"WL"` magic bytes (`0x57, 0x4C`); predates RIFF.
     - **v3–X3 (`Riff`, v300–v1300)**: Bare `RIFF` (little-endian) or `RIFX` (big-endian Mac CMX) container with form type `CDR*` or `CMX1`/`cdr6`.
     - **X4–2025+ (`ZipCdr`, v1400+)**: PKZIP container (`PK\x03\x04`). X4–X5 store the main RIFF stream at `content/riffData.cdr`; X6+ (`v1600+`) store a RIFF root at `content/root.dat` alongside `content/dataFileList.dat`.
  2. **Form-Type ASCII Version Mapping**:
     - `"CDR "` (`0x20` space) or `"CDR3"` → `300` (v3)
     - `'4'..'9'` → `400..900` (v4–v9)
     - `'A'..'R'` → `1000..2700` (`A`=1000 v10, `B`=1100 v11, `C`=1200 v12, `D`=1300 X3, `E`=1400 X4, `F`=1500 X5, `G`=1600 X6, `H`=1700 X7, `I`=1800 X8, `J`=1900 2017, `K`=2000 2018, `L`=2100 2019, `M`=2200 2020, `N`=2300 2021, `O`=2400 2022, `P`=2500 2023, `Q`=2600 2024, `R`=2700 2025).
     - When a 2-byte `vrsn` chunk is present, its `u16` value overrides the form-type version estimate.
  3. **Opaque `LIST cmpr` and `LIST stlt`**:
     - `LIST cmpr` begins with `compressedsize(u32)`, `uncompressedsize(u32)`, `blocksizessize(u32)`, raw header bytes, and two zlib streams (`CPng`).
     - `LIST stlt` carries a non-RIFF style table after its 4-byte list type.
     - Standard RIFF tree walkers must treat `LIST cmpr` and `LIST stlt` payloads as opaque during structural probing and only unpack `cmpr` in M3's zlib stage.
- **Implementation Impact**:
  - Implemented in [`crates/cdr/src/lib.rs`](../crates/cdr/src/lib.rs), [`crates/cdr/src/riff.rs`](../crates/cdr/src/riff.rs), [`crates/cdr/src/zip.rs`](../crates/cdr/src/zip.rs), and [`crates/cdr/src/error.rs`](../crates/cdr/src/error.rs).

---

## REV-0002: `LIST cmpr` Dual-Stream Zlib Layout, `mcfg` Page Size Offsets, `fild`/`outl` Styles, and `LIST page`/`layr`/`obj ` Object Tree
- **Date**: 2026-10-19
- **Status**: Verified
- **Public Sources**:
  - [Photopea CDR Format Specification (§LIST cmpr, §mcfg, §loda)](https://github.com/photopea/CDR-specification)
  - RFC 1950 (ZLIB Compressed Data Format Specification v3.3) & RFC 1951 (DEFLATE Compressed Data Format Specification v1.3)
- **Evidence / Synthetic Test**:
  - [`crates/cdr/src/lib.rs`](../crates/cdr/src/lib.rs) (`parses_compressed_cmpr_page_layer_and_objects_with_styles_and_fallback` and `parses_all_real_world_cdr_samples_when_present` passing 64/64 real-world `.cdr` samples)
- **Structural Findings**:
  1. **`LIST cmpr` Dual-Stream Layout**:
     - After the 4-byte `"cmpr"` list type, the header stores `c1: u32` (Part1 compressed size), `u1: u32` (Part1 uncompressed size), `c2: u32` (Part2 compressed size), and `u2: u32` (Part2 uncompressed size), followed by two `"CPng\x01\x00\x04\x00"` + RFC 1950 zlib streams.
     - `Part2` decompresses to an array of `u32` block sizes.
     - `Part1` decompresses to a RIFF-like chunk stream where each 8-byte chunk header's `u32` size field is an index into `Part2` (`block_sizes[idx]`).
  2. **`mcfg` Default Page Dimensions**:
     - Stores `(width, height)` in CDR coordinates (`sint2 / 1000` inch when `V < 600`; `sint4 / 254,000` inch = `0.1 µm` when `V >= 600`) at version-dependent byte offsets (`12` for `V >= 1300`, `4` for `900 <= V < 1300`, `28` for `600 <= V < 700`, `2` for `V < 600`).
  3. **`fild` and `outl` Style Records**:
     - `fild`: `fill_id: u32`, `fill_type: u16` (`0` = None, `1` = Uniform color, `2` = Fountain Fill Linear/Radial).
     - `outl`: `outl_id: u32`, `flags: u32` (bit `0x01` = `behind_fill`, bit `0x02` = `scale_with_object`, bits `2..4` = line cap, bits `4..6` = line join), `width: coord`, and color record.
  4. **Page, Layer, and Object Hierarchy**:
     - `LIST page` -> `LIST layr` -> `LIST obj ` (`bbox`, `trfd`, `loda`, `fild`, `outl`); unimplemented shape codes or effect chunks gracefully degrade to `ObjectKind::Unsupported` with `fallback_bounds` and `Residual` chunk preservation.
- **Implementation Impact**:
  - Implemented in [`crates/cdr/src/inflate.rs`](../crates/cdr/src/inflate.rs), [`crates/cdr/src/parse.rs`](../crates/cdr/src/parse.rs), and [`crates/cdr/src/zip.rs`](../crates/cdr/src/zip.rs).

