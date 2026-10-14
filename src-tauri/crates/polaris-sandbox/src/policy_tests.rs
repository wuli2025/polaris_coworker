use crate::{config::{Config, WorkdirChange}, lifecycle::{decide, Action}, policy::path_for_docker};
use anyhow::Result;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
pub(crate) struct Fixture {
    pub(crate) root: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) project: PathBuf,
    pub(crate) kb: PathBuf,
}
impl Fixture {
    pub(crate) fn new() -> Result<Self> {
        let root = std::env::temp_dir().join(format!("p3b-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
        let result = Self { home: root.join("home"), project: root.join("project"), kb: root.join("kb"), root };
        for path in [&result.home, &result.project, &result.kb] { std::fs::create_dir_all(path)?; }
        Ok(result)
    }
    pub(crate) fn inspect(&self) -> Value {
        json!({"Mounts":[
            {"Type":"bind","Source":self.project,"Destination":"/workspace","RW":true},
            {"Type":"bind","Source":self.kb,"Destination":"/kb","RW":false}],
            "HostConfig":{"Memory":4294967296u64,"NanoCpus":2000000000u64,"CapDrop":["ALL"],
                "CapAdd":null,"SecurityOpt":["no-new-privileges"],"Privileged":false,
                "PidMode":"","IpcMode":"private","NetworkMode":"bridge","Devices":[]}})
    }
    fn validate(&self, config: &Config, info: &Value) -> Result<PathBuf> {
        config.validate_container(info, &self.project, &self.kb, &self.home)
    }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); } }
#[test]
fn run_mounts_only_project_and_readonly_kb() -> Result<()> {
    let f = Fixture::new()?;
    let c = Config::default();
    let args = c.run_args("test", "image", &f.project, &f.kb, &f.home)?;
    let text = args.join(" ");
    for forbidden in ["data/", ".key", "providers.json", "collab.db", "claude-home", "sessions"] {
        assert!(!text.contains(forbidden), "{text}");
    }
    assert_eq!(args.iter().filter(|a| *a == "--mount").count(), 2);
    assert!(text.contains("dst=/kb,readonly"));
    assert!(text.contains("--cap-drop=ALL --security-opt=no-new-privileges"));
    f.validate(&c, &f.inspect())?;
    Ok(())
}
#[test]
fn forbidden_config_and_parent_mounts_are_rejected() -> Result<()> {
    let f = Fixture::new()?;
    for suffix in ["Polaris/data", ".polaris", ".pi", ".claude", ".codex", ".ssh", ".aws", ".gnupg", ".kube",
        "Polaris/claude-home", "Polaris/sessions"] {
        let secret = f.home.join(suffix);
        std::fs::create_dir_all(&secret)?;
        let mut c = Config::default();
        c.mounts[0].source = secret.to_string_lossy().into_owned();
        assert!(c.run_args("test", "image", &f.project, &f.kb, &f.home).is_err());
        assert!(c.run_args("test", "image", &secret, &f.kb, &f.home).is_err());
    }
    let mut old = f.inspect();
    old["Mounts"][0]["Source"] = json!(f.home.join("Polaris"));
    assert!(f.validate(&Config::default(), &old).err().is_some_and(|e|
        e.to_string().contains("SANDBOX_UNSAFE_MOUNT") && e.to_string().contains("rebuild:true")));
    Ok(())
}
#[test]
fn rejects_wrong_destinations_modes_sources_and_extra_mounts() -> Result<()> {
    let f = Fixture::new()?;
    let c = Config::default();
    for (index, field, value) in [(0, "Destination", json!("/other")), (0, "RW", json!(false)),
        (1, "RW", json!(true)), (1, "Destination", json!("/workspace")), (1, "Type", json!("volume")),
        (1, "Source", json!(f.project))] {
        let mut info = f.inspect();
        info["Mounts"][index][field] = value;
        assert!(f.validate(&c, &info).is_err(), "{info}");
    }
    let mut info = f.inspect();
    if let Some(mounts) = info["Mounts"].as_array_mut() { mounts.push(json!({"Type":"bind","Source":f.home,"Destination":"/extra","RW":false})); }
    assert!(f.validate(&c, &info).is_err());
    let child = f.project.join("child");
    std::fs::create_dir(&child)?;
    assert!(c.validate_container(&f.inspect(), &child, &f.kb, &f.home).err()
        .is_some_and(|e| e.to_string().starts_with("SANDBOX_PATH_UNMOUNTED")));
    Ok(())
}
#[test]
fn rejects_missing_and_weakened_security_limits() -> Result<()> {
    let f = Fixture::new()?;
    let c = Config::default();
    for (key, value) in [("Memory", json!(0)), ("Memory", json!(8589934592u64)), ("NanoCpus", json!(0)),
        ("NanoCpus", json!(3000000000u64)), ("CapDrop", json!([])), ("CapAdd", json!(["SYS_ADMIN"])),
        ("SecurityOpt", json!([])), ("SecurityOpt", json!(["no-new-privileges:false"])),
        ("Privileged", json!(true)), ("PidMode", json!("host")), ("NetworkMode", json!("host")),
        ("Devices", json!([{"PathOnHost":"/dev/sda"}]))] {
        let mut info = f.inspect();
        info["HostConfig"][key] = value;
        assert!(f.validate(&c, &info).err().is_some_and(|e| e.to_string().contains("SANDBOX_UNSAFE_CONTAINER")));
    }
    let mut missing = f.inspect();
    missing["HostConfig"] = json!({});
    assert!(f.validate(&c, &missing).is_err());
    Ok(())
}
#[test]
fn config_types_defaults_and_append_only_protection() -> Result<()> {
    for input in [json!({"cpus":0}), json!({"memoryBytes":"4g"}), json!({"workdirChange":"delete"}),
        json!({"capAdd":["ALL"]}), json!({"mounts":[]}), json!({"unknown":true})] {
        assert!(serde_json::from_value::<Config>(input).and_then(|c|
            c.validate().map(|_| c).map_err(serde::de::Error::custom)).is_err());
    }
    let f = Fixture::new()?;
    let mut c: Config = serde_json::from_value(json!({"protectedPaths":[f.project]}))?;
    assert!(f.validate(&c, &f.inspect()).is_err());
    c.protected_paths.clear();
    assert!(c.protected(&f.home).contains(&f.home.join(".gnupg")));
    assert_eq!(c.memory_bytes, 4294967296);
    Ok(())
}
#[test]
fn lifecycle_never_automatically_migrates_unsafe_containers() -> Result<()> {
    let mut c = Config::default();
    assert_eq!(decide(None, &c, false)?, Action::Create);
    assert_eq!(decide(Some(Ok(PathBuf::from("project"))), &c, false)?, Action::Reuse);
    let changed = || Some(Err(anyhow::anyhow!("SANDBOX_PATH_UNMOUNTED: changed")));
    assert!(decide(changed(), &c, false).is_err());
    c.workdir_change = WorkdirChange::Rebuild;
    assert_eq!(decide(changed(), &c, false)?, Action::PreserveAndRebuild);
    for code in ["SANDBOX_UNSAFE_MOUNT", "SANDBOX_UNSAFE_CONTAINER"] {
        assert!(decide(Some(Err(anyhow::anyhow!(code))), &c, false).is_err());
        assert_eq!(decide(Some(Err(anyhow::anyhow!(code))), &c, true)?, Action::PreserveAndRebuild);
    }
    Ok(())
}
#[test]
fn windows_drive_paths_preserve_drive_and_spaces() {
    assert_eq!(path_for_docker(Path::new(r"D:\My Project\app")), "D:/My Project/app");
    assert_eq!(path_for_docker(Path::new(r"C:\项目")), "C:/项目");
    assert_eq!(path_for_docker(Path::new("E:/project")), "E:/project");
    assert_eq!(path_for_docker(Path::new(r"\\?\D:\project")), "D:/project");
}
#[cfg(unix)]
#[test]
fn symlink_alias_of_protected_source_is_rejected() -> Result<()> {
    let f = Fixture::new()?;
    let secret = f.home.join(".ssh");
    std::fs::create_dir(&secret)?;
    let alias = f.root.join("alias");
    std::os::unix::fs::symlink(secret, &alias)?;
    assert!(Config::default().run_args("test", "image", &alias, &f.kb, &f.home).is_err());
    Ok(())
}

#[test]
fn published_schema_defaults_match_runtime() -> Result<()> {
    let schema: Value = serde_json::from_str(include_str!("../sandbox.schema.json"))?;
    let defaults = serde_json::to_value(Config::default())?;
    if let Some(fields) = defaults.as_object() {
        for (key, value) in fields { assert_eq!(&schema["properties"][key]["default"], value, "{key}"); }
    } else { anyhow::bail!("config must be an object"); }
    Ok(())
}

#[test]
fn ordinary_names_are_allowed_in_sources_and_contents() -> Result<()> {
    let mut f = Fixture::new()?;
    f.project = f.project.join("data/sessions");
    for suffix in ["src/data", "sessions", "fixtures"] {
        std::fs::create_dir_all(f.project.join(suffix))?;
    }
    std::fs::write(f.project.join("fixtures/x.key"), "test fixture")?;
    let c = Config::default();
    let args = c.run_args("test", "image", &f.project, &f.kb, &f.home)?;
    assert!(args.contains(&format!("type=bind,src={},dst=/workspace",
        path_for_docker(&std::fs::canonicalize(&f.project)?))));
    assert_eq!(f.validate(&c, &f.inspect())?, std::fs::canonicalize(&f.project)?);
    Ok(())
}

#[test]
fn protected_ancestors_descendants_home_and_root_fail_creation_and_routing() -> Result<()> {
    let f = Fixture::new()?;
    let protected = f.project.join("private");
    let child = protected.join("child");
    std::fs::create_dir_all(&child)?;
    let mut c = Config::default();
    c.protected_paths.push(protected.clone());
    let safe_kb = f.root.join("safe-kb");
    std::fs::create_dir(&safe_kb)?;
    let filesystem_root = f.home.ancestors().last().ok_or_else(|| anyhow::anyhow!("missing root"))?;
    for source in [&f.project, &protected, &child, &f.home, &f.root, filesystem_root] {
        assert!(c.run_args("test", "image", source, &f.kb, &f.home).is_err(), "{source:?}");
        let mut info = f.inspect();
        info["Mounts"][0]["Source"] = json!(source);
        assert!(c.validate_container(&info, source, &f.kb, &f.home).is_err(), "{source:?}");
        // Also reject an unsafe actual mount when the requested project is safe.
        info["Mounts"][1]["Source"] = json!(safe_kb);
        let error = c.validate_container(&info, &f.kb, &safe_kb, &f.home)
            .err().ok_or_else(|| anyhow::anyhow!("unsafe actual mount accepted"))?;
        assert!(error.to_string().starts_with("SANDBOX_UNSAFE_MOUNT:"), "{error}");
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn route_validation_never_requires_directory_enumeration() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    struct Restore(PathBuf, std::fs::Permissions);
    impl Drop for Restore {
        fn drop(&mut self) { let _ = std::fs::set_permissions(&self.0, self.1.clone()); }
    }
    let f = Fixture::new()?;
    let c = Config::default();
    // Search/metadata access remains available; directory enumeration is denied.
    let _restore = Restore(f.project.clone(), std::fs::metadata(&f.project)?.permissions());
    for count in [0, 2048] {
        std::fs::set_permissions(&f.project, std::fs::Permissions::from_mode(0o700))?;
        for index in 0..count { std::fs::write(f.project.join(format!("fixture-{index}.key")), "fixture")?; }
        std::fs::set_permissions(&f.project, std::fs::Permissions::from_mode(0o111))?;
        // Root bypasses DAC, so this test needs an unprivileged test runner.
        anyhow::ensure!(std::fs::read_dir(&f.project).is_err(), "run this enumeration regression as an unprivileged user");
        c.run_args("test", "image", &f.project, &f.kb, &f.home)?;
        f.validate(&c, &f.inspect())?;
    }
    Ok(())
}
