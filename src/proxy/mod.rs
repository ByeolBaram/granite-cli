//! Localhost usage-tracking reverse proxy, enabled by `--usage-tracking` on
//! `granite-cli launch`. See `docs/specs/0020-usage-tracking-proxy.md`.

mod capability_wrapper;
pub use capability_wrapper::UsageTrackingCapability;

mod server;
pub use server::ProxyServer;

mod usage;
pub use usage::{UsageStats, UsageTracker};
