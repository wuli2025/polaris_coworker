use super::*;

#[test]
fn windows_resolution_bundle_system_git_and_missing() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("polaris-pi-shell-{}", std::process::id()));
    let bundle = root.join("runtime");
    let bash = bundle.join("git/usr/bin/bash.exe");
    let pf = root.join("Program Files");
    let x86 = root.join("Program Files (x86)");
    let git = pf.join("Git/bin/bash.exe");
    let git_x86 = x86.join("Git/bin/bash.exe");
    for path in [&bash, &git, &git_x86] {
        std::fs::create_dir_all(path.parent().ok_or("missing parent")?)?;
        std::fs::write(path, "fixture")?;
    }
    let system_root = root.join("Windows");
    let choose = || windows_shell(super::super::bundled::git_bash_at(&bundle),
        [pf.clone(), x86.clone()], Vec::new, &system_root);
    assert_eq!(choose(), Some(ResolvedShell { path: bash.clone(), source: ShellSource::Bundled }));
    std::fs::remove_file(&bash)?;
    assert_eq!(choose(), Some(ResolvedShell { path: git.clone(), source: ShellSource::ProgramFiles }));
    std::fs::remove_file(&git)?;
    assert_eq!(choose(), Some(ResolvedShell { path: git_x86.clone(), source: ShellSource::ProgramFiles }));
    std::fs::remove_file(&git_x86)?;
    assert_eq!(choose(), None);
    let path_bash = root.join("PATH/bash.exe");
    assert_eq!(windows_shell(None, [pf.clone()], || vec![path_bash.clone()], &system_root), None);
    std::fs::create_dir_all(path_bash.parent().ok_or("missing PATH parent")?)?;
    std::fs::write(&path_bash, "fixture")?;
    assert_eq!(windows_shell(None, [pf], || vec![path_bash.clone()], &system_root), Some(ResolvedShell { path: path_bash, source: ShellSource::Path }));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn child_environment_precedes_system_path_without_changing_parent() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir();
    let unix = root.join("Min Git/usr/bin");
    let mingw = root.join("Min Git/mingw64/bin");
    let system = root.join("Windows/System32");
    let bash = unix.join("bash.exe");
    let parent = std::env::var_os("PATH");
    let current = std::env::join_paths([&system, &unix])?;
    let mut command = std::process::Command::new("node");
    apply_environment(&mut command, &bash, vec![unix.clone(), mingw.clone()], Some(current))?;
    let env: std::collections::HashMap<_, _> = command.get_envs().collect();
    assert_eq!(env.get(std::ffi::OsStr::new("POLARIS_AGENTD_SHELL_PATH")), Some(&Some(bash.as_os_str())));
    let path = env.get(std::ffi::OsStr::new("PATH")).copied().flatten().ok_or("missing PATH")?;
    assert_eq!(std::env::split_paths(path).collect::<Vec<_>>(), vec![unix, mingw, system]);
    assert_eq!(std::env::var_os("PATH"), parent);
    Ok(())
}

#[test]
fn explicit_config_matches_doctor_and_invalid_path_does_not_fall_back() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("polaris-shell-config-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let config_file = root.join("agentd.json");
    let shell = root.join("custom-bash");
    std::fs::write(&shell, "fixture")?;
    let mut config = Agentd::default();
    config.args = vec!["--config".into(), config_file.to_string_lossy().into_owned()];
    std::fs::write(&config_file, serde_json::to_vec(&serde_json::json!({"shellPath":shell}))?)?;
    assert_eq!(resolve(&config)?, Some(ResolvedShell { path: shell.clone(), source: ShellSource::Explicit }));
    std::fs::remove_file(shell)?;
    assert_eq!(resolve(&config)?, None);
    std::fs::write(&config_file, r#"{"shellPath":42}"#)?;
    assert_eq!(resolve(&config).err().as_deref(), Some("AgentdShellPathInvalid"));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn windows_path_excludes_system_launchers_and_aliases_before_selecting_git() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("polaris-shell-exclusions-{}", std::process::id()));
    let system_root = root.join("Custom Windows");
    let mut excluded = vec![root.join("Users/test/AppData/Local/Microsoft/WindowsApps/bash.exe")];
    for dir in ["System32", "SysWOW64", "Sysnative"] {
        excluded.push(system_root.join(dir).join("bash.exe"));
    }
    let git = root.join("Tools/Git/bin/bash.exe");
    for path in excluded.iter().chain([&git]) {
        std::fs::create_dir_all(path.parent().ok_or("missing parent")?)?;
        std::fs::write(path, "fixture")?;
    }
    for candidate in &excluded {
        assert_eq!(windows_shell(None, [], || vec![candidate.clone()], &system_root), None);
    }
    assert_eq!(windows_shell(None, [], || excluded.clone(), &system_root), None);
    excluded.push(git.clone());
    assert_eq!(windows_shell(None, [], || excluded, &system_root),
        Some(ResolvedShell { path: git, source: ShellSource::Path }));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn windows_exclusions_normalize_case_separators_and_directory_boundaries() {
    let system_root = Path::new(r"D:\Custom Windows\");
    for candidate in [r"d:\CUSTOM WINDOWS\system32\bash.exe", "D:/Custom Windows/SysWOW64/bash.exe",
        r"D:\Custom Windows\Sysnative\bash.exe", r"\\?\D:\Custom Windows\System32\bash.exe",
        "D:/Custom Windows/./System32/../Sysnative/bash.exe",
        r"C:\Users\test\WINDOWSAPPS\bash.exe"] {
        assert!(excluded_windows_shell(Path::new(candidate), system_root), "{candidate}");
    }
    for candidate in ["D:/Custom Windows Git/bin/bash.exe", "C:/Tools/WindowsAppsGit/bash.exe",
        "C:/Tools/Git/bin/bash.exe"] {
        assert!(!excluded_windows_shell(Path::new(candidate), system_root), "{candidate}");
    }
}
