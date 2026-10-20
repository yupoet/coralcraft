<h1 align="center">CoralCraft</h1>

<p align="center">
  <b>Vector illustration, multipage layout, and print production; an open-source, modern CorelDRAW-parity application, built in pure Rust.</b>
</p>

<p align="center">
  A fast, open-source vector illustration and desktop publishing workflow targeting <b>CorelDRAW</b> parity (often searched as <i>CoralDraw</i>) with native <code>.cdr</code> and <code>.cmx</code> format support. It runs natively on macOS, Windows, Linux and FreeBSD, and in the browser via WebAssembly.
</p>

<p align="center">
  <img alt="Status: usable beta baseline (M0-M64)" src="https://img.shields.io/badge/status-usable%20beta%20(M0--M64)-2ea44f">
  <img alt="Written in pure Rust" src="https://img.shields.io/badge/pure-Rust-b83a24?logo=rust&logoColor=white">
  <img alt="Target parity: CorelDRAW (.cdr / .cmx)" src="https://img.shields.io/badge/parity-CorelDRAW%20(.cdr%20%C2%B7%20.cmx)-555555">
  <img alt="Runs on macOS, Windows, Linux, FreeBSD and the web" src="https://img.shields.io/badge/runs%20on-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux%20%C2%B7%20FreeBSD%20%C2%B7%20Web-555555">
  <img alt="MCP server: 240 tools for AI agents" src="https://img.shields.io/badge/MCP-240%20tools-0969da">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-555555">
</p>

<p align="center">
  <a href="#why-coralcraft">Why CoralCraft</a> ·
  <a href="#downloads">Downloads</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#240-mcp-tools-agent-native-automation">240 MCP tools</a> ·
  <a href="#status">Status</a> ·
  <a href="#clean-room-methodology">Clean-room methodology</a> ·
  <a href="#star-history">Star history</a> ·
  <a href="#license-and-credits">License</a>
</p>

## Why CoralCraft

- **Familiar to CorelDRAW users.** Designed around the layout, tools, menus, dockers and shortcuts of **CorelDRAW** (frequently searched as *CoralDraw*): Pick, Shape (node editing), Freehand and Bézier, Smart Fill, Shaping (`Weld` / `Trim` / `Intersect` / `Simplify` / `FrontMinusBack` / `BackMinusFront` / `Boundary`), Contour, Blend, Envelope, Extrude, Perspective, PowerClip, Object Manager, Multipage spreads and Color Palettes.
- **Native `.cdr` and `.cmx` format support.** A dedicated clean-room format engine (`coralcraft-cdr`) for CorelDRAW `.cdr` files (from legacy `WL` v1–v2 and RIFF-based v3–X3 to ZIP+RIFF X4–2024+) and `.cmx` exchange files across three complete pipelines:
  1. **Probe (`M1`)**: container detection (`RIFF` vs. `WL` vs. `ZIP+RIFF`), version identification, and hostile-input-safe chunk tree inspection.
  2. **Parse (`M3`)**: translating `.cdr` / `.cmx` object hierarchies, curves, fills, strokes, pages and layers into the CoralCraft document model with structured import-warning ledgers.
  3. **Write (`M5+`)**: clean-room open-source `.cdr` (`CDR v12 RIFF`, `CDR X4+ ZIP+RIFF`) and `.cmx` (`CMX1 RIFF`) export plus `.coralcraft.zip` package round-trip for production workflows.
- **Multipage & print-first.** Unlike single-artboard illustration tools, CoralCraft's document model (`coralcraft-doc`) is built around multipage documents, facing-page spreads, master pages, drawing scales (`1:10`, `1:100`), CMYK + CIE LAB + Spot color palettes (`.cpl` / XML), print imposition (2-up saddle-stitch, 4-up N-up), crop/registration/color-bar prepress marks, and G-code/HPGL CAM output for signage, print shops, vinyl cutting, laser engraving and packaging.
- **Fast.** Multithreaded software and tile-based rendering (`coralcraft-render`) backed by a zero-dependency 2D geometry kernel (`coralcraft-geom`) with exact adaptive flattening, dash stroking, miter/round/bevel joins, and supersampled anti-aliasing.
- **Robust.** Every parser treats `.cdr` / `.cmx` files as untrusted hostile input — `unsafe_code`, `unwrap_used`, `expect_used` and `panic` are denied at the workspace level (`Cargo.toml` lints), and undo/redo uses structural sharing.
- **Open.** Documented native `.coralcraft` JSON and `.coralcraft.zip` formats alongside `.cdr`, `.cmx`, SVG 1.1, PDF 1.4 (with `/Separation` spot color & CMYK prepress output), EPSF-3.0, AutoCAD R12 DXF, HPGL, G-code, PPM, BMP and PNG exchange.
- **Agent-native (`240 MCP tools`).** Every menu item, tool gesture, docker, geometric transformation, generative pattern, and export pipeline is exposed through an embedded **Model Context Protocol (MCP) server** (`coralcraft-mcp`) with **240 tools**, so Claude Code, Cursor, Codex and other AI agents can inspect, draw, lay out and export complex multi-page documents programmatically.
- **Everywhere.** One pure-Rust codebase (`#![forbid(unsafe_code)]`) for native desktop apps on macOS (Intel & Apple Silicon Universal2), Windows (`x64` & `arm64`), Linux (`AppImage`, `Flatpak`, `.deb`, `.rpm`, `.tar.gz`), FreeBSD (`x86_64`), and the browser via WebAssembly.

## Downloads

Pre-built binaries for **Windows** (`x64` / `arm64`), **macOS** (Universal2 `.dmg` / `.pkg`, Apple Silicon `aarch64`, and Intel `x86_64`), **Linux** (`AppImage`, `Flatpak`, `.deb`, `.rpm`, `.tar.gz`), **FreeBSD** (`x86_64`) and **Web (WebAssembly)** are published on [**GitHub Releases (`v0.0.1`)**](https://github.com/yupoet/coralcraft/releases/latest):

| Platform | Architecture | Format | Package / Download |
| :--- | :--- | :--- | :--- |
| **macOS** | **Universal2** (`Intel + Apple Silicon`) | `.dmg` Disk Image | [`coralcraft-0.0.1-macos-universal.dmg`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-macos-universal.dmg) |
| **macOS** | **Universal2** (`Intel + Apple Silicon`) | `.pkg` Installer | [`coralcraft-0.0.1-macos-universal.pkg`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-macos-universal.pkg) |
| **macOS** | **Apple Silicon (`M1`–`M4`, `aarch64`)** | `.dmg` / `.pkg` / `.app.zip` | [`coralcraft-0.0.1-macos-aarch64.dmg`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-macos-aarch64.dmg) · [`.pkg`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-macos-aarch64.pkg) · [`.app.zip`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-macos-aarch64.app.zip) |
| **macOS** | **Intel (`x86_64`)** | `.dmg` / `.pkg` / `.app.zip` | [`coralcraft-0.0.1-macos-x86_64.dmg`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-macos-x86_64.dmg) · [`.pkg`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-macos-x86_64.pkg) · [`.app.zip`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-macos-x86_64.app.zip) |
| **Windows** | **64-bit (`x64`)** | GUI Setup `.exe` & Portable `.zip` | [`coralcraft-0.0.1-windows-x64-setup.exe`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-windows-x64-setup.exe) · [`-portable.zip`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-windows-x64-portable.zip) |
| **Windows** | **ARM64 (`arm64`)** | GUI Setup `.exe` & Portable `.zip` | [`coralcraft-0.0.1-windows-arm64-setup.exe`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-windows-arm64-setup.exe) · [`-portable.zip`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-windows-arm64-portable.zip) |
| **Linux (GUI)** | **64-bit (`x86_64`)** | `AppImage` (Portable GUI) | [`coralcraft-0.0.1-linux-x86_64.AppImage`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-linux-x86_64.AppImage) |
| **Linux (GUI)** | **64-bit (`x86_64`)** | `Flatpak` Bundle | [`coralcraft-0.0.1-linux-x86_64.flatpak`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-linux-x86_64.flatpak) |
| **Linux (GUI)** | **64-bit (`x86_64`)** | `.deb` (Debian / Ubuntu) | [`coralcraft-0.0.1-linux-x86_64.deb`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-linux-x86_64.deb) |
| **Linux (GUI)** | **64-bit (`x86_64`)** | `.rpm` (Fedora / RHEL / openSUSE) | [`coralcraft-0.0.1-linux-x86_64.rpm`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-linux-x86_64.rpm) |
| **Linux (GUI)** | **64-bit (`x86_64`)** | `.tar.gz` Archive | [`coralcraft-0.0.1-linux-x86_64.tar.gz`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-linux-x86_64.tar.gz) |
| **FreeBSD** | **64-bit (`x86_64`)** | `.tar.gz` Archive | [`coralcraft-0.0.1-freebsd-x86_64.tar.gz`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-freebsd-x86_64.tar.gz) |
| **Web** | **WebAssembly (`wasm32`)** | Browser Studio `.zip` / `.tar.gz` | [`coralcraft-0.0.1-web-wasm32.zip`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-web-wasm32.zip) · [`.tar.gz`](https://github.com/yupoet/coralcraft/releases/latest/download/coralcraft-0.0.1-web-wasm32.tar.gz) |

To reproduce all release packages locally from source:

```sh
cargo xtask package
# or directly:
./packaging/package-all.sh
```

## Quick start

Requires **Rust 1.95+** (Edition 2024).

```sh
git clone https://github.com/yupoet/coralcraft
cd coralcraft

# Launch the interactive desktop GUI studio
cargo run --release -p coralcraft

# Inspect, convert, render, or run the 240-tool MCP server via CLI
cargo run --release -p coralcraft-cli -- probe <file.cdr>
cargo run --release -p coralcraft-cli -- import <file.cdr> -o output.coralcraft
cargo run --release -p coralcraft-cli -- export output.coralcraft -o output.svg
cargo run --release -p coralcraft-cli -- render output.coralcraft -o preview.png --scale 2.0
cargo run --release -p coralcraft-cli -- mcp

# Workspace quality, layering & clean-room gates
cargo xtask cleanroom                                       # scan for forbidden vendor copyright markers & fixtures
cargo xtask assets                                          # verify every asset is registered in ASSETS.md
cargo xtask layers                                          # enforce L0..L7 crate dependency layering
cargo xtask stats                                           # crate-by-crate file, line & test counts
cargo xtask ci                                              # fmt, clippy, cleanroom, assets, layers, tests
```

## 240 MCP Tools (Agent-Native Automation)

CoralCraft includes a built-in, zero-configuration **Model Context Protocol (MCP)** server (`coralcraft-mcp`, exposed via `coralcraft-cli mcp` over JSON-RPC 2.0 `stdio`) providing **240 structured tools** for AI agents.

### Register with Claude Code / Cursor / MCP Clients

```sh
claude mcp add coralcraft -- /path/to/coralcraft-cli mcp
```

### Capability Overview Across All 240 MCP Tools

| Category | Tool Count | Representative MCP Tools |
| :--- | :---: | :--- |
| **1. Document, Pages, Spreads & Guides** | 24 | `new_document`, `get_document_info`, `add_page`, `remove_page`, `duplicate_page`, `reorder_pages`, `set_active_page`, `set_page_size`, `set_page_orientation`, `set_page_margins`, `set_page_bleed`, `set_page_background`, `set_facing_pages`, `set_drawing_scale`, `add_guideline`, `add_grid_guidelines`, `auto_paginate_objects_into_grid` |
| **2. Layers, Object Manager & Hierarchy** | 18 | `add_layer`, `remove_layer`, `set_layer_visibility`, `set_layer_locked`, `set_layer_wireframe`, `move_layer`, `flatten_layers`, `group_selection`, `ungroup_selection`, `reorder_object`, `duplicate_object`, `clone_object`, `find_objects_by_Filter`, `set_object_name_and_tags`, `set_object_hyperlink` |
| **3. Classic & Parametric Vector Shapes** | 32 | `add_rectangle`, `add_ellipse`, `add_polygon`, `add_star`, `add_complex_star`, `add_spiral`, `add_archimedean_spiral`, `add_grid_paper`, `add_callout_shape`, `add_banner_ribbon`, `add_arrow_shape`, `add_gear_wheel`, `add_sprocket_wheel`, `add_involute_gear`, `add_rack_gear`, `add_cam_disk_profile`, `add_heart_shape`, `add_crescent_shape`, `add_cross_shape`, `add_ring_donut`, `add_pie_arc_sector`, `add_rounded_polygon`, `add_superellipse_squircle`, `add_reuleaux_polygon`, `add_hypocycloid_astroid`, `add_epicycloid_cardioid`, `add_rose_rhodonea`, `add_lissajous_curve` |
| **4. Node Editing, Curves & Path Surgery** | 28 | `add_bezier_path`, `add_bspline_curve`, `add_catmull_rom_spline`, `add_hobby_smooth_curve`, `add_variable_width_stroke_outline`, `reverse_object_path`, `close_object_subpaths`, `split_object_subpaths`, `join_open_paths`, `simplify_object_path`, `chaikin_smooth_object_path`, `fillet_object_corners`, `chamfer_object_corners`, `scallop_object_corners`, `roughen_object_path`, `zig_zag_object_path`, `wave_distort_object_path`, `dash_decompose_object_path`, `slice_object_with_line`, `crop_objects_to_rect`, `eraser_nibble_object`, `trim_path_segment_at_intersections` |
| **5. Boolean Shaping, Offsets & Contours** | 14 | `boolean_shape` (`Weld`, `Trim`, `Intersect`, `Simplify`, `FrontMinusBack`, `BackMinusFront`, `Boundary`), `offset_object_path`, `create_object_boundary`, `smart_fill_at_point`, `apply_contour_effect`, `expand_contour_to_objects`, `medial_axis_skeleton` |
| **6. Color Management, Palettes, Fills & Strokes** | 24 | `set_fill_solid`, `set_fill_cmyk`, `set_fill_spot`, `set_fill_linear_gradient`, `set_fill_radial_gradient`, `set_fill_conical_gradient`, `set_fill_mesh_gradient`, `set_fill_hatch_pattern`, `set_fill_postscript_pattern`, `set_stroke`, `set_stroke_arrows`, `set_stroke_dashes`, `copy_object_style`, `apply_object_style`, `convert_document_colors_to_cmyk`, `convert_document_colors_to_grayscale`, `invert_selection_colors`, `adjust_selection_hue_saturation`, `harmony_palette_from_object`, `extract_document_palette`, `check_color_contrast_wcag` |
| **7. Live Effects, Envelopes, 3D & PowerClip** | 24 | `apply_blend_between`, `apply_blend_along_path`, `apply_envelope`, `apply_extrude`, `apply_perspective`, `apply_drop_shadow`, `apply_block_shadow`, `apply_inner_shadow`, `apply_bevel_effect`, `apply_lens_effect`, `place_inside_powerclip`, `extract_from_powerclip`, `twirl_distort_object_path`, `spherize_lens_distort_object_path`, `polar_to_cartesian_warp_object_path`, `bend_warp_object_path`, `shear_warp_object_path`, `lattice_ffd_warp_object_path`, `perspective_project_object_path` |
| **8. Typography, Tables & Technical Callouts** | 18 | `add_artistic_text`, `add_paragraph_text`, `set_text_content`, `set_text_typography`, `set_paragraph_columns`, `fit_text_to_path`, `convert_text_to_curves`, `link_paragraph_text_frames`, `add_table`, `set_table_cell_text`, `merge_table_cells`, `convert_table_to_vector_group`, `add_linear_dimension`, `add_angular_dimension`, `add_radial_dimension`, `add_callout_dimension`, `add_connector_line`, `measure_object_geometry` |
| **9. Transforms, Alignment, Packing & Step-and-Repeat** | 22 | `translate_selection`, `scale_selection`, `rotate_selection`, `skew_selection`, `mirror_selection`, `align_selection`, `distribute_selection`, `step_and_repeat`, `radial_repeat_selection`, `grid_cloner_selection`, `pack_objects_on_page`, `nest_objects_in_sheet`, `distribute_objects_along_path`, `distribute_objects_in_circle`, `phyllotaxis_arrange_selection`, `isometric_project_selection`, `scale_selection_progressively`, `rotate_selection_progressively`, `harmonize_selection_sizes`, `snap_selection_to_pixel_grid` |
| **10. Computational, Mathematical & Generative Art** | 20 | `add_voronoi_diagram`, `add_delaunay_triangulation`, `add_convex_hull_around_selection`, `add_minimum_bounding_circle_around_selection`, `add_truchet_tile_pattern`, `add_islamic_star_rosette`, `add_celtic_knot_interlace`, `add_guilloche_rosette`, `add_moir_concentric_rings`, `add_halftone_dot_screen`, `add_stipple_dot_cloud`, `add_flow_field_streamlines`, `add_hilbert_curve`, `add_peano_curve`, `add_sierpinski_triangle`, `add_koch_snowflake`, `add_dragon_curve`, `add_lsystem_fractal_tree`, `add_metaball_connected_blobs`, `add_maze_labyrinth` |
| **11. Barcodes, Charts, Print Merge & CAM/CNC** | 12 | `add_qr_code`, `add_code128_barcode`, `add_ean13_barcode`, `add_data_matrix_code`, `add_pie_chart`, `add_bar_chart`, `add_line_radar_chart`, `execute_print_merge`, `add_prepress_marks`, `impose_pages_2up_saddle_stitch`, `impose_pages_4up_nup`, `export_gcode_toolpath`, `export_hpgl_plt` |
| **12. File Format I/O, Rendering & Session Inspection** | 12 | `probe_cdr_base64`, `import_cdr_base64`, `export_cdr_base64`, `export_cmx_base64`, `import_coralcraft_zip_base64`, `export_coralcraft_zip_base64`, `import_svg`, `export_svg`, `export_pdf_base64`, `export_eps`, `export_dxf`, `render_page_ppm_base64`, `render_ui_frame_ppm_base64`, `undo`, `redo`, `preflight_document` |

## Status

CoralCraft has completed **Milestones `M0` through `M64`** (**usable beta baseline**). See [**ROADMAP.md**](ROADMAP.md) for per-dimension parity scores, [`docs/target-app-parity.md`](docs/target-app-parity.md) for the CorelDRAW feature parity matrix, and [`docs/file-format-parity.md`](docs/file-format-parity.md) for the format support matrix.

- **12 Workspace Crates (`L0`–`L7`):** `crates/{geom, color, doc, cdr, render, tools, engine, ui-egui, mcp}`, `apps/{coralcraft, coralcraft-cli}`, and `xtask`.
- **Zero-Panic & 100% Safe Rust:** `#![forbid(unsafe_code)]`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"` enforced across all non-test library and application code.
- **240 MCP Tools:** Full programmatic document authoring, node editing, boolean shaping, live effects, prepress imposition, and multi-format import/export via `coralcraft-mcp`.

## Clean-room methodology

CoralCraft follows the clean-room engineering discipline established by [storytold/vectorcraft](https://github.com/storytold/vectorcraft), extended with a dual-track protocol for `.cdr` / `.cmx` format interoperability (see [`AGENTS.md`](AGENTS.md)):

1. **Behavior track (public documentation only):**
   - Application features, workflows, dockers and shortcuts are derived **exclusively from public documentation** (official online user manuals, published books, public tutorials and forum discussions, cited by URL).
   - Contributors and AI agents **never** run, script, automate, screenshot, measure or disassemble CorelDRAW or any Corel software, and **never** use Corel software to create test files.
2. **Format track (public specifications & owned samples):**
   - Binary format support is written from scratch in safe Rust using public specifications ([Photopea CDR Format Specification](https://github.com/photopea/CDR-specification), [Document Liberation Project / libcdr documentation](https://wiki.documentfoundation.org/DLP/Libraries/libcdr), RIFF and ISO 32000 specs) and synthetic byte fixtures authored directly in code.
   - No GPL/AGPL code (such as Inkscape, UniConvertor or sK1) is ever copied or linked. MPL-2.0 references (`libcdr`) are consulted strictly as read-only format documentation, with chunk-level findings recorded in `docs/reverse/`.

> [!NOTE]
> **Trademark Disclaimer:** CoralCraft is an independent open-source project and is **not** affiliated with, endorsed by, or sponsored by Corel or Alludo. **CorelDRAW** is a registered trademark of its respective owner and is referenced here solely for descriptive interoperability and feature-parity comparison.

## Star history

[![Star History Chart](https://api.star-history.com/svg?repos=yupoet/coralcraft&type=Date&legend=top-left)](https://www.star-history.com/?repos=yupoet%2Fcoralcraft&type=date&legend=top-left)

## License and credits

CoralCraft is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Required notices are in [NOTICE](NOTICE), and every bundled asset is listed with its author, source and license in [ASSETS.md](ASSETS.md).

CoralCraft's workspace architecture, crate layering (`L0`–`L7`), agent-native MCP design, and clean-room engineering methodology are modeled after [VectorCraft](https://github.com/storytold/vectorcraft) and the [ArtCraft Crafting Apps](https://getartcraft.com/apps) family (`DesignCraft`, `CADCraft`, `PhotoCraft`, `PdfCraft`).
