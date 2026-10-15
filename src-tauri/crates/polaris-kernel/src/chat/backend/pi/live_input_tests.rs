use super::{PiBackend, tests::{config, input}, test_support::{wait_budget, wait_until}};
use crate::chat::backend::AgentBackend;

#[test]
fn live_input_uses_active_session_and_rejects_finished_request() -> anyhow::Result<()> {
    let config = config();
    let dir = config.session_map.clone();
    let backend = PiBackend::new(config)?;
    backend.models()?;
    let stream = backend.start(input("running", "permission"))?;
    let payload = loop {
        let event = stream.recv_timeout(wait_budget())?;
        if event.kind == "permission.request" {
            break serde_json::from_str::<serde_json::Value>(event.text.as_deref().ok_or_else(|| anyhow::anyhow!("missing permission"))?)?;
        }
    };
    // The protocol permits events before the prompt response; wait on the actor's named readiness result.
    let receipt = std::cell::RefCell::new(None);
    wait_until("live-input route bound", || match backend.live_input("running", "steering", false) {
        Ok(value) => { *receipt.borrow_mut() = Some(value); true }
        Err(error) => { assert_eq!(error.to_string(), "ChatTurnNotReady"); false }
    })?;
    let receipt = receipt.into_inner().ok_or_else(|| anyhow::anyhow!("missing receipt"))?;
    let queued = backend.live_input("running", "following", true)?;
    assert_eq!(receipt["turnId"], queued["turnId"]);
    assert_eq!(queued["disposition"], "queued");
    assert!(backend.live_input("other", "wrong", false).is_err());
    backend.permission_respond(super::protocol::PermissionResponse {
        session_id: payload["sessionId"].as_str().ok_or_else(|| anyhow::anyhow!("missing session"))?.into(),
        call_id: payload["callId"].as_str().ok_or_else(|| anyhow::anyhow!("missing call"))?.into(),
        decision: super::protocol::Decision::Allow,
    })?;
    while stream.recv_timeout(wait_budget())?.kind != "done" {}
    for follow_up in [false, true] {
        let error = backend.live_input("running", "late", follow_up).err().ok_or_else(|| anyhow::anyhow!("late input accepted"))?;
        assert_eq!(error.to_string(), "ChatNotRunning");
    }
    backend.shutdown();
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
