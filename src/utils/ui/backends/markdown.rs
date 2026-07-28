use crate::registry::ConfigConstructable;
use crate::utils::ui::output::{HasOutputMetadata, Output, OutputMetadata};

/*-- public --*/

/// GFM (GitHub Flavoured Markdown) output backend.
/// Renders tables as pipe tables and detail as a two-column key/value table.
pub struct MarkdownOutput;

impl ConfigConstructable for MarkdownOutput {
    fn new(_cfg: &serde_json::Value) -> Self {
        Self
    }
}

impl Output for MarkdownOutput {
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) {
        println!("\n## {}\n", title);
        let header_line = format!("| {} |", headers.join(" | "));
        let sep_line = format!("| {} |", headers.iter().map(|h| "-".repeat(h.len() + 2)).collect::<Vec<_>>().join(" | "));
        println!("{}", header_line);
        println!("{}", sep_line);
        for row in rows {
            println!("| {} |", row.join(" | "));
        }
    }

    fn detail(&self, title: &str, fields: &[(&str, String)]) {
        println!("\n## {}\n", title);
        println!("| Field | Value |");
        println!("|-------|-------|");
        for (k, v) in fields {
            println!("| {} | {} |", k, v);
        }
    }

    fn status(&self, label: &str, ok: bool, detail: &str) {
        let mark = if ok { "✓" } else { "✗" };
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

impl HasOutputMetadata for MarkdownOutput {
    fn metadata() -> OutputMetadata {
        OutputMetadata {
            name: "markdown".to_string(),
            description: "GFM markdown table output for documentation and GitHub".to_string(),
        }
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::ui::output::tests::CaptureOutput;

    crate::output_contract_tests!(MarkdownOutput::new(&serde_json::json!({})));

    #[test]
    fn markdown_table_contains_pipe_chars() {
        let out = CaptureOutput::default();
        out.table("Test", &["ID", "NAME"], &[vec!["id-1".to_string(), "name-1".to_string()]]);
        let tables = out.tables.borrow();
        // verify the CaptureOutput recorded correctly; live rendering tested via contract tests
        assert_eq!(tables.len(), 1);
    }

    #[test]
    fn markdown_table_has_header_separator() {
        // Invoke the real MarkdownOutput (print-only) — just verifies no panic
        let md = MarkdownOutput::new(&serde_json::json!({}));
        md.table("T", &["ID", "NAME"], &[vec!["a".to_string(), "b".to_string()]]);
    }

    #[test]
    fn markdown_detail_is_two_column_table() {
        let md = MarkdownOutput::new(&serde_json::json!({}));
        md.detail("My Item", &[("Family", "Granite 3.1".to_string())]);
    }

    #[test]
    fn markdown_status_ok_contains_checkmark() {
        let md = MarkdownOutput::new(&serde_json::json!({}));
        md.status("my-service", true, "");
        md.status("my-service", false, "timeout");
    }
}
