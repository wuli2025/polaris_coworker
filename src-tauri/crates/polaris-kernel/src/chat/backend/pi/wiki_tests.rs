//! Public headless extension preserves arguments without changing UI events.
use super::*;
use protocol::{Event, TurnStatus};
use serde_json::json;
#[test]
fn detailed_tools_and_abort_status_are_opt_in() -> anyhow::Result<()> {
    for details in [false, true] {
        let mut translator = translate::Translator::new("wiki".into(), None,
            std::env::temp_dir(), &config::Agentd::default()).with_details(details);
        let started = translator.accept(Event::ToolStarted {
            call_id: "write-1".into(), tool: "write".into(), args: json!({"path":"concepts/test.md"}), parent_call_id: None,
        });
        let detailed = started.iter().find(|e| e.kind == "tool.started");
        assert_eq!(detailed.is_some(), details);
        if let Some(event) = detailed {
            let data: Value = serde_json::from_str(event.text.as_deref().unwrap_or_default())?;
            assert_eq!(data["args"]["path"], "concepts/test.md");
            assert_eq!(data["callId"], "write-1");
        }
        let completed = translator.accept(Event::ToolCompleted { call_id: "write-1".into(), tool: "write".into(),
            result: json!({}), is_error: false, parent_call_id: None });
        assert_eq!(completed.iter().any(|e| e.kind == "tool.completed"), details);
        let done = translator.accept(Event::Completed { status: TurnStatus::Aborted });
        assert_eq!(done.last().and_then(|e| e.text.as_deref()), details.then_some("Aborted"));
    }
    Ok(())
}
#[test]
fn wiki_profile_permission_and_sandbox_reach_session_open() -> anyhow::Result<()> {
    let mut config = super::tests::config();
    let record = config.session_map.with_extension("wiki-requests.jsonl");
    config.args = vec!["--record".into(), record.to_string_lossy().into()];
    let backend = PiBackend::new(config)?;
    let input = TurnRequest::headless("wiki-options".into(), std::env::temp_dir(), "normal".into(), PermissionMode::AutoCurrent);
    let stream = backend.start_with_tools(input, ToolOptions {
        sandbox: true, tool_profile: Some("wiki-custom".into()), detailed_events: true, ..Default::default()
    })?;
    loop { if stream.recv_timeout(super::test_support::wait_budget())?.kind == "done" { break; } }
    backend.models()?;
    let sent: Vec<Value> = std::fs::read_to_string(&record)?.lines().map(serde_json::from_str).collect::<Result<_, _>>()?;
    let open = sent.iter().find(|v| v["method"] == "session.open").ok_or_else(|| anyhow::anyhow!("no session open"))?;
    assert_eq!(open["params"]["cwd"], std::fs::canonicalize(std::env::temp_dir())?.to_string_lossy().as_ref());
    assert_eq!(open["params"]["permissionMode"], "auto-current");
    assert_eq!(open["params"]["toolProfile"], "wiki-custom");
    assert_eq!(open["params"]["sandbox"], true);
    backend.close("wiki-options")?;
    backend.shutdown();
    std::fs::remove_file(record)?;
    Ok(())
}
