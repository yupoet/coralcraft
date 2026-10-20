//! Clean-room paragraph text word-wrapping, multi-column flow, and alignment engine.

use serde::{Deserialize, Serialize};

use crate::model::{ParagraphTextSpec, TextAlignment};

/// Text case transformation mode in CorelDRAW's Change Case dialog (`Text -> Change Case` / `Shift+F3`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextCaseKind {
    /// Capitalizes the first letter of each sentence (`Sentence case`).
    #[default]
    SentenceCase,
    /// Converts all letters to lowercase (`lowercase`).
    Lowercase,
    /// Converts all letters to uppercase (`UPPERCASE`).
    Uppercase,
    /// Capitalizes the first letter of every word (`Title Case`).
    TitleCase,
    /// Inverts uppercase and lowercase on every letter (`tOGGLE cASE`).
    ToggleCase,
}

/// Applies `kind` (`Shift+F3` Change Case) to `text`.
#[must_use]
pub fn apply_text_case(text: &str, kind: TextCaseKind) -> String {
    match kind {
        TextCaseKind::Lowercase => text.to_lowercase(),
        TextCaseKind::Uppercase => text.to_uppercase(),
        TextCaseKind::ToggleCase => text
            .chars()
            .flat_map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().collect::<Vec<char>>()
                } else if c.is_lowercase() {
                    c.to_uppercase().collect::<Vec<char>>()
                } else {
                    vec![c]
                }
            })
            .collect(),
        TextCaseKind::TitleCase => {
            let mut out = String::with_capacity(text.len());
            let mut start_word = true;
            for c in text.chars() {
                if c.is_whitespace() || c == '-' || c == '/' {
                    out.push(c);
                    start_word = true;
                } else if start_word {
                    for u in c.to_uppercase() {
                        out.push(u);
                    }
                    start_word = false;
                } else {
                    for l in c.to_lowercase() {
                        out.push(l);
                    }
                }
            }
            out
        }
        TextCaseKind::SentenceCase => {
            let mut out = String::with_capacity(text.len());
            let mut cap_next = true;
            for c in text.chars() {
                if matches!(c, '.' | '!' | '?' | '\n') {
                    out.push(c);
                    cap_next = true;
                } else if c.is_alphabetic() {
                    if cap_next {
                        for u in c.to_uppercase() {
                            out.push(u);
                        }
                        cap_next = false;
                    } else {
                        for l in c.to_lowercase() {
                            out.push(l);
                        }
                    }
                } else {
                    out.push(c);
                }
            }
            out
        }
    }
}

/// Average character advance factor as a fraction of font point size.
pub const AVG_CHAR_EM: f64 = 0.55;
/// Space character advance factor as a fraction of font point size.
pub const SPACE_EM: f64 = 0.30;
/// First baseline ascent factor as a fraction of font point size.
pub const ASCENT_EM: f64 = 0.80;
/// Line bottom descent factor as a fraction of font point size.
pub const DESCENT_EM: f64 = 0.20;
/// Tab stop advance factor as a fraction of font point size.
pub const TAB_EM: f64 = 2.0;

/// Measures estimated advance width of a string in typographic points.
#[must_use]
pub fn advance_pt(s: &str, font_size_pt: f64) -> f64 {
    s.chars().fold(0.0, |acc, ch| {
        acc + font_size_pt
            * match ch {
                ' ' => SPACE_EM,
                '\t' => TAB_EM,
                '\u{200B}'..='\u{200D}' | '\u{FEFF}' => 0.0,
                _ => AVG_CHAR_EM,
            }
    })
}

/// A positioned line of text within a paragraph text frame.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedTextLine {
    /// 0-based column index where this line was placed.
    pub column: u32,
    /// Absolute baseline X position in local frame coordinates (including frame position).
    pub origin_x: f64,
    /// Absolute baseline Y position in local frame coordinates (including frame position).
    pub origin_y: f64,
    /// The text content of the line (with trailing spaces trimmed).
    pub text: String,
    /// Natural measured advance width of the line in points.
    pub width_pt: f64,
    /// Word spacing extra advance in points (only > 0 for non-last lines in full justification).
    pub word_spacing_pt: f64,
    /// Number of inter-word gaps in this line.
    pub gap_count: usize,
}

/// Complete layout result for a [`ParagraphTextSpec`].
#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphLayout {
    /// All positioned lines of text across all columns.
    pub lines: Vec<PositionedTextLine>,
    /// Whether any text overflowed the available height across all columns.
    pub overflow: bool,
    /// Number of columns used.
    pub column_count: u32,
    /// Computed width of each column in points.
    pub column_width_pt: f64,
}

/// Computes the word-wrapped, aligned, multi-column layout for a paragraph text specification.
#[must_use]
pub fn layout_paragraph_text(spec: &ParagraphTextSpec) -> ParagraphLayout {
    let frame_w = spec.frame_width_pt.get();
    let frame_h = spec.frame_height_pt.get();
    let font_size = spec.font_size_pt.get().max(1.0);
    let line_spacing_mult = spec.line_spacing_multiplier.get().max(0.1);
    let line_height = font_size * line_spacing_mult;
    let cols = spec.columns.max(1);
    let gutter = spec.gutter_pt.get().max(0.0);

    let total_gutter = if cols > 1 { (cols - 1) as f64 * gutter } else { 0.0 };
    let col_w = ((frame_w - total_gutter) / (cols as f64)).max(0.0);

    if col_w <= 1e-4 || frame_h <= 1e-4 {
        return ParagraphLayout { lines: Vec::new(), overflow: !spec.text.trim().is_empty(), column_count: cols, column_width_pt: col_w };
    }

    let normalized = spec.text.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs: Vec<&str> = normalized.split('\n').collect();

    let mut positioned_lines = Vec::new();
    let mut current_col: u32 = 0;
    let mut cursor_y = ASCENT_EM * font_size;
    let mut overflow = false;

    let space_width = SPACE_EM * font_size;

    for (para_idx, para) in paragraphs.iter().enumerate() {
        let _is_last_para = para_idx + 1 == paragraphs.len();
        let words: Vec<&str> = para.split_whitespace().collect();

        if words.is_empty() {
            if cursor_y + DESCENT_EM * font_size > frame_h {
                if current_col + 1 < cols {
                    current_col += 1;
                    cursor_y = ASCENT_EM * font_size;
                } else {
                    overflow = true;
                }
            }
            cursor_y += line_height;
            continue;
        }

        let mut line_words: Vec<&str> = Vec::new();
        let mut line_natural_w = 0.0;

        let mut w_idx = 0;
        while w_idx < words.len() {
            let word = words[w_idx];
            let word_w = advance_pt(word, font_size);

            if line_words.is_empty() {
                line_words.push(word);
                line_natural_w = word_w;
                w_idx += 1;
            } else {
                let test_w = line_natural_w + space_width + word_w;
                if test_w <= col_w {
                    line_words.push(word);
                    line_natural_w = test_w;
                    w_idx += 1;
                } else {
                    emit_line(
                        &line_words,
                        line_natural_w,
                        false,
                        spec,
                        col_w,
                        gutter,
                        &mut current_col,
                        &mut cursor_y,
                        frame_h,
                        cols,
                        line_height,
                        font_size,
                        &mut overflow,
                        &mut positioned_lines,
                    );
                    line_words.clear();
                    line_natural_w = 0.0;
                }
            }
        }

        if !line_words.is_empty() {
            emit_line(
                &line_words,
                line_natural_w,
                true,
                spec,
                col_w,
                gutter,
                &mut current_col,
                &mut cursor_y,
                frame_h,
                cols,
                line_height,
                font_size,
                &mut overflow,
                &mut positioned_lines,
            );
        }
    }

    ParagraphLayout { lines: positioned_lines, overflow, column_count: cols, column_width_pt: col_w }
}

#[allow(clippy::too_many_arguments)]
fn emit_line(
    words: &[&str],
    natural_w: f64,
    is_last_line_of_para: bool,
    spec: &ParagraphTextSpec,
    col_w: f64,
    gutter: f64,
    current_col: &mut u32,
    cursor_y: &mut f64,
    frame_h: f64,
    cols: u32,
    line_height: f64,
    font_size: f64,
    overflow: &mut bool,
    positioned_lines: &mut Vec<PositionedTextLine>,
) {
    if *cursor_y + DESCENT_EM * font_size > frame_h {
        if *current_col + 1 < cols {
            *current_col += 1;
            *cursor_y = ASCENT_EM * font_size;
        } else {
            *overflow = true;
            return;
        }
    }

    let gap_count = words.len().saturating_sub(1);
    let mut word_spacing_pt = 0.0;

    let col_x_offset = (*current_col as f64) * (col_w + gutter);
    let align_offset = match spec.alignment {
        TextAlignment::Left => 0.0,
        TextAlignment::Center => ((col_w - natural_w) / 2.0).max(0.0),
        TextAlignment::Right => (col_w - natural_w).max(0.0),
        TextAlignment::Justify => {
            if !is_last_line_of_para && gap_count > 0 && col_w > natural_w {
                word_spacing_pt = (col_w - natural_w) / (gap_count as f64);
            }
            0.0
        }
    };

    let line_text = words.join(" ");
    let origin_x = spec.position.x.get() + col_x_offset + align_offset;
    let origin_y = spec.position.y.get() + *cursor_y;

    positioned_lines.push(PositionedTextLine {
        column: *current_col,
        origin_x,
        origin_y,
        text: line_text,
        width_pt: natural_w,
        word_spacing_pt,
        gap_count,
    });

    *cursor_y += line_height;
}
