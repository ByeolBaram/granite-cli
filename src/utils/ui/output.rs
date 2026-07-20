use std::cell::RefCell;

/*-- public --*/

/// The core output abstraction.
///
/// All command methods receive `out: &dyn Output` as their final parameter.
/// Command code never calls `println!` directly — it calls these methods and
/// the registered backend decides how to render.
pub trait Output: Send + Sync {
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
