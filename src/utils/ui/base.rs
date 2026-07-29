use std::any::Any;
use std::sync::LazyLock;

use crate::registry::ConfigConstructable;

/*-- public --*/

/// Metadata describing a registered UI backend.
#[derive(Debug, Clone)]
pub struct UiMetadata {
    pub name: String,
    pub description: String,
}

// Generate UiFactory and HasUiMetadata via the existing macro.
use crate::define_factory;
define_factory!(Ui, UiMetadata, UiFactory);

/// Global registry of UI backends.
/// Backends are registered by name and constructed on demand via --output flag.
pub static UI_REGISTRY: LazyLock<UiFactory> = LazyLock::new(|| {
    let mut f = UiFactory::new();
    f.register::<crate::utils::ui::backends::terminal::TerminalOutput>("terminal");
    f.register::<crate::utils::ui::backends::plain::PlainOutput>("plain");
    f.register::<crate::utils::ui::backends::json::JsonOutput>("json");
    f.register::<crate::utils::ui::backends::markdown::MarkdownOutput>("markdown");
    f
});

/// Shared error for backends (json, markdown) that render for scripting/docs
/// rather than a live session, and so can't sensibly block on interactive input.
pub(crate) fn non_interactive<T>() -> anyhow::Result<T> {
    anyhow::bail!("interactive prompts are not supported with this output format; rerun with --output=terminal or --output=plain")
}

/// Generates panic-safety contract tests for any [`Ui`] implementation.
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

/// The core UI abstraction: rendering output and prompting for input.
///
/// All command methods receive `ui: &dyn Ui` as their final parameter.
/// Command code never calls `println!` or `dialoguer` directly — it calls
/// these methods and the registered backend decides how to render and
/// whether/how to prompt.
///
/// `Any` is included as a supertrait so that `&dyn Ui` can be downcast
/// to concrete types at runtime via `downcast_ref::<T>()`. This enables
/// test inspection of `CaptureUi` fields and allows prod code to
/// access type-specific methods when necessary.
pub trait Ui: ConfigConstructable + Send + Sync + Any {
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

    /// Ask the user to pick one of `items`, returning the chosen index.
    fn select(&self, prompt: &str, items: &[String], default: usize) -> anyhow::Result<usize> {
        Ok(dialoguer::Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(default)
            .interact()?)
    }

    /// Ask a yes/no question.
    fn confirm(&self, prompt: &str, default: bool) -> anyhow::Result<bool> {
        Ok(dialoguer::Confirm::new()
            .with_prompt(prompt)
            .default(default)
            .interact()?)
    }

    /// Ask for a line of free text, pre-filled with `default`.
    fn text(&self, prompt: &str, default: &str) -> anyhow::Result<String> {
        Ok(dialoguer::Input::new()
            .with_prompt(prompt)
            .with_initial_text(default)
            .interact_text()?)
    }

    /// Ask for a secret value (masked input, not echoed).
    fn password(&self, prompt: &str) -> anyhow::Result<String> {
        Ok(dialoguer::Password::new()
            .with_prompt(prompt)
            .allow_empty_password(true)
            .interact()?)
    }
}

/*-- tests --*/

// NOTE: tests module is crate public for CaptureUi reuse
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// A test double that records every `Ui` call into inspectable `Vec`s,
    /// and answers `select`/`confirm`/`text`/`password` from canned queues
    /// (falling back to the method's own `default` when a queue is empty).
    ///
    /// Uses interior mutability (`RefCell`) because `Ui` methods take `&self`.
    /// Safe for single-threaded test code.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ui = CaptureUi::default();
    /// ModelCommands::catalog(&ctx, None, &ui).unwrap();
    /// let (title, headers, rows) = &ui.tables.borrow()[0];
    /// assert!(headers.contains(&"FAMILY".to_string()));
    /// ```
    #[derive(Default)]
    pub struct CaptureUi {
        /// (title, headers, rows) for each table() call
        pub tables: RefCell<Vec<(String, Vec<String>, Vec<Vec<String>>)>>,
        /// (title, fields) for each detail() call
        pub details: RefCell<Vec<(String, Vec<(String, String)>)>>,
        /// (label, ok, detail) for each status() call
        pub statuses: RefCell<Vec<(String, bool, String)>>,
        pub infos: RefCell<Vec<String>>,
        pub warns: RefCell<Vec<String>>,
        pub errors: RefCell<Vec<String>>,

        /// (prompt, items, default) for each select() call
        pub select_prompts: RefCell<Vec<(String, Vec<String>, usize)>>,
        /// (prompt, default) for each confirm() call
        pub confirm_prompts: RefCell<Vec<(String, bool)>>,
        /// (prompt, default) for each text() call
        pub text_prompts: RefCell<Vec<(String, String)>>,
        /// prompt for each password() call
        pub password_prompts: RefCell<Vec<String>>,

        /// Canned answers consumed in order by select(); falls back to `default` when empty.
        pub select_answers: RefCell<VecDeque<usize>>,
        /// Canned answers consumed in order by confirm(); falls back to `default` when empty.
        pub confirm_answers: RefCell<VecDeque<bool>>,
        /// Canned answers consumed in order by text(); falls back to `default` when empty.
        pub text_answers: RefCell<VecDeque<String>>,
        /// Canned answers consumed in order by password(); falls back to "" when empty.
        pub password_answers: RefCell<VecDeque<String>>,
    }

    impl ConfigConstructable for CaptureUi {
        fn new(_cfg: &serde_json::Value) -> Self {
            Self::default()
        }
    }

    impl Ui for CaptureUi {
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

        fn select(&self, prompt: &str, items: &[String], default: usize) -> anyhow::Result<usize> {
            self.select_prompts.borrow_mut().push((prompt.to_string(), items.to_vec(), default));
            Ok(self.select_answers.borrow_mut().pop_front().unwrap_or(default))
        }

        fn confirm(&self, prompt: &str, default: bool) -> anyhow::Result<bool> {
            self.confirm_prompts.borrow_mut().push((prompt.to_string(), default));
            Ok(self.confirm_answers.borrow_mut().pop_front().unwrap_or(default))
        }

        fn text(&self, prompt: &str, default: &str) -> anyhow::Result<String> {
            self.text_prompts.borrow_mut().push((prompt.to_string(), default.to_string()));
            Ok(self.text_answers.borrow_mut().pop_front().unwrap_or_else(|| default.to_string()))
        }

        fn password(&self, prompt: &str) -> anyhow::Result<String> {
            self.password_prompts.borrow_mut().push(prompt.to_string());
            Ok(self.password_answers.borrow_mut().pop_front().unwrap_or_default())
        }
    }

    // CaptureUi is single-threaded test-only code, but the Ui trait
    // requires Send + Sync so it can be used as &dyn Ui in command signatures.
    // RefCell is not Send; these impls assert that test code won't share the
    // capture across threads, which is always true in practice.
    unsafe impl Send for CaptureUi {}
    unsafe impl Sync for CaptureUi {}


    // -- UiFactory registry ------------------------------------------------

    #[test]
    fn ui_registry_contains_all_backends() {
        assert!(UI_REGISTRY.get("terminal").is_some());
        assert!(UI_REGISTRY.get("plain").is_some());
        assert!(UI_REGISTRY.get("json").is_some());
        assert!(UI_REGISTRY.get("markdown").is_some());
    }

    #[test]
    fn ui_registry_construct_unknown_returns_err() {
        let result = UI_REGISTRY.construct("nonexistent", &serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn ui_registry_has_exactly_four_backends() {
        assert_eq!(UI_REGISTRY.entries().len(), 4);
    }

    #[test]
    fn ui_metadata_has_non_empty_name_and_description() {
        for name in &["terminal", "plain", "json", "markdown"] {
            let meta = UI_REGISTRY.get(name).unwrap();
            assert!(!meta.name.is_empty(), "{} name empty", name);
            assert!(!meta.description.is_empty(), "{} description empty", name);
        }
    }

    // -- CaptureUi ---------------------------------------------------------

    fn make() -> CaptureUi {
        CaptureUi::default()
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

    // -- CaptureUi input methods -------------------------------------------

    #[test]
    fn select_falls_back_to_default_when_no_canned_answer() {
        let ui = make();
        let choice = ui.select("Pick one", &["a".to_string(), "b".to_string()], 1).unwrap();
        assert_eq!(choice, 1);
        assert_eq!(ui.select_prompts.borrow()[0].0, "Pick one");
    }

    #[test]
    fn select_consumes_canned_answers_in_order() {
        let ui = make();
        ui.select_answers.borrow_mut().extend([2, 0]);
        assert_eq!(ui.select("first", &["a".to_string()], 0).unwrap(), 2);
        assert_eq!(ui.select("second", &["a".to_string()], 0).unwrap(), 0);
    }

    #[test]
    fn confirm_falls_back_to_default_when_no_canned_answer() {
        let ui = make();
        assert_eq!(ui.confirm("Sure?", true).unwrap(), true);
    }

    #[test]
    fn confirm_consumes_canned_answer() {
        let ui = make();
        ui.confirm_answers.borrow_mut().push_back(false);
        assert_eq!(ui.confirm("Sure?", true).unwrap(), false);
    }

    #[test]
    fn text_falls_back_to_default_when_no_canned_answer() {
        let ui = make();
        assert_eq!(ui.text("Name?", "bob").unwrap(), "bob");
    }

    #[test]
    fn text_consumes_canned_answer() {
        let ui = make();
        ui.text_answers.borrow_mut().push_back("alice".to_string());
        assert_eq!(ui.text("Name?", "bob").unwrap(), "alice");
    }

    #[test]
    fn password_falls_back_to_empty_string_when_no_canned_answer() {
        let ui = make();
        assert_eq!(ui.password("Secret?").unwrap(), "");
    }

    #[test]
    fn password_consumes_canned_answer() {
        let ui = make();
        ui.password_answers.borrow_mut().push_back("s3cr3t".to_string());
        assert_eq!(ui.password("Secret?").unwrap(), "s3cr3t");
    }
}
