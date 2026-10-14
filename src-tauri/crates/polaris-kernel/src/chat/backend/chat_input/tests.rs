use super::*;
use super::super::pi::protocol::Attachment;
use base64::Engine;
use serde_json::json;
use std::fs;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> anyhow::Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!("chat-input-{}-{}", std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        fs::create_dir_all(root.join("uploads"))?;
        Ok(Self(root))
    }
    fn uploads(&self) -> PathBuf { self.0.join("uploads") }
    fn prepare(&self, args: &ChatSendArgs, limits: &images::ImageLimits) -> anyhow::Result<TurnRequest> {
        prepare("request", args, &self.uploads(), self.0.clone(), limits)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[test]
fn chat_json_becomes_encoded_turn_without_removing_prompt_paths() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let path = f.uploads().join("image.png");
    let bytes = b"\x89PNG\r\n\x1a\nfixture";
    fs::write(&path, bytes)?;
    let prompt = format!("Describe {}", path.display());
    let args: ChatSendArgs = serde_json::from_value(json!({
        "prompt": prompt, "permissionMode": "auto_current", "conversationId": "conversation",
        "imagePaths": [path], "thinkingLevel": "high"
    }))?;
    let input = f.prepare(&args, &images::ImageLimits::default())?;
    assert_eq!(input.prompt, prompt);
    assert_eq!(input.req_id, "request");
    assert_eq!(input.session_id, "conversation");
    assert_eq!(input.conversation_id.as_deref(), Some("conversation"));
    assert!(matches!(input.permission, PermissionMode::AutoCurrent));
    assert_eq!(input.thinking_level, Some(ThinkingLevel::High));
    assert_eq!(input.attachments.len(), 1);
    // The TurnRequest owns the bytes; later filesystem changes cannot change the image sent.
    fs::remove_file(path)?;
    let Attachment::Image { mime_type, data } = &input.attachments[0];
    assert_eq!(mime_type, "image/png");
    assert_eq!(base64::engine::general_purpose::STANDARD.decode(data)?, bytes);
    Ok(())
}

#[test]
fn omitted_null_and_empty_inputs_preserve_old_text_callers() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    fs::remove_dir(f.uploads())?;
    for extra in [json!({}), json!({"imagePaths": null, "thinkingLevel": null}), json!({"imagePaths": []})] {
        let mut value = json!({"prompt": "Read /old/picture.png", "permissionMode": "deny"});
        if let Some(fields) = extra.as_object() {
            for (key, value_extra) in fields { value[key] = value_extra.clone(); }
        }
        let args: ChatSendArgs = serde_json::from_value(value)?;
        let input = f.prepare(&args, &images::ImageLimits::default())?;
        assert!(input.attachments.is_empty());
        assert!(input.thinking_level.is_none());
        assert_eq!(input.prompt, args.prompt);
        assert_eq!(input.session_id, "request");
    }
    Ok(())
}

#[test]
fn validates_all_thinking_levels_and_rejects_unknown_values() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    for level in ["off", "minimal", "low", "medium", "high", "xhigh", "", "HIGH", "auto", " high "] {
        let args: ChatSendArgs = serde_json::from_value(json!({
            "prompt": "test", "permissionMode": "manual", "thinkingLevel": level
        }))?;
        let result = f.prepare(&args, &images::ImageLimits::default());
        if ["off", "minimal", "low", "medium", "high", "xhigh"].contains(&level) {
            assert_eq!(serde_json::to_value(result?.thinking_level)?, json!(level));
        } else {
            let error = result.err().ok_or_else(|| anyhow::anyhow!("accepted invalid thinking level"))?;
            assert!(error.to_string().starts_with("THINKING_LEVEL_INVALID"));
        }
    }
    Ok(())
}

#[test]
fn chat_boundary_propagates_named_image_errors_and_configured_limits() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let inside = f.uploads().join("image.png");
    let outside = f.0.join("outside.png");
    let wrong_type = f.uploads().join("fake.png");
    fs::write(&inside, b"\x89PNG\r\n\x1a\n")?;
    fs::write(&outside, b"\x89PNG\r\n\x1a\n")?;
    fs::write(&wrong_type, b"text")?;
    for (paths, code) in [
        (vec![outside], "ATTACHMENT_PATH_OUTSIDE_UPLOADS"),
        (vec![f.uploads().join("../uploads/image.png")], "ATTACHMENT_PATH_INVALID"),
        (vec![wrong_type], "ATTACHMENT_TYPE_UNSUPPORTED"),
        (vec![inside.clone(); 5], "ATTACHMENT_COUNT_LIMIT"),
    ] {
        let args: ChatSendArgs = serde_json::from_value(json!({
            "prompt": "test", "permissionMode": "deny", "imagePaths": paths
        }))?;
        let error = f.prepare(&args, &images::ImageLimits::default()).err()
            .ok_or_else(|| anyhow::anyhow!("expected {code}"))?;
        assert!(error.to_string().starts_with(code), "{error}");
    }
    let args = serde_json::from_value(json!({
        "prompt": "test", "permissionMode": "deny", "imagePaths": [inside]
    }))?;
    let limits = images::ImageLimits { max_bytes: 7, ..Default::default() };
    let error = f.prepare(&args, &limits).err().ok_or_else(|| anyhow::anyhow!("expected size rejection"))?;
    assert!(error.to_string().starts_with("ATTACHMENT_SIZE_LIMIT"));
    Ok(())
}
