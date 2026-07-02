pub mod hardware;
pub mod web_fetch;

pub use hardware::{HardwareProfile, detect_hardware};
pub use web_fetch::{fetch_markdown, extract_urls};
