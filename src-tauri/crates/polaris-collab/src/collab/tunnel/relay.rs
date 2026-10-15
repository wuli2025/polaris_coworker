//! Relay policy shared by host, client and gateway endpoints.
use super::super::interconnect::{self, Relay};
use iroh::{Endpoint, RelayMode, RelayUrl};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Weak};

#[derive(Default)]
struct Policy {
    manual: Option<Relay>,
    authority: Option<Relay>,
    endpoints: Vec<Weak<AppliedPolicy>>,
}

static POLICY: Lazy<Mutex<Policy>> = Lazy::new(|| Mutex::new(Policy::default()));

/// Keep this wrapper alive for the endpoint's full lifetime, including watchdog tasks.
pub(crate) struct ConfiguredEndpoint {
    endpoint: Endpoint,
    applied: Arc<AppliedPolicy>,
}

struct AppliedPolicy {
    applied: Relay,
    role: &'static str,
}

impl ConfiguredEndpoint {
    pub(crate) fn uses_policy(&self, policy: &Relay) -> bool {
        &self.applied.applied == policy
    }
}

impl std::ops::Deref for ConfiguredEndpoint {
    type Target = Endpoint;
    fn deref(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl Policy {
    fn desired(&self, defaults: Relay, env_urls: Option<&str>) -> Relay {
        let mut desired = self.manual.as_ref().or(self.authority.as_ref())
            .cloned().map_or(defaults.clone(), |mut policy| {
                // Fallback is local policy; the authority can only advertise URLs.
                policy.fallback = defaults.fallback;
                policy
            });
        if let Some(raw) = env_urls {
            desired.urls = raw.split(',').map(str::trim).filter(|v| !v.is_empty())
                .map(String::from).collect();
        }
        desired
    }

    fn active(&self, desired: &Relay) -> Vec<Value> {
        self.endpoints.iter().filter_map(Weak::upgrade).map(|entry| json!({
            "role": entry.role, "urls": entry.applied.urls,
            "fallback": entry.applied.fallback, "pending": &entry.applied != desired,
        })).collect()
    }
}

pub(crate) fn desired() -> Result<Relay, String> {
    let defaults = interconnect::load()?.relay;
    let env_urls = std::env::var("POLARIS_RELAYS").ok();
    let desired = POLICY.lock().desired(defaults, env_urls.as_deref());
    mode(&desired)?;
    Ok(desired)
}

pub(crate) fn mode(policy: &Relay) -> Result<RelayMode, String> {
    if policy.urls.is_empty() {
        return Ok(if policy.fallback { RelayMode::Default } else { RelayMode::Disabled });
    }
    let urls: Result<Vec<RelayUrl>, String> = policy.urls.iter().map(|url| {
        interconnect::validate_url(url)?;
        url.parse().map_err(|e| format!("relay URL 非法 {url}: {e}"))
    }).collect();
    Ok(RelayMode::custom(urls?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireConfig {
    relays: Vec<WireRelay>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRelay {
    url: String,
}

/// Advanced manual override, retained until process restart. Empty means fallback policy.
pub fn apply_relay_config(raw: &str) -> Result<(), String> {
    let wire: WireConfig = serde_json::from_str(raw)
        .map_err(|e| format!("relay 配置格式错误: {e}"))?;
    let config = Relay {
        urls: wire.relays.into_iter().map(|entry| entry.url).collect(),
        fallback: interconnect::load()?.relay.fallback,
    };
    mode(&config)?;
    POLICY.lock().manual = Some(config);
    Ok(())
}

/// The local fallback preference and manual override always survive remote announcements.
pub fn apply_authority_relays(urls: Vec<String>) -> Result<(), String> {
    let config = Relay { urls, fallback: interconnect::load()?.relay.fallback };
    mode(&config)?;
    POLICY.lock().authority = Some(config);
    Ok(())
}

pub(crate) fn track(
    endpoint: Endpoint,
    applied: Relay,
    role: &'static str,
) -> Arc<ConfiguredEndpoint> {
    let applied = Arc::new(AppliedPolicy { applied, role });
    let mut policy = POLICY.lock();
    policy.endpoints.retain(|entry| entry.strong_count() > 0);
    policy.endpoints.push(Arc::downgrade(&applied));
    Arc::new(ConfiguredEndpoint { endpoint, applied })
}

pub fn relay_status() -> Value {
    match desired() {
        Ok(desired) => {
            let policy = POLICY.lock();
            let active = policy.active(&desired);
            let pending = active.iter().any(|entry| entry["pending"] == true);
            json!({
                "urls": desired.urls, "fallback": desired.fallback,
                "pending": pending, "state": if pending { "pending_reconnect" } else { "applied" },
                "source": if std::env::var_os("POLARIS_RELAYS").is_some() { "environment" }
                    else if policy.manual.is_some() { "manual" }
                    else if policy.authority.is_some() { "authority" } else { "config" },
                "endpoints": active,
            })
        }
        Err(error) => json!({"state": "invalid", "error": error, "pending": false}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list_respects_local_fallback() -> Result<(), String> {
        assert!(matches!(mode(&Relay { urls: vec![], fallback: true })?, RelayMode::Default));
        assert!(matches!(mode(&Relay { urls: vec![], fallback: false })?, RelayMode::Disabled));
        assert!(mode(&Relay { urls: vec!["::bad::".into()], fallback: false }).is_err());
        Ok(())
    }

    #[test]
    fn advanced_override_and_environment_survive_announcement() {
        let defaults = Relay { urls: vec![], fallback: false };
        let authority = Relay { urls: vec!["https://authority.example".into()], fallback: true };
        let manual = Relay { urls: vec!["https://manual.example".into()], fallback: true };
        let policy = Policy { authority: Some(authority), manual: Some(manual), endpoints: vec![] };
        let effective = policy.desired(defaults.clone(), None);
        assert_eq!(effective.urls, ["https://manual.example"]);
        assert!(!effective.fallback);
        assert!(policy.desired(defaults, Some("")).urls.is_empty());
    }

    #[test]
    fn missing_or_malformed_list_is_not_silently_cleared() {
        assert!(serde_json::from_str::<WireConfig>(r#"{}"#).is_err());
        assert!(serde_json::from_str::<WireConfig>(r#"{"relays":false}"#).is_err());
        assert!(serde_json::from_str::<WireConfig>(r#"{"relays":[{}]}"#).is_err());
    }

    #[test]
    fn pending_tracks_old_endpoint_lifetime_until_reconnect() {
        let old = Relay { urls: vec![], fallback: true };
        let next = Relay { urls: vec!["https://relay.example.com".into()], fallback: false };
        let lease = Arc::new(AppliedPolicy { applied: old, role: "client" });
        let policy = Policy { endpoints: vec![Arc::downgrade(&lease)], ..Policy::default() };
        assert_eq!(policy.active(&next)[0]["pending"], true);
        let second_tunnel = lease.clone();
        drop(lease);
        assert_eq!(policy.active(&next)[0]["pending"], true);
        drop(second_tunnel);
        assert!(policy.active(&next).is_empty(), "last disconnect releases old policy");
        let reconnected = Arc::new(AppliedPolicy { applied: next.clone(), role: "client" });
        let policy = Policy { endpoints: vec![Arc::downgrade(&reconnected)], ..Policy::default() };
        assert_eq!(policy.active(&next)[0]["pending"], false);
    }
}
