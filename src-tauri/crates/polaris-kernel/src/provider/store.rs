//! Provider store facade; public command paths are preserved.
use super::*;
#[path = "store/bundled.rs"]
pub mod bundled;
#[path = "store/presets.rs"]
mod presets;
use presets::*;
#[path = "store/state.rs"]
mod state;
pub use state::*;
#[path = "store/failover.rs"]
mod failover;
pub use failover::*;
#[path = "store/gifts.rs"]
mod gifts;
pub use gifts::*;
#[path = "store/routing.rs"]
mod routing;
pub use routing::*;
#[path = "store/persistence.rs"]
mod persistence;
pub use persistence::*;
#[path = "store/secrets.rs"]
mod secrets;
use secrets::*;
#[path = "store/views.rs"]
mod views;
pub use views::*;
#[path = "store/global.rs"]
mod global;
use global::*;
#[path = "store/listing.rs"]
mod listing;
pub use listing::*;
#[path = "store/child.rs"]
mod child;
pub use child::*;
#[path = "store/commands.rs"]
mod commands;
pub use commands::*;
#[cfg(test)]
#[path = "store/tests.rs"]
mod tests;

#[path = "store/credential_migration.rs"]
mod credential_migration;
use credential_migration::*;
