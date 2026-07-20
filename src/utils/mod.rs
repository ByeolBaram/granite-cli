pub mod hardware;
pub mod schema_prompt;

pub use hardware::{HardwareProfile, detect_hardware};
pub use schema_prompt::prompt_from_schema;
