use crate::registry::ConfigConstructable;
use crate::utils::ui::output::{Output, OutputMetadata};

/*-- public --*/

/// The default terminal backend.
///
/// Phase 9 replaces the internals with ratatui widgets.
/// For now it renders via plain formatted text so the binary stays usable
/// while the factory scaffolding settles.
pub struct TerminalOutput;

impl ConfigConstructable for TerminalOutput {
    fn new(_cfg: &serde_json::Value) -> Self {
        Self
    }
}

impl Output for TerminalOutput {
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) {
        println!("\n{}", title);
        // Compute column widths from headers and all cell values
        let col_count = headers.len();
        let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }
        // Header row
        let header_line: String = headers.iter().zip(widths.iter())
            .map(|(h, w)| format!("{:<width$}", h, width = w))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{}", header_line);
        let sep: String = widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  ");
        println!("{}", sep);
        // Data rows
        for row in rows {
            let line: String = row.iter().zip(widths.iter())
                .map(|(c, w)| format!("{:<width$}", c, width = w))
                .collect::<Vec<_>>()
                .join("  ");
            println!("{}", line);
        }
    }

    fn detail(&self, title: &str, fields: &[(&str, String)]) {
        println!("\n{}", title);
        let key_width = fields.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (k, v) in fields {
            println!("  {:<width$}  {}", k, v, width = key_width);
        }
    }

    fn status(&self, label: &str, ok: bool, detail: &str) {
        let mark = if ok { "✓" } else { "✗" };
        if detail.is_empty() {
            println!("  {} {}", mark, label);
        } else {
            println!("  {} {}  {}", mark, label, detail);
        }
    }

    fn info(&self, msg: &str) {
        println!("{}", msg);
    }

    fn warn(&self, msg: &str) {
        println!("Warning: {}", msg);
    }

    fn error(&self, msg: &str) {
        eprintln!("Error: {}", msg);
    }
}

impl crate::utils::ui::output::HasOutputMetadata for TerminalOutput {
    fn metadata() -> OutputMetadata {
        OutputMetadata {
            name: "terminal".to_string(),
            description: "Formatted terminal output (ratatui in Phase 9)".to_string(),
        }
    }
}
