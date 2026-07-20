use std::cell::RefCell;
use std::sync::LazyLock;

use crate::registry::ConfigConstructable;

/*-- public --*/

/// Metadata describing a registered output backend.
#[derive(Debug, Clone)]
pub struct OutputMetadata {
    pub name: String,
    pub description: String,
}

// Generate OutputFactory and HasOutputMetadata via the existing macro.
use crate::define_factory;
define_factory!(Output, OutputMetadata, OutputFactory);

/// Global registry of output backends.
/// Backends are registered by name and constructed on demand via --output flag.
pub static OUTPUT_REGISTRY: LazyLock<OutputFactory> = LazyLock::new(|| {
    let mut f = OutputFactory::new();
    f.register::<crate::utils::ui::backends::terminal::TerminalOutput>("terminal");
    f.register::<crate::utils::ui::backends::plain::PlainOutput>("plain");
    f.register::<crate::utils::ui::backends::json::JsonOutput>("json");
    f
});

/// Generates panic-safety contract tests for any [`Output`] implementation.
/// Invoke with the constructor expression as argument:
///
/// ```ignore
/// output_contract_tests!(PlainOutput::new(&serde_json::json!({})));
/// ```
#[macro_export]
macro_rules! output_contract_tests {
    ($make:expr) => {
        #[test]
        fn contract_empty_table_does_not_panic() {
            $make.table("T", &["A"], &[]);
        }
        #[test]
        fn contract_single_row_table_does_not_panic() {
            $make.table("T", &["A"], &[vec!["x".to_string()]]);
        }
        #[test]
        fn contract_hundred_row_table_does_not_panic() {
            let rows: Vec<Vec<String>> = (0..100)
                .map(|i| vec![format!("id-{}", i), format!("val-{}", i)])
                .collect();
            $make.table("Big", &["ID", "VAL"], &rows);
        }
        #[test]
        fn contract_table_with_empty_cell_does_not_panic() {
            $make.table("T", &["A"], &[vec!["".to_string()]]);
        }
        #[test]
        fn contract_detail_with_no_fields_does_not_panic() {
            $make.detail("Empty", &[]);
        }
        #[test]
        fn contract_status_ok_does_not_panic() {
            $make.status("svc", true, "");
        }
        #[test]
        fn contract_status_fail_does_not_panic() {
            $make.status("svc", false, "timed out");
        }
        #[test]
        fn contract_info_empty_string_does_not_panic() {
            $make.info("");
            $make.warn("");
            $make.error("");
        }
    };
}

/// The core output abstraction.
///
/// All command methods receive `out: &dyn Output` as their final parameter.
/// Command code never calls `println!` directly — it calls these methods and
/// the registered backend decides how to render.
pub trait Output: ConfigConstructable + Send + Sync {
    /// Render a tabular result (catalog, list, health).
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]);

    /// Render a key-value detail block (info commands).
    fn detail(&self, title: &str, fields: &[(&str, String)]);

    /// Render a single status row with pass/fail indicator (health checks).
    fn status(&self, label: &str, ok: bool, detail: &str);

    /// Plain informational message.
    fn info(&self, msg: &str);

    /// Warning message.
    fn warn(&self, msg: &str);

    /// Error message. Implementations should route this to stderr.
    fn error(&self, msg: &str);
}

/// A test double that records every `Output` call into inspectable `Vec`s.
///
/// Uses interior mutability (`RefCell`) because `Output` methods take `&self`.
/// Safe for single-threaded test code.
///
/// # Example
///
/// ```ignore
/// let out = CaptureOutput::default();
/// ModelCommands::catalog(&ctx, None, &out).unwrap();
/// let (title, headers, rows) = &out.tables.borrow()[0];
/// assert!(headers.contains(&"FAMILY".to_string()));
/// ```
#[derive(Default)]
pub struct CaptureOutput {
    /// (title, headers, rows) for each table() call
    pub tables: RefCell<Vec<(String, Vec<String>, Vec<Vec<String>>)>>,
    /// (title, fields) for each detail() call
    pub details: RefCell<Vec<(String, Vec<(String, String)>)>>,
    /// (label, ok, detail) for each status() call
    pub statuses: RefCell<Vec<(String, bool, String)>>,
    pub infos: RefCell<Vec<String>>,
    pub warns: RefCell<Vec<String>>,
    pub errors: RefCell<Vec<String>>,
}

impl ConfigConstructable for CaptureOutput {
    fn new(_cfg: &serde_json::Value) -> Self {
        Self::default()
    }
}

impl Output for CaptureOutput {
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) {
        self.tables.borrow_mut().push((
            title.to_string(),
            headers.iter().map(|h| h.to_string()).collect(),
            rows.to_vec(),
        ));
    }

    fn detail(&self, title: &str, fields: &[(&str, String)]) {
        self.details.borrow_mut().push((
            title.to_string(),
            fields.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        ));
    }

    fn status(&self, label: &str, ok: bool, detail: &str) {
        self.statuses
            .borrow_mut()
            .push((label.to_string(), ok, detail.to_string()));
    }

    fn info(&self, msg: &str) {
        self.infos.borrow_mut().push(msg.to_string());
    }

    fn warn(&self, msg: &str) {
        self.warns.borrow_mut().push(msg.to_string());
    }

    fn error(&self, msg: &str) {
        self.errors.borrow_mut().push(msg.to_string());
    }
}

// CaptureOutput is single-threaded test-only code, but the Output trait
// requires Send + Sync so it can be used as &dyn Output in command signatures.
// RefCell is not Send; these impls assert that test code won't share the
// capture across threads, which is always true in practice.
unsafe impl Send for CaptureOutput {}
unsafe impl Sync for CaptureOutput {}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;

    // ── OutputFactory registry ────────────────────────────────────────────────

    #[test]
    fn output_registry_contains_all_three_backends() {
        assert!(OUTPUT_REGISTRY.get("terminal").is_some());
        assert!(OUTPUT_REGISTRY.get("plain").is_some());
        assert!(OUTPUT_REGISTRY.get("json").is_some());
    }

    #[test]
    fn output_registry_construct_unknown_returns_err() {
        let result = OUTPUT_REGISTRY.construct("nonexistent", &serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn output_registry_has_exactly_three_backends() {
        assert_eq!(OUTPUT_REGISTRY.entries().len(), 3);
    }

    #[test]
    fn output_metadata_has_non_empty_name_and_description() {
        for name in &["terminal", "plain", "json"] {
            let meta = OUTPUT_REGISTRY.get(name).unwrap();
            assert!(!meta.name.is_empty(), "{} name empty", name);
            assert!(!meta.description.is_empty(), "{} description empty", name);
        }
    }

    // ── CaptureOutput ─────────────────────────────────────────────────────────

    fn make() -> CaptureOutput {
        CaptureOutput::default()
    }

    #[test]
    fn capture_default_starts_with_all_vecs_empty() {
        let out = make();
        assert!(out.tables.borrow().is_empty());
        assert!(out.details.borrow().is_empty());
        assert!(out.statuses.borrow().is_empty());
        assert!(out.infos.borrow().is_empty());
        assert!(out.warns.borrow().is_empty());
        assert!(out.errors.borrow().is_empty());
    }

    #[test]
    fn capture_records_table_title_headers_rows() {
        let out = make();
        out.table(
            "My Table",
            &["A", "B"],
            &[vec!["r1a".to_string(), "r1b".to_string()]],
        );
        let tables = out.tables.borrow();
        assert_eq!(tables.len(), 1);
        let (title, headers, rows) = &tables[0];
        assert_eq!(title, "My Table");
        assert_eq!(headers, &["A".to_string(), "B".to_string()]);
        assert_eq!(rows[0], vec!["r1a".to_string(), "r1b".to_string()]);
    }

    #[test]
    fn capture_records_multiple_tables_in_order() {
        let out = make();
        out.table("T1", &["X"], &[vec!["a".to_string()]]);
        out.table("T2", &["Y"], &[vec!["b".to_string()]]);
        let tables = out.tables.borrow();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].0, "T1");
        assert_eq!(tables[1].0, "T2");
    }

    #[test]
    fn capture_records_detail_title_and_field_pairs() {
        let out = make();
        out.detail("Item", &[("Key", "Value".to_string()), ("Foo", "Bar".to_string())]);
        let details = out.details.borrow();
        assert_eq!(details.len(), 1);
        let (title, fields) = &details[0];
        assert_eq!(title, "Item");
        assert_eq!(fields[0], ("Key".to_string(), "Value".to_string()));
        assert_eq!(fields[1], ("Foo".to_string(), "Bar".to_string()));
    }

    #[test]
    fn capture_records_info_warn_error_to_separate_vecs() {
        let out = make();
        out.info("hello");
        out.warn("careful");
        out.error("boom");
        assert_eq!(*out.infos.borrow(), vec!["hello"]);
        assert_eq!(*out.warns.borrow(), vec!["careful"]);
        assert_eq!(*out.errors.borrow(), vec!["boom"]);
    }

    #[test]
    fn capture_records_status_with_ok_flag_and_detail() {
        let out = make();
        out.status("provider-a", true, "");
        out.status("provider-b", false, "connection refused");
        let statuses = out.statuses.borrow();
        assert_eq!(statuses[0], ("provider-a".to_string(), true, "".to_string()));
        assert_eq!(statuses[1], ("provider-b".to_string(), false, "connection refused".to_string()));
    }
}
