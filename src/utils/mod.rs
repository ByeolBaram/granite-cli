pub mod hardware;
pub mod traits;
pub mod ui;

pub use hardware::{HardwareProfile, detect_hardware};
pub use traits::Searchable;
pub use ui::prompt_from_schema;
