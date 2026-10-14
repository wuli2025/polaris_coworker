//! Compatibility facade for the legacy Claude pool and backend lifecycle.
pub use super::backend::claude_cli::pool::{cancel_by_req, is_active_req, prewarm};
pub fn kill_conv(id: &str) { super::backend::close_session(id); }
pub fn kill_all() { super::backend::shutdown(); }
