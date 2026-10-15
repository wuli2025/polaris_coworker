use super::*;

fn fixture() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("context-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[test]
fn selection_priority_and_missing_policy() -> Result<(), Box<dyn std::error::Error>> {
    for (agents, claude, expected) in [(false, false, "AGENTS.md"), (false, true, "CLAUDE.md"),
        (true, false, "AGENTS.md"), (true, true, "AGENTS.md")] {
        let dir = fixture()?;
        if agents { fs::write(dir.join("AGENTS.md"), "agents")?; }
        if claude { fs::write(dir.join("CLAUDE.md"), "claude")?; }
        let selected = select(&dir, "AGENTS.md")?;
        assert_eq!(selected.file_name, expected);
        assert_eq!(selected.agents_exists, agents);
        assert_eq!(selected.claude_exists, claude);
        assert_eq!(selected.relative_path, expected);
        if selected.exists { assert!(!read(&dir, &selected)?.is_empty()); }
        else {
            assert!(read(&dir, &selected).is_err_and(|e| e.starts_with("ContextNotFound")));
            let alternate = select(&dir, "CLAUDE.md")?;
            assert_eq!(alternate.file_name, "CLAUDE.md");
            write(&dir, &alternate, "explicit legacy creation")?;
            assert!(!dir.join("AGENTS.md").exists());
            write(&dir, &selected, "default creation")?;
            assert_eq!(read(&dir, &selected)?, "default creation");
            assert_eq!(read(&dir, &alternate)?, "explicit legacy creation");
        }
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn writing_pins_selection_even_after_priority_changes() -> Result<(), Box<dyn std::error::Error>> {
    let dir = fixture()?;
    fs::write(dir.join("CLAUDE.md"), "old")?;
    let selected = select(&dir, "AGENTS.md")?;
    fs::write(dir.join("AGENTS.md"), "keep")?;
    write(&dir, &selected, "changed")?;
    assert_eq!(fs::read_to_string(dir.join("CLAUDE.md"))?, "changed");
    assert_eq!(fs::read_to_string(dir.join("AGENTS.md"))?, "keep");
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn paths_and_stale_roots_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let dir = fixture()?;
    let other = fixture()?;
    let mut selected = select(&dir, "AGENTS.md")?;
    assert!(write(&other, &selected, "bad").is_err());
    for name in ["../AGENTS.md", "/tmp/CLAUDE.md", "sub/AGENTS.md", "invalid"] {
        assert!(select(&dir, name).is_err());
        selected.file_name = name.into();
        selected.relative_path = name.into();
        assert!(write(&dir, &selected, "bad").is_err());
    }
    assert!(select(&dir.join(".."), "AGENTS.md").is_err());
    fs::remove_dir_all(dir)?;
    fs::remove_dir_all(other)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn permission_denied_is_not_missing_and_links_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = fixture()?;
    let file = dir.join("AGENTS.md");
    fs::write(&file, "secret")?;
    fs::write(dir.join("CLAUDE.md"), "fallback must not be read")?;
    let selected = select(&dir, "AGENTS.md")?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0))?;
    let result = read(&dir, &selected);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600))?;
    assert!(result.is_err_and(|e| e.starts_with("ContextPermissionDenied")));
    fs::remove_file(&file)?;
    symlink("/tmp/outside-context-missing", &file)?;
    assert!(select(&dir, "AGENTS.md").is_err());
    assert!(write(&dir, &selected, "bad").is_err());
    fs::remove_dir_all(dir)?;
    Ok(())
}
