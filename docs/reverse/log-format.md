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

---

## REV-0003: CDR v7 (`version = 700`) `fild`, `outl`, `trfd`, and `mcfg` Binary Layouts
- **Date**: 2026-10-20
- **Status**: Verified
- **Public Sources**:
  - [Photopea CDR Format Specification (§mcfg, §fild, §outl, §trfd)](https://github.com/photopea/CDR-specification)
  - [Kaitai Struct `coreldraw_cdr.ksy`](https://formats.kaitai.io/coreldraw_cdr/)
  - [LibreOffice `libcdr` Public Wiki](https://wiki.documentfoundation.org/DLP/Libraries/libcdr) (`cdr2raw` differential oracle)
- **Evidence / Synthetic Test**:
  - [`crates/cdr/tests/oracle_cdr2raw.rs`](../crates/cdr/tests/oracle_cdr2raw.rs) (`write_cdr7_and_write_cmx1_pass_libcdr_cdr2raw_and_cmx2raw_oracle_r7_1`)
- **Structural Findings**:
  1. **`mcfg` v700..v1200 Layout**:
     - Stores `(page_width: i32, page_height: i32)` in `0.1 µm` units (`1 inch = 254,000`, `1 pt = 3,527.7778`) at byte offset `4..12` (`16` bytes total).
     - Placed inside a top-level `LIST doc ` before `LIST page` so page dimensions apply globally.
  2. **`fild` v700..v1200 Layout**:
     - `fill_id: u32` (`0..4`), `fill_type: u16` (`4..6`: `0` = None, `1` = Uniform, `2` = Fountain), 2-byte pad (`6..8`), followed by a 12-byte color record at offset `8..20`: `color_model: u16` (`5` = RGB with `[B, G, R, 0]` at `16..20`; `2`/`3` = CMYK with `[C, M, Y, K]` in `0..=100` at `16..20`).
  3. **`outl` v700..v1200 Layout**:
     - `outl_id: u32` (`0..4`), `line_type: u16` (`4..6`), `caps: u16` (`6..8`: `0` = Butt, `1` = Round, `2` = Square), `join: u16` (`8..10`: `0` = Miter, `1` = Round, `2` = Bevel), 2-byte pad (`10..12`), `line_width: i32` in `0.1 µm` (`12..16`), `stretch: u16` (`16..18`, `100` = 100%), `angle: i16` (`18..20`), 28 bytes of affine/reserved fields (`20..48`), color record at `48..60`, and `dash_count: u16` + dash segments at `76..`.
  4. **`trfd` v600..v1200 Layout**:
     - Multi-arg chunk table (`num_args = 1`, `offsets_offset = 16`, `arg_offset[0] = 20`); arg payload begins with an 8-byte subheader followed by 6 IEEE-754 little-endian `f64` values `(v0, v1, x0, v3, v4, y0)` at payload+8 (`48` bytes).
     - `x0` and `y0` are expressed in `0.1 µm` units (`1 pt = 254,000 / 72`).
- **Implementation Impact**:
  - Implemented in [`crates/cdr/src/parse.rs`](../crates/cdr/src/parse.rs) (`write_cdr7`, `parse_fild_chunk`, `parse_outl_chunk`, `parse_trfd_chunk`).

---

## REV-0004: CDR v7 (`version = 700`) `loda`/`lobj` Multi-Argument Table and Shape Payloads
- **Date**: 2026-10-20
- **Status**: Verified
- **Public Sources**:
  - [Photopea CDR Format Specification (§loda, §Polygon, §Spline)](https://github.com/photopea/CDR-specification)
  - [Kaitai Struct `coreldraw_cdr.ksy`](https://formats.kaitai.io/coreldraw_cdr/)
- **Evidence / Synthetic Test**:
  - [`crates/cdr/tests/oracle_cdr2raw.rs`](../crates/cdr/tests/oracle_cdr2raw.rs) (`write_cdr7_and_write_cmx1_pass_libcdr_cdr2raw_and_cmx2raw_oracle_r7_1`, `write_cdr7_and_write_cmx1_return_err_instead_of_truncating_oversized_paths_r7_1`)
- **Structural Findings**:
  1. **`loda` Multi-Argument Header**:
     - `chunk_length: u32` (`0..4`), `num_args: u32` (`4..8`), `offsets_offset: u32` (`8..12`, typically `0x18`), `types_offset: u32` (`12..16`), `shape_type: u32` (`16..20`: `1` = Rectangle, `2` = Ellipse, `3` = Line/Curve, `0x14` = Polygon), `unknown: u32` (`20..24`).
     - At `offsets_offset`: `(num_args + 1)` `u32` offsets, followed at `types_offset` by `num_args` `u32` argument tags stored in reverse order (`arg_types[num_args - 1 - i]`): `0x1E` (`30`) = shape geometry, `0x0A` (`10`) = `outl` reference (`u32` ID), `0x14` (`20`) = `fild` reference (`u32` ID), `0x64` (`100`) = `trfd` transform, `0x2BC0` (`11200`) = polygon parameters.
  2. **Shape Geometry (`arg_type == 0x1E`)**:
     - **Rectangle (`1`)**: `width: i32`, `height: i32` (signed `0.1 µm`), `scale_x: f64`, `scale_y: f64`, `r0: i32`, `r1: i32`, `r2: i32`, `r3: i32` (`36` bytes).
     - **Ellipse (`2`)**: `cx: i32`, `cy: i32` (width/height in `0.1 µm`), `angle1: i32`, `angle2: i32`, `pie: u32` (`20` bytes).
     - **Curve / Polygon (`3` / `0x14`)**: `point_num: u16`, `pad: u16`, `point_num * (x: i32, y: i32)` coordinates in `0.1 µm` with Y-up relative to page center, followed by `point_num` node-type bytes (`0x00` = `MoveTo`, `0x40` = `LineTo`, `0x80` = `CubicTo` endpoint, `0xC0` = `CubicTo` control1, `0xA0` = `CubicTo` control2, bit `0x08` = close subpath).
- **Implementation Impact**:
  - Implemented in [`crates/cdr/src/parse.rs`](../crates/cdr/src/parse.rs) (`write_cdr7`, `parse_loda_chunk`, `parse_curve_payload`).

---

## REV-0005: CMX1 (`RIFF CMX1`) Header (`cont`), Page (`page`), Reference Lists (`rclr`, `rott`, `rpen`, `rotl`), and Index Table (`ixpg`, `ixmr`)
- **Date**: 2026-10-20
- **Status**: Verified
- **Public Sources**:
  - [LibreOffice `libcdr` CMX specification notes](https://wiki.documentfoundation.org/DLP/Libraries/libcdr) (`cmx2raw` differential oracle)
  - Public CMX 1.0 (Corel Presentation Exchange) RIFF specification
- **Evidence / Synthetic Test**:
  - [`crates/cdr/tests/oracle_cdr2raw.rs`](../crates/cdr/tests/oracle_cdr2raw.rs) (`write_cdr7_and_write_cmx1_pass_libcdr_cdr2raw_and_cmx2raw_oracle_r7_1`)
- **Structural Findings**:
  1. **`cont` Chunk (182 bytes)**:
     - Fixed 32-byte ASCII file identifier (`{Corel Binary Meta File}`), 16-byte OS string (`Windows 3.1`), 4-byte ASCII byte-order (`"2"` = little-endian), 2-byte coord-size (`"4"` = 32-bit), 4-byte major (`"1"`), 4-byte minor (`"0"`), `unit: u16` (`35` = mm), `factor: f64` (`0.001` = `1 µm` per coordinate unit), `index_section_offset: u32` at offset `82`, `info_section_offset: u32` at offset `86`, `thumbnail_offset: u32` at offset `90`, bounding box `(x0, y1, x1, y0): 4 * i32` at offset `94..110`, and `tally: u32` at offset `110..114`.
  2. **Resource Tables (`rclr`, `rott`, `rpen`, `rotl`)**:
     - 1-based indexed tables placed before `ixpg`/`ixmr`:
       - `rclr`: `count: u16`, per-entry `model: u8` (`5` = RGB, `2` = CMYK), `palette: u8`, and 4-byte color components.
       - `rott`: `count: u16`, per-entry `line_style: u8` + `cap_join: u8` (`(cap << 4) | join`).
       - `rpen`: `count: u16`, per-entry `width: i32` (in `1 µm` units), `aspect: u16` (`100`), `angle: i32` (`0`), and 6-fp16 identity matrix `[1, 0, 0, 0, 1, 0]`.
       - `rotl`: `count: u16`, per-entry `(outline_style_idx, screen_idx, color_idx, arrowheads_idx, pen_idx, dash_idx): 6 * u16`.
  3. **Instruction Stream (`page`)**:
     - Each instruction starts with `size: i16` (including the 4-byte instruction header) and `opcode: i16` (stored as positive opcode, negated on read):
       - `BeginPage (1)`: `page_num: u16`, `flags: u32`, `bbox: 4 * i32`, `end_page_offset: u32`, `group_count: u16`, `instr_count: i32`, identity matrix (`flag = 1`), and mapping mode `0`.
       - `BeginLayer (11)` / `EndLayer (12)` / `EndPage (2)`.
       - `PolyCurve (69)`: rendering attributes (`mask: u8` where `0x01` = fill, `0x02` = outline; Uniform fill `type = 1, color_idx: u16`; outline `rotl_idx: u16`), `point_num: u16`, `point_num * (x: i32, y: i32)`, `point_num` node bytes (`0x10`/`0x30`/`0x20`/`0x08`), and `bbox: 4 * i32`.
       - `JumpAbsolute (111)`: `offset: u32` pointing to the end of the `page` chunk.
  4. **Master Index (`ixpg`, `ixmr`)**:
     - `ixpg`: `count: u16`, per-page `(size_in_file = 16, page_offset: u32, ref_list_offset = 0xFFFF_FFFF, thumbnail_offset = 0xFFFF_FFFF, ref_records = 0)`.
     - `ixmr`: `master_id = 1`, `size = 64`, `record_count: u16`, followed by `(type_id: u16, offset: u32)` entries for `ixpg (3)`, `rclr (6)`, `rott (15)`, `rpen (16)`, and `rotl (17)`.
- **Implementation Impact**:
  - Implemented in [`crates/cdr/src/parse.rs`](../crates/cdr/src/parse.rs) (`write_cmx1`, `parse_cmx_page_chunk`, `parse_cmx_rclr_chunk`, `parse_cmx_rpen_chunk`, `parse_cmx_rotl_chunk`).


