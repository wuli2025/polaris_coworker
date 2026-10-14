//! Host configuration: POLARIS_EXEC_CONFIG points to a JSON file.
//! Defaults preserve existing users. Pi itself does not need `claude`;
//! Pi-only hosts can omit it from allowedPrograms (see the migration guide).
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Config {
    pub allowed_programs: Vec<String>,
}
impl Config {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, String> {
        let config: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        for name in &config.allowed_programs {
            if name.is_empty() || name.contains("..") || name.starts_with('-')
                || !name.chars().all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
            {
                return Err(format!("InvalidExecConfig: invalid program name {name:?}"));
            }
        }
        Ok(config)
    }
}
pub(super) fn load() -> Result<Config, String> {
    match std::env::var_os("POLARIS_EXEC_CONFIG") {
        Some(path) => Config::parse(&std::fs::read(path).map_err(|e| format!("InvalidExecConfig: {e}"))?),
        None => Config::parse(include_bytes!("defaults.json")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_defaults_and_pi_only_configuration() -> Result<(), String> {
        let legacy = Config::parse(include_bytes!("defaults.json"))?;
        let pi = Config::parse(br#"{"allowedPrograms":["node","git","cargo"]}"#)?;
        for program in ["claude", "codex", "git"] {
            super::super::check_configured_whitelist(program, &[], &legacy.allowed_programs)?;
        }
        assert!(super::super::check_configured_whitelist("claude", &[], &pi.allowed_programs).is_err());
        super::super::check_configured_whitelist("node", &[], &pi.allowed_programs)?;
        assert!(super::super::check_configured_whitelist("/tmp/node", &[], &pi.allowed_programs).is_err());
        assert!(super::super::check_configured_whitelist("node", &["x;y".into()], &pi.allowed_programs).is_err());
        for invalid in [r#"{"allowedPrograms":"node"}"#, r#"{"allowedPrograms":["../node"]}"#,
            r#"{"allowedPrograms":["node.exe"]}"#, r#"{"allowedPrograms":[1]}"#, r#"{}"#] {
            assert!(Config::parse(invalid.as_bytes()).is_err());
        }
        Ok(())
    }
}
