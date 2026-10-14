//! Rebuild preserves the old container by renaming it; never deletes user containers.
use crate::{config::{Config, WorkdirChange}, policy::REBUILD_HINT};
use anyhow::{ensure, Result};
use serde_json::Value;
use std::{path::Path, process::Command, sync::Mutex, time::{SystemTime, UNIX_EPOCH}};
static LIFECYCLE: Mutex<()> = Mutex::new(());

pub fn docker(args: &[String]) -> Result<String> {
    let out = Command::new("docker").args(args).output()?;
    ensure!(out.status.success(), "SANDBOX_DOCKER_FAILED: {}", String::from_utf8_lossy(&out.stderr));
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
pub fn inspect(name: &str) -> Result<Option<Value>> {
    inspect_with(name, &mut docker)
}
fn inspect_with(name: &str, run: &mut impl FnMut(&[String]) -> Result<String>) -> Result<Option<Value>> {
    // A failed inspect is not proof of absence (daemon/permissions may be broken).
    let names = run(&["ps".into(), "-a".into(), "--format".into(), "{{.Names}}".into()])?;
    if !names.lines().any(|n| n == name) { return Ok(None); }
    let values: Vec<Value> = serde_json::from_str(&run(&["inspect".into(), name.into()])?)?;
    Ok(Some(values.into_iter().next().ok_or_else(|| anyhow::anyhow!("SANDBOX_INVALID_CONTAINER"))?))
}
#[derive(Debug, PartialEq)]
pub enum Action { Create, Reuse, PreserveAndRebuild }
pub fn decide(validation: Option<Result<std::path::PathBuf>>, config: &Config, rebuild: bool) -> Result<Action> {
    match validation {
        None => Ok(Action::Create),
        Some(Ok(_)) => Ok(Action::Reuse),
        Some(Err(error)) if rebuild || (config.workdir_change == WorkdirChange::Rebuild
            && error.to_string().starts_with("SANDBOX_PATH_UNMOUNTED:")) => Ok(Action::PreserveAndRebuild),
        Some(Err(error)) => Err(error),
    }
}
pub fn start(config: &Config, name: &str, image: &str, project: &Path, kb: &Path, home: &Path, rebuild: bool) -> Result<String> {
    let _lock = LIFECYCLE.lock().map_err(|_| anyhow::anyhow!("SANDBOX_LOCK_FAILED"))?;
    start_with(config, name, image, project, kb, home, rebuild, &mut docker)
}
fn start_with(config: &Config, name: &str, image: &str, project: &Path, kb: &Path, home: &Path,
    rebuild: bool, run: &mut impl FnMut(&[String]) -> Result<String>) -> Result<String> {
    let args = config.run_args(name, image, project, kb, home)?;
    let existing = inspect_with(name, run)?;
    let action = decide(existing.as_ref().map(|i| config.validate_container(i, project, kb, home)), config, rebuild)?;
    if action == Action::Reuse {
        run(&["start".into(), name.into()])?;
        return Ok(format!("[start ok] 已复用项目容器 {name}"));
    }
    let mut backup = None;
    if action == Action::PreserveAndRebuild {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let saved = format!("{name}-previous-{stamp}");
        run(&["stop".into(), "-t".into(), "5".into(), name.into()])?;
        run(&["rename".into(), name.into(), saved.clone()])?;
        backup = Some(saved);
    }
    match run(&args) {
        Ok(id) => Ok(format!("[run ok] {name} ({id}); 旧容器保留为 {}", backup.as_deref().unwrap_or("无"))),
        Err(error) => Err(anyhow::anyhow!("{error}; 旧容器保留为 {}; {REBUILD_HINT}", backup.as_deref().unwrap_or("无"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_retains_old_container_and_never_deletes_it() -> Result<()> {
        let root = std::env::temp_dir().join(format!("p3b-lifecycle-{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()));
        let home = root.join("home");
        let project = root.join("project");
        let kb = root.join("kb");
        for path in [&home, &project, &kb, &home.join("Polaris")] { std::fs::create_dir_all(path)?; }
        let mut calls = Vec::<Vec<String>>::new();
        let old = serde_json::json!([{"Mounts":[
            {"Type":"bind","Source":home.join("Polaris"),"Destination":"/workspace","RW":true}]}]);
        let mut fake = |args: &[String]| -> Result<String> {
            calls.push(args.to_vec());
            Ok(match args[0].as_str() { "ps" => "managed".into(), "inspect" => old.to_string(), _ => "ok".into() })
        };
        let c = Config::default();
        assert!(start_with(&c, "managed", "image", &project, &kb, &home, false, &mut fake).is_err());
        assert!(calls.iter().all(|a| ["ps", "inspect"].contains(&a[0].as_str())));
        calls.clear();
        let mut fake = |args: &[String]| -> Result<String> {
            calls.push(args.to_vec());
            Ok(match args[0].as_str() { "ps" => "managed".into(), "inspect" => old.to_string(), _ => "ok".into() })
        };
        let message = start_with(&c, "managed", "image", &project, &kb, &home, true, &mut fake)?;
        assert!(message.contains("managed-previous-"));
        assert_eq!(calls.iter().map(|a| a[0].as_str()).collect::<Vec<_>>(), ["ps", "inspect", "stop", "rename", "run"]);
        assert!(!calls.iter().any(|a| a[0] == "rm"));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn ordinary_project_mounts_end_to_end_and_protected_roots_never_reach_docker() -> Result<()> {
        let f = crate::policy_tests::Fixture::new()?;
        for suffix in ["src/data", "sessions", "fixtures"] {
            std::fs::create_dir_all(f.project.join(suffix))?;
        }
        std::fs::write(f.project.join("fixtures/x.key"), "fixture")?;
        let c = Config::default();
        let mut calls = Vec::<Vec<String>>::new();
        start_with(&c, "managed", "image", &f.project, &f.kb, &f.home, false, &mut |args| {
            calls.push(args.to_vec());
            Ok(if args[0] == "ps" { "" } else { "container-id" }.into())
        })?;
        assert_eq!(calls.iter().map(|a| a[0].as_str()).collect::<Vec<_>>(), ["ps", "run"]);
        let mounts: Vec<_> = calls[1].windows(2).filter(|pair| pair[0] == "--mount")
            .map(|pair| pair[1].clone()).collect();
        assert_eq!(mounts, [
            format!("type=bind,src={},dst=/workspace", crate::policy::path_for_docker(&std::fs::canonicalize(&f.project)?)),
            format!("type=bind,src={},dst=/kb,readonly", crate::policy::path_for_docker(&std::fs::canonicalize(&f.kb)?)),
        ]);
        // Route the generated mount sources back through the same inspect policy.
        c.validate_container(&f.inspect(), &f.project, &f.kb, &f.home)?;
        calls.clear();
        let mut protected_config = c.clone();
        let protected = f.project.join("private");
        let child = protected.join("child");
        std::fs::create_dir_all(&child)?;
        protected_config.protected_paths.push(protected.clone());
        let filesystem_root = f.home.ancestors().last().ok_or_else(|| anyhow::anyhow!("missing root"))?;
        for source in [&f.project, &protected, &child, &f.home, filesystem_root] {
            assert!(start_with(&protected_config, "managed", "image", source, &f.kb, &f.home, false, &mut |args| {
                calls.push(args.to_vec());
                Ok("unexpected Docker call".into())
            }).is_err(), "{source:?}");
        }
        assert!(calls.is_empty());
        Ok(())
    }

}
