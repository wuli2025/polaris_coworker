//! Short state lock and generation fence; daemon I/O must never hold this lock.
use super::*;
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
// Caller holds PROVIDER_MUTATION while editing selection/configuration state.
pub(crate) fn changed() { GENERATION.fetch_add(1, Ordering::SeqCst); }
pub(crate) fn snapshot() -> (u64, String) {
    let _state = PROVIDER_MUTATION.lock();
    let id = current_provider_id();
    (GENERATION.load(Ordering::SeqCst), if id.is_empty() { store::bundled::ID.into() } else { id })
}
pub(crate) fn current(generation: u64) -> bool { GENERATION.load(Ordering::SeqCst) == generation }
