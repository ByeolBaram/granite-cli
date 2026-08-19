pub mod hardware;
pub mod shell;
pub mod subserver;
pub mod traits;
pub mod ui;

pub use hardware::{HardwareProfile, detect_hardware};
pub use shell::resolve_shell_command;
pub use traits::Searchable;
pub use ui::prompt_from_schema;
