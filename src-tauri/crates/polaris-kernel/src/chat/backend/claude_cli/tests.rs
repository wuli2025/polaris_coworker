use super::*;
use std::{process::Command, time::Instant};
fn fixture() -> Command {
    let mut command = Command::new("node");
    command.arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-claude.mjs"));
    command
}
#[test]
fn readonly_preserves_text_and_legacy_result_deduplication() {
    let mut events = vec![];
    let text = headless::run_command(&std::env::temp_dir(), "normal",
        |kind, text| events.push((kind.to_owned(), text.to_owned())),
        Some(Duration::from_secs(2)), "fixture-readonly", fixture()).unwrap();
    assert_eq!(text, " hello \nworld ");
    assert_eq!(events, [("tool".into(), "Read".into()), ("delta".into(), text)]);
}
#[test]
fn readonly_can_be_cancelled_through_backend() {
    let handle = std::thread::spawn(|| headless::run_command(&std::env::temp_dir(), "hang", |_,_| {},
        Some(Duration::from_secs(3)), "fixture-cancel", fixture()));
    let deadline = Instant::now() + Duration::from_secs(2);
    while !CHILDREN.lock().contains_key("fixture-cancel") {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    ClaudeCliBackend.cancel("fixture-cancel").unwrap();
    assert_eq!(handle.join().unwrap().unwrap(), "");
    assert!(!CHILDREN.lock().contains_key("fixture-cancel"));
}
