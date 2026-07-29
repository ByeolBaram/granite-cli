use crate::registry::ConfigConstructable;
use crate::utils::ui::base::{HasUiMetadata, Ui, UiMetadata};

/*-- public --*/

/// Plain text backend with no ANSI codes.
/// Suitable for CI, piped output, and non-ANSI terminals.
pub struct PlainOutput;

impl ConfigConstructable for PlainOutput {
    fn new(_cfg: &serde_json::Value) -> Self {
        Self
    }
}

impl Ui for PlainOutput {
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) {
        println!("\n{}", title);
        let col_count = headers.len();
        let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }
        let header_line: String = headers.iter().zip(widths.iter())
            .map(|(h, w)| format!("{:<width$}", h, width = w))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{}", header_line);
        let sep: String = widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  ");
        println!("{}", sep);
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
        let mark = if ok { "[OK]  " } else { "[FAIL]" };
        if detail.is_empty() {
            println!("{} {}", mark, label);
        } else {
            println!("{} {}  {}", mark, label, detail);
        }
    }

    fn info(&self, msg: &str)  { println!("{}", msg); }
    fn warn(&self, msg: &str)  { println!("Warning: {}", msg); }
    fn error(&self, msg: &str) { eprintln!("Error: {}", msg); }
}

impl HasUiMetadata for PlainOutput {
    fn metadata() -> UiMetadata {
        UiMetadata {
            name: "plain".to_string(),
            description: "Plain text output without ANSI codes".to_string(),
        }
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;

    crate::output_contract_tests!(PlainOutput::new(&serde_json::json!({})));
}
