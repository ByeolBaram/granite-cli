pub mod hardware;
pub mod ui;

pub use hardware::{HardwareProfile, detect_hardware};
pub use ui::prompt_from_schema;
