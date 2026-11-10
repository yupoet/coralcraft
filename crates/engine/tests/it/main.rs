//! Integration tests: one test binary for the crate, one module per area.
// Unwrapping and panicking on failure is fine here, unlike in shipped code (AGENTS.md › Robustness).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../support/affinity_docs.rs"]
mod affinity_docs;
#[path = "../../../affinity/tests/support/text_fixture.rs"]
mod text_fixture;

mod affinity_features;
mod affinity_local;
mod artboard_click;
mod beyond_canvas;
mod clear_commands;
mod compound_attributes;
mod copy_direct_selection;
// CoralCraft: legacy CDR text decoding and residuals (P2 Task 9).
mod coreldraw_cdr_text;
// CoralCraft: advanced fills and transparency (P3 Task 3).
mod coreldraw_fills_transparency;
// CoralCraft: the CorelDRAW selection and node workflow (P1 Task 3).
mod coreldraw_node_workflow;
// CoralCraft: the CorelDRAW pages and master layers (P1 Task 10).
mod coreldraw_pages;
// CoralCraft: the CorelDRAW PowerClip lifecycle (P3 Task 2).
mod coreldraw_powerclip;
// CoralCraft: the P1 persistence audit (P1 Task 11).
mod coreldraw_p1_persistence;
// CoralCraft: the CorelDRAW fill/outline appearance (P1 Task 7).
mod coreldraw_appearance;
// CoralCraft: the P1 acceptance scenario (P1 Task 13).
mod coreldraw_p1_acceptance;
// CoralCraft: the P2 brochure acceptance scenario (P2 Task 12).
mod coreldraw_p2_acceptance;
// CoralCraft: the CorelDRAW shaping operations (P1 Task 4).
mod coreldraw_shaping;
// CoralCraft: the CorelDRAW styles and symbols (P1 Task 9).
mod coreldraw_styles_symbols;
// CoralCraft: professional text workflows (P2 Task 3).
mod coreldraw_text;
// CoralCraft: path text, wrapping, and layout workflows (P2 Task 5).
mod coreldraw_layout;
// CoralCraft: the CorelDRAW transform and align workflows (P1 Task 6).
mod coreldraw_transform;
mod dimensions;
mod eyedropper_sources;
mod find_replace_paras;
mod inline_text;
mod known_bugs;
mod lossy_overwrite;
mod model_based;
mod param_keys;
mod pen_click_after_smooth;
mod pen_endpoint_picking;
mod ruler_origin;
mod selection_commands;
mod stroke_visibility;
mod text_stroke_scale;
mod three_d_light_color;
