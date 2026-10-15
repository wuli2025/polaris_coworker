use super::*;
use protocol::{SessionOptions, ThinkingLevel};
use serde_json::json;

#[test]
fn mismatched_protocol_fuses_before_any_application_request() -> anyhow::Result<()> {
    let mut c = super::tests::config();
    let record = c.session_map.with_extension("requests.jsonl");
    c.args = vec!["--protocol-mismatch".into(), "--record".into(), record.to_string_lossy().into()];
    let b = PiBackend::new(c)?;
    let error = b.models().err().ok_or_else(|| anyhow::anyhow!("expected mismatch"))?;
    assert!(error.to_string().contains("AgentdProtocolMismatch"), "{error}");
    assert!(b.status().circuit_open);
    assert!(!b.status().healthy);
    assert_eq!(b.status().restarts, 0);
    let sent = std::fs::read_to_string(&record)?;
    assert_eq!(sent.lines().count(), 1);
    assert_eq!(serde_json::from_str::<Value>(&sent)?["method"], "ping");
    assert!(b.models().is_err());
    std::fs::remove_file(record)?;
    Ok(())
}

#[test]
fn model_profile_image_and_idle_reconfiguration_are_on_the_wire() -> anyhow::Result<()> {
    let mut c = super::tests::config();
    let record = c.session_map.with_extension("requests.jsonl");
    let uploads = c.session_map.with_extension("uploads");
    std::fs::create_dir_all(&uploads)?;
    let image = uploads.join("image.png");
    c.args = vec!["--record".into(), record.to_string_lossy().into()];
    std::fs::write(&image, b"\x89PNG\r\n\x1a\nfixture")?;
    let args: ChatSendArgs = serde_json::from_value(json!({
        "prompt": format!("Describe {}", image.display()), "permissionMode": "deny",
        "conversationId": "wire", "imagePaths": [image], "thinkingLevel": "low"
    }))?;
    let mut first = chat_input::prepare("wire", &args, &uploads, std::env::temp_dir(), &c.attachments)?;
    let b = PiBackend::new(c)?;
    first.model = Some(json!({"provider":"fixture","id":"test"}));
    std::fs::remove_file(&image)?;
    collect(&b, first, |_, _| {})?;
    b.configure_session("wire", SessionOptions { model: None,
        thinking_level: Some(ThinkingLevel::High), tool_profile: Some("full".into()), ..Default::default() })?;
    // A new turn has a distinct request ID, but reuses the mapped session.
    // The old preparation worker may still be cleaning up its request ID.
    let mut second = super::tests::input("wire-second", "normal");
    second.session_id = "wire".into();
    second.model = Some(Value::String("test".into()));
    second.permission = PermissionMode::Manual;
    second.thinking_level = Some(ThinkingLevel::Xhigh);
    // collect waits for done; Route::drain releases the session before done.
    assert!(!b.preparing.lock().contains("session:wire"));
    collect(&b, second, |_, _| {})?;
    // An acknowledged command is a recording barrier: the fixture fsyncs each
    // request before dispatching it, including both prompts and configurations.
    b.models()?;
    let sent: Vec<Value> = std::fs::read_to_string(&record)?.lines()
        .map(serde_json::from_str).collect::<Result<_, _>>()?;
    let open = sent.iter().find(|r| r["method"] == "session.open")
        .ok_or_else(|| anyhow::anyhow!("missing open"))?;
    assert_eq!(open["params"]["model"], json!({"provider":"fixture","id":"test"}));
    assert_eq!(open["params"]["toolProfile"], "readonly");
    assert_eq!(open["params"]["thinkingLevel"], "low");
    assert!(sent.iter().any(|r| r["method"] == "session.configure" && r["params"]["thinkingLevel"] == "high"));
    assert!(sent.iter().any(|r| r["method"] == "session.configure" && r["params"]["toolProfile"] == "workspace"));
    assert!(sent.iter().any(|r| r["method"] == "session.configure" && r["params"]["thinkingLevel"] == "xhigh"));
    let prompt = sent.iter().find(|r| r["method"] == "prompt")
        .ok_or_else(|| anyhow::anyhow!("missing prompt"))?;
    assert_eq!(prompt["params"]["attachments"][0]["mimeType"], "image/png");
    assert_eq!(prompt["params"]["attachments"][0]["type"], "image");
    assert_eq!(prompt["params"]["attachments"][0]["data"], "iVBORw0KGgpmaXh0dXJl");
    assert_eq!(prompt["params"]["text"], args.prompt);
    assert_eq!(b.status().agentd_version.as_deref(), Some("0.1.0"));
    b.shutdown();
    std::fs::remove_file(record)?;
    std::fs::remove_dir(uploads)?;
    Ok(())
}

#[test]
fn missing_or_noninteger_protocol_version_is_a_named_mismatch() {
    for version in [Value::Null, json!(3), json!(1), json!("2"), json!(2.1)] {
        let result = protocol::check_ping(json!({"status":"ok","protocolVersion":version}));
        assert!(result.err().is_some_and(|e| e.to_string().contains("AgentdProtocolMismatch")));
    }
}

#[test]
fn permission_confirmation_round_trip_preserves_call_and_parent() -> anyhow::Result<()> {
    for decision in [protocol::Decision::Allow, protocol::Decision::Deny, protocol::Decision::AllowAlways] {
        let backend = PiBackend::new(super::tests::config())?;
        let stream = backend.start(super::tests::input("approval", "permission"))?;
        let approval = loop {
            let event = stream.recv_timeout(super::test_support::wait_budget())?;
            if event.kind == "permission.request" { break event; }
            anyhow::ensure!(event.kind != "done" && event.kind != "error", "unexpected terminal");
        };
        let payload: Value = serde_json::from_str(approval.text.as_deref().ok_or_else(|| anyhow::anyhow!("missing approval"))?)?;
        assert_eq!(payload["callId"], "approve-call");
        assert_eq!(payload["parentCallId"], "parent-call");
        backend.permission_respond(protocol::PermissionResponse {
            session_id: payload["sessionId"].as_str().ok_or_else(|| anyhow::anyhow!("missing session"))?.into(),
            call_id: "approve-call".into(), decision: decision.clone(),
        })?;
        let mut text = String::new();
        let mut resolved = false;
        loop {
            let event = stream.recv_timeout(super::test_support::wait_budget())?;
            if event.kind == "permission.resolved" {
                let body: Value = serde_json::from_str(event.text.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("missing resolution"))?)?;
                assert_eq!(body["sessionId"], payload["sessionId"]);
                assert_eq!(body["callId"], "approve-call");
                assert_eq!(body["reason"], "responded");
                resolved = true;
            }
            if event.kind == "delta" { text.push_str(event.text.as_deref().unwrap_or_default()); }
            if event.kind == "done" { break; }
        }
        assert!(resolved, "resolution must reach frontend before done");
        assert_eq!(Value::String(text), serde_json::to_value(decision)?);
        backend.shutdown();
    }
    Ok(())
}

#[test]
fn explicit_tool_options_reach_session_open_without_legacy_context() -> anyhow::Result<()> {
    let mut config = super::tests::config();
    let record = config.session_map.with_extension("tools.jsonl");
    config.args = vec!["--record".into(), record.to_string_lossy().into()];
    let backend = PiBackend::new(config)?;
    let stream = backend.start_with_tools(super::tests::input("tools", "normal"), ToolOptions {
        sandbox: true, expert_prompt: Some("expert-section".into()), ..Default::default()
    })?;
    while stream.recv_timeout(super::test_support::wait_budget())?.kind != "done" {}
    let sent: Vec<Value> = std::fs::read_to_string(&record)?.lines()
        .map(serde_json::from_str).collect::<Result<_, _>>()?;
    let open = sent.iter().find(|r| r["method"] == "session.open").ok_or_else(|| anyhow::anyhow!("missing open"))?;
    assert_eq!(open["params"]["sandbox"], true);
    assert_eq!(open["params"]["expertPrompt"], "expert-section");
    assert_eq!(open["params"]["permissionMode"], "deny");
    backend.shutdown();
    std::fs::remove_file(record)?;
    Ok(())
}

#[test]
fn selected_expert_is_loaded_into_this_turn_without_project_writes() -> anyhow::Result<()> {
    struct Loader;
    impl host_tools::ExpertLoader for Loader {
        fn load(&self, selection: &super::super::expert_selection::ExpertSelection) -> anyhow::Result<String> {
            use super::super::expert_selection::ExpertSelection;
            match selection {
                ExpertSelection::Expert { id } if id == "qa" => Ok("QA full prompt".into()),
                ExpertSelection::Team { id } if id == "review" => Ok("Review team full prompt".into()),
                _ => anyhow::bail!("unknown selection"),
            }
        }
    }
    for (kind, id, prompt) in [("expert", "qa", "QA full prompt"), ("team", "review", "Review team full prompt")] {
        let mut config = super::tests::config();
        let project = config.session_map.with_extension("project");
        let record = config.session_map.with_extension("expert.jsonl");
        std::fs::create_dir_all(&project)?;
        for name in ["CLAUDE.md", "AGENTS.md"] { std::fs::write(project.join(name), "user content")?; }
        config.args = vec!["--record".into(), record.to_string_lossy().into()];
        let backend = PiBackend::new(config)?;
        let args: ChatSendArgs = serde_json::from_value(json!({
            "prompt":"task", "permissionMode":"deny", "agentMode":"single-expert",
            "expertSelection":{"kind":kind,"id":id}
        }))?;
        let expert_prompt = host_tools::expert_prompt_with(&args, Some(&Loader))?;
        let mut input = super::tests::input("selected-expert", "normal");
        input.cwd = project.clone();
        let stream = backend.start_with_tools(input, ToolOptions { sandbox: false, expert_prompt, ..Default::default() })?;
        while stream.recv_timeout(super::test_support::wait_budget())?.kind != "done" {}
        let sent: Vec<Value> = std::fs::read_to_string(&record)?.lines()
            .map(serde_json::from_str).collect::<Result<_, _>>()?;
        let open = sent.iter().find(|r| r["method"] == "session.open")
            .ok_or_else(|| anyhow::anyhow!("missing open"))?;
        assert_eq!(open["params"]["expertPrompt"], prompt);
        for name in ["CLAUDE.md", "AGENTS.md"] { assert_eq!(std::fs::read_to_string(project.join(name))?, "user content"); }
        backend.shutdown();
        std::fs::remove_file(record)?;
        std::fs::remove_dir_all(project)?;
    }
    Ok(())
}
