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
}

impl ConfigConstructable for TerminalOutput {
    fn new(_cfg: &serde_json::Value) -> Self {
        Self {
            is_tty: crossterm::terminal::size().is_ok(),
        }
    }
}

impl Ui for TerminalOutput {
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) {
        if !self.is_tty {
            return PlainOutput.table(title, headers, rows);
        }

        let col_count = headers.len();
        let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        // Title in bold
        println!(
            "\n{}{}{}",
            SetAttribute(Attribute::Bold),
            title,
            SetAttribute(Attribute::Reset)
        );

        // Header row in cyan + bold
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

        // Separator
        let sep: String = widths.iter().map(|w| "─".repeat(*w)).collect::<Vec<_>>().join("  ");
        println!("{}{}{}", SetForegroundColor(Color::DarkGrey), sep, ResetColor);

        // Data rows — alternate subtle dim on odd rows
        for (i, row) in rows.iter().enumerate() {
            let line: String = row
                .iter()
                .zip(widths.iter())
                .map(|(c, w)| format!("{:<width$}", c, width = w))
                .collect::<Vec<_>>()
                .join("  ");
            if i % 2 == 0 {
                println!("{}", line);
            } else {
                println!(
                    "{}{}{}",
                    SetForegroundColor(Color::Grey),
                    line,
                    ResetColor
                );
            }
        }
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
            println!("  {}{}{}  {}  {}", SetForegroundColor(colour), mark, ResetColor, label, detail);
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
