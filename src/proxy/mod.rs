//! Localhost usage-tracking reverse proxy, enabled by `--usage-tracking` on
//! `granite-cli launch`. See `docs/specs/0020-usage-tracking-proxy.md`.

mod model_wrapper;
pub use model_wrapper::{UsageTrackingContext, UsageTrackingModel};

mod server;
pub use server::ProxyServer;

mod usage;
pub use usage::{UsageStats, UsageTracker};
