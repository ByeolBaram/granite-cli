use crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};

use crate::registry::ConfigConstructable;
use crate::utils::ui::backends::plain::PlainOutput;
use crate::utils::ui::base::{HasUiMetadata, Ui, UiMetadata};

/*-- public --*/

/// The default terminal backend. Renders coloured, column-aligned output using
/// direct crossterm ANSI codes alongside `println!`.
///
/// No ratatui widgets, no raw mode, no Terminal, no draw loop. Output prints
/// directly to the terminal and stays there like any other CLI tool.
///
/// If stdout is not a tty (pipe, file redirect, CI — detected via
/// `crossterm::terminal::size()`) every method delegates to `PlainOutput`.
pub struct TerminalOutput {
    is_tty: bool,
    terminal_width: Option<u16>,
}

impl ConfigConstructable for TerminalOutput {
    fn new(_cfg: &serde_json::Value) -> Self {
        let (is_tty, width) = match crossterm::terminal::size() {
            Ok((cols, _rows)) => (true, Some(cols.max(20))),
            Err(_) => (false, None),
        };
        Self {
            is_tty,
            terminal_width: width,
        }
    }
}

impl Ui for TerminalOutput {
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) {
        if !self.is_tty {
            return PlainOutput.table(title, headers, rows);
        }

        let col_count = headers.len();

        // 1. Calculate natural column widths
        let mut natural_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    natural_widths[i] = natural_widths[i].max(cell.len());
                }
            }
        }

        // 2. Scale to terminal width
        let widths = self
            .terminal_width
            .map(|w| scale_widths(natural_widths.clone(), w, col_count))
            .unwrap_or(natural_widths);

        // 3. Render title (unchanged)
        println!(
            "\n{}{}{}",
            SetAttribute(Attribute::Bold),
            title,
            SetAttribute(Attribute::Reset)
        );

        // 4. Render header (unchanged)
        let header_line: String = headers
            .iter()
            .zip(widths.iter())
            .map(|(h, w)| format!("{:<width$}", h, width = w))
            .collect::<Vec<_>>()
            .join("  ");
        println!(
            "{}{}{}{}",
            SetForegroundColor(Color::Cyan),
            SetAttribute(Attribute::Bold),
            header_line,
            ResetColor,
        );

        // 5. Render separator (unchanged)
        let sep: String = widths.iter().map(|w| "─".repeat(*w)).collect::<Vec<_>>().join("  ");
        println!("{}{}{}", SetForegroundColor(Color::DarkGrey), sep, ResetColor);

        // 6. Render rows with wrapping
        render_rows_with_wrapping(rows, &widths);
    }

    fn detail(&self, title: &str, fields: &[(&str, String)]) {
        if !self.is_tty {
            return PlainOutput.detail(title, fields);
        }

        println!(
            "\n{}{}{}",
            SetAttribute(Attribute::Bold),
            title,
            SetAttribute(Attribute::Reset)
        );

        let key_width = fields.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (k, v) in fields {
            println!(
                "  {}{:<width$}{}  {}",
                SetForegroundColor(Color::Cyan),
                k,
                ResetColor,
                v,
                width = key_width,
            );
        }
    }

    fn status(&self, label: &str, ok: bool, detail: &str) {
        if !self.is_tty {
            return PlainOutput.status(label, ok, detail);
        }

        let (mark, colour) = if ok {
            ("✓", Color::Green)
        } else {
            ("✗", Color::Red)
        };
        if detail.is_empty() {
            println!("  {}{}{}  {}", SetForegroundColor(colour), mark, ResetColor, label);
        } else {
            println!(
                "  {}{}{}  {}  {}",
                SetForegroundColor(colour),
                mark,
                ResetColor,
                label,
                detail
            );
        }
    }

    fn info(&self, msg: &str) {
        if !self.is_tty {
            return PlainOutput.info(msg);
        }
        println!("{}", msg);
    }

    fn warn(&self, msg: &str) {
        if !self.is_tty {
            return PlainOutput.warn(msg);
        }
        println!(
            "{}{}Warning:{} {}",
            SetForegroundColor(Color::Yellow),
            SetAttribute(Attribute::Bold),
            ResetColor,
            msg
        );
    }

    fn error(&self, msg: &str) {
        if !self.is_tty {
            return PlainOutput.error(msg);
        }
        eprintln!(
            "{}{}Error:{} {}",
            SetForegroundColor(Color::Red),
            SetAttribute(Attribute::Bold),
            ResetColor,
            msg
        );
    }

    fn ok(&self, msg: &str) -> String {
        if !self.is_tty {
            return PlainOutput.ok(msg);
        }
        format!("{}{}{}", SetForegroundColor(Color::Green), msg, ResetColor)
    }

    fn warn_mark(&self, msg: &str) -> String {
        if !self.is_tty {
            return PlainOutput.warn_mark(msg);
        }
        format!(
            "{}{}{}{}",
            SetForegroundColor(Color::Yellow),
            SetAttribute(Attribute::Bold),
            msg,
            ResetColor
        )
    }

    fn error_mark(&self, msg: &str) -> String {
        if !self.is_tty {
            return PlainOutput.error_mark(msg);
        }
        format!(
            "{}{}{}{}",
            SetForegroundColor(Color::Red),
            SetAttribute(Attribute::Bold),
            msg,
            ResetColor
        )
    }

    fn detail_mark(&self, msg: &str) -> String {
        if !self.is_tty {
            return PlainOutput.detail_mark(msg);
        }
        format!("{}{}{}", SetForegroundColor(Color::DarkGrey), msg, ResetColor)
    }
}

impl HasUiMetadata for TerminalOutput {
    fn metadata() -> UiMetadata {
        UiMetadata {
            name: "terminal".to_string(),
            description: "Coloured ANSI terminal output; falls back to plain when not a tty"
                .to_string(),
        }
    }
}

/*-- private --*/

/// Minimum readable column width.
const MIN_WIDTH: usize = 8;

/// Minimum terminal width to enforce (treat narrower terminals as this wide).
const MIN_TERMINAL_WIDTH: u16 = 20;

/// Number of separator characters between columns.
const SEPARATOR_WIDTH: usize = 2;

/// Scale natural column widths proportionally to fit within the terminal width.
///
/// When the natural widths (plus separators) exceed the terminal width, columns
/// are scaled down proportionally. A minimum column width of 8 characters is
/// enforced. Rounding errors from floor operations are distributed to columns
/// that lost the largest fractional parts.
///
/// If the natural widths fit comfortably, they are returned unchanged.
fn scale_widths(natural: Vec<usize>, terminal_width: u16, col_count: usize) -> Vec<usize> {
    let term_width = terminal_width.max(MIN_TERMINAL_WIDTH) as usize;
    let separator_total = col_count.saturating_sub(1) * SEPARATOR_WIDTH;
    let available = term_width.saturating_sub(separator_total);

    let natural_total: usize = natural.iter().sum();

    // No scaling needed — natural widths fit
    if natural_total <= available {
        return natural;
    }

    // Scale proportionally
    let scale = available as f64 / natural_total as f64;

    // Compute scaled values with fractional tracking
    let mut scaled_with_fractions: Vec<(usize, usize, f64)> = natural
        .iter()
        .enumerate()
        .map(|(idx, &w)| {
            let exact = w as f64 * scale;
            let floored = exact.floor().max(MIN_WIDTH as f64) as usize;
            let lost = exact - floored as f64;
            (idx, floored, lost)
        })
        .collect();

    let mut scaled: Vec<usize> = scaled_with_fractions
        .iter()
        .map(|(_, floored, _)| *floored)
        .collect();

    // Handle case where MIN_WIDTH causes overflow: cap columns that exceed
    let scaled_total: usize = scaled.iter().sum();
    let mut excess = scaled_total.saturating_sub(available);

    while excess > 0 {
        // Find the column with the largest value that can still be reduced
        let mut best_idx: Option<usize> = None;
        let mut best_val = 0usize;
        for (idx, floored, _) in &scaled_with_fractions {
            if *floored > best_val && scaled[*idx] > MIN_WIDTH {
                best_idx = Some(*idx);
                best_val = scaled[*idx];
            }
        }
        if let Some(idx) = best_idx {
            scaled[idx] -= 1;
            excess -= 1;
        } else {
            break;
        }
    }

    // Distribute any remaining slack to columns that lost the most fraction
    let scaled_total: usize = scaled.iter().sum();
    let mut remaining = available.saturating_sub(scaled_total);

    scaled_with_fractions.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

    for (idx, _, _) in scaled_with_fractions.iter() {
        if remaining == 0 {
            break;
        }
        scaled[*idx] += 1;
        remaining -= 1;
    }

    scaled
}

/// Word-wrap text to fit within `max_width` characters.
///
/// Splitting occurs at whitespace or punctuation characters (including
/// hyphens). Words longer than `max_width` are character-wrapped as a
/// fallback. The result is a vector of lines that, when joined,
/// reconstitute the original text (minus trailing spaces).
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if text.is_empty() || text.len() <= max_width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_inclusive(|c: char| c.is_whitespace() || ",.;:!?-".contains(c)) {
        let word_trimmed = word.trim_start();

        if current_line.len() + word_trimmed.len() <= max_width {
            current_line.push_str(word_trimmed);
        } else {
            // Flush current line if not empty
            if !current_line.is_empty() {
                lines.push(current_line.trim_end().to_string());
                current_line = String::new();
            }

            // Handle word longer than max_width (character-wrap fallback)
            if word_trimmed.len() > max_width {
                let mut remaining = word_trimmed;
                while remaining.len() > max_width {
                    let (chunk, rest) = remaining.split_at(max_width);
                    lines.push(chunk.to_string());
                    remaining = rest;
                }
                if !remaining.is_empty() {
                    current_line.push_str(remaining);
                }
            } else {
                current_line.push_str(word_trimmed);
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line.trim_end().to_string());
    }

    // Ensure at least one line
    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Render data rows with word-wrapping support.
///
/// Each cell is wrapped independently to its column width. All lines of a
/// row share the same alternating background colour (odd rows → Grey).
fn render_rows_with_wrapping(rows: &[Vec<String>], widths: &[usize]) {
    for (row_idx, row) in rows.iter().enumerate() {
        // Wrap each cell
        let wrapped_cells: Vec<Vec<String>> = row
            .iter()
            .zip(widths.iter())
            .map(|(cell, width)| wrap_text(cell, *width))
            .collect();

        // Find max lines in this row
        let max_lines = wrapped_cells.iter().map(|l| l.len()).max().unwrap_or(1);

        // Determine if this row should be grey (alternating)
        let use_grey = row_idx % 2 == 1;

        // Print each line of the row
        for line_idx in 0..max_lines {
            let line_parts: Vec<String> = wrapped_cells
                .iter()
                .zip(widths.iter())
                .map(|(cell_lines, width)| {
                    cell_lines
                        .get(line_idx)
                        .map(|s| format!("{:<width$}", s, width = width))
                        .unwrap_or_else(|| " ".repeat(*width))
                })
                .collect();

            let line = line_parts.join("  ");

            // Apply colour to ALL lines of the row
            if use_grey {
                println!("{}{}{}", SetForegroundColor(Color::Grey), line, ResetColor);
            } else {
                println!("{}", line);
            }
        }
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_short_no_wrapping() {
        assert_eq!(wrap_text("hello", 10), vec!["hello"]);
    }

    #[test]
    fn wrap_text_empty() {
        assert_eq!(wrap_text("", 10), vec![""]);
    }

    #[test]
    fn wrap_text_at_space() {
        assert_eq!(wrap_text("hello world", 8), vec!["hello", "world"]);
    }

    #[test]
    fn wrap_text_at_hyphen() {
        assert_eq!(
            wrap_text("granite-3.0-8b", 12),
            vec!["granite-3.0-", "8b"]
        );
    }

    #[test]
    fn wrap_text_at_comma() {
        assert_eq!(wrap_text("Hello, world", 8), vec!["Hello,", "world"]);
    }

    #[test]
    fn wrap_text_character_wrap_long_word() {
        assert_eq!(
            wrap_text("verylongword", 6),
            vec!["verylo", "ngword"]
        );
    }

    #[test]
    fn wrap_text_multiple_breaks() {
        let result = wrap_text("a b c d e", 4);
        assert_eq!(result, vec!["a b", "c d", "e"]);
    }

    #[test]
    fn wrap_text_multiple_words() {
        let result = wrap_text("hello world this is a test", 12);
        assert_eq!(result, vec!["hello world", "this is a", "test"]);
    }

    #[test]
    fn wrap_text_punctuation_chain() {
        let result = wrap_text("one.two,three.four", 8);
        assert_eq!(result, vec!["one.two,", "three.", "four"]);
    }

    #[test]
    fn scale_widths_no_scaling_when_fits() {
        let natural = vec![10, 10, 10];
        let scaled = scale_widths(natural, 50, 3);
        assert_eq!(scaled, vec![10, 10, 10]);
    }

    #[test]
    fn scale_widths_proportional_scaling() {
        let natural = vec![20, 10, 10]; // total 40
        let scaled = scale_widths(natural, 30, 3); // available: 30-4=26
        // scale: 26/40 = 0.65
        // exact: [13, 6.5, 6.5]
        // floored: [13, 8, 8] (min 8) = 29, need 26
        // After reducing excess of 3: [10, 8, 8] = 26
        assert!(scaled.iter().sum::<usize>() <= 26);
        assert!(scaled.iter().all(|&w| w >= 8));
    }

    #[test]
    fn scale_widths_enforces_minimum_when_scaling_needed() {
        let natural = vec![20, 20, 20];
        let scaled = scale_widths(natural, 30, 3); // available: 30-4=26
        assert!(scaled.iter().all(|&w| w >= 8));
    }

    #[test]
    fn scale_widths_minimum_terminal_width() {
        let natural = vec![10, 10];
        let scaled = scale_widths(natural, 10, 2); // very narrow
        // Should treat as 20: available = 20-2 = 18
        assert!(scaled.iter().sum::<usize>() <= 18);
    }

    #[test]
    fn scale_widths_single_column() {
        let natural = vec![30];
        let scaled = scale_widths(natural, 30, 1); // no separators
        // available = 30, natural = 30, no scaling needed
        assert_eq!(scaled, vec![30]);
    }

    #[test]
    fn scale_widths_preserves_total_approximately() {
        let natural = vec![20, 15, 10, 5]; // total 50
        let scaled = scale_widths(natural, 40, 4); // available: 40-6=34
        // Total should be close to 34
        let total: usize = scaled.iter().sum();
        assert!(total <= 34);
        assert!(total >= 30); // within a few of target due to rounding
        assert!(scaled.iter().all(|&w| w >= 8));
    }

    #[test]
    fn scale_widths_very_large_difference() {
        // Natural is 100, available is 20 — should scale down to near 20
        let natural = vec![100];
        let scaled = scale_widths(natural, 22, 1); // available: 22
        assert_eq!(scaled, vec![22]);
    }

    #[test]
    fn scale_widths_multiple_columns_scale_down() {
        let natural = vec![25, 10, 6, 8, 10]; // total 59
        let scaled = scale_widths(natural, 50, 5); // available: 50-8=42
        let total: usize = scaled.iter().sum();
        assert!(total <= 42);
        assert!(scaled.iter().all(|&w| w >= 8));
    }

    #[test]
    fn scale_widths_rounding_compensation() {
        // Create a scenario where MIN_WIDTH forces total > available
        let natural = vec![7, 7, 7, 7, 7, 7, 7, 7, 7, 7]; // total 70
        let scaled = scale_widths(natural, 30, 10); // available: 30-18=12
        // Each gets at least MIN_WIDTH=8, so 10*8=80 > 12
        // This is expected — MIN_WIDTH is a hard floor
        assert!(scaled.iter().all(|&w| w >= 8));
        assert_eq!(scaled.iter().sum::<usize>(), 80);
    }

    #[test]
    fn scale_widths_exact_fit() {
        let natural = vec![10, 10]; // total 20
        let scaled = scale_widths(natural, 22, 2); // available: 22-2=20
        // Natural total (20) == available (20), no scaling needed
        assert_eq!(scaled, vec![10, 10]);
    }

    #[test]
    fn render_rows_with_wrapping_produces_no_panic() {
        let rows = vec![
            vec!["hello".to_string(), "world".to_string()],
            vec!["short".to_string(), "line".to_string()],
        ];
        let widths = vec![8, 8];
        render_rows_with_wrapping(&rows, &widths);
    }

    #[test]
    fn render_rows_with_wrapping_empty_rows() {
        let rows: Vec<Vec<String>> = vec![];
        let widths = vec![8, 8];
        render_rows_with_wrapping(&rows, &widths);
    }
}
