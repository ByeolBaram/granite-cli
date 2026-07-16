pub mod hardware;
pub mod schema_prompt;
pub mod web_fetch;

pub use hardware::{HardwareProfile, detect_hardware};
pub use schema_prompt::prompt_from_schema;
pub use web_fetch::{fetch_markdown, extract_urls};
