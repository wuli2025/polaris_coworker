use super::*;
/// Pure transformation; the caller holds the cross-process lock and atomically
/// writes config plus credentials together. No intermediate second key file.
pub(super) fn move_credentials(store: &mut Store, credentials: &mut Map<String, Value>) {
    for item in &mut store.items {
        let field = if item.token_field.is_empty() { DEFAULT_TOKEN_FIELD } else { &item.token_field };
        let Some(env) = item.settings_config.get_mut("env").and_then(Value::as_object_mut) else { continue; };
        let key = env.remove(field).and_then(|v| v.as_str().map(str::to_owned));
        let mut secret_env = Map::new();
        for field in SECRET_ENV_KEYS {
            if let Some(value) = env.remove(*field) {
                secret_env.insert((*field).into(), value);
            }
        }
        // Duplicate API-key aliases are representations of the same credential.
        let key = key.or_else(|| secret_env.remove(DEFAULT_TOKEN_FIELD).or_else(|| secret_env.remove(API_KEY_FIELD))
            .and_then(|v| v.as_str().map(str::to_owned)));
        if let Some(key) = key.filter(|k| !k.is_empty()) {
            secret_env.retain(|_, value| value.as_str() != Some(&key));
            let mut credential = json!({"type":"api_key","key":key});
            if !secret_env.is_empty() { credential["env"] = json!(secret_env); }
            credentials.insert(pi_translate::credential_id(&item.id).into(), credential);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_has_one_key_and_preserves_rotated_oauth() {
        let mut store = Store::default();
        store.items.push(StoredProvider { id: "gateway".into(), token_field: DEFAULT_TOKEN_FIELD.into(),
            settings_config: json!({"models":["m"],"env":{
                "ANTHROPIC_AUTH_TOKEN":"key","ANTHROPIC_API_KEY":"key","ANTHROPIC_MODEL":"m"}}),
            ..StoredProvider::default() });
        let oauth = json!({"type":"oauth","access":"new-access","refresh":"rotated","expires":20});
        let mut credentials = Map::new();
        credentials.insert("anthropic".into(), oauth.clone());
        move_credentials(&mut store, &mut credentials);
        assert_eq!(credentials["anthropic"], oauth);
        assert_eq!(credentials["gateway"]["key"], "key");
        assert!(store.items[0].settings_config["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
        assert_eq!(store.items[0].settings_config["env"]["ANTHROPIC_MODEL"], "m");
        assert!(credentials["gateway"].get("env").is_none());
        credentials.insert("gateway".into(), json!({"type":"api_key","key":"updated-from-agentd"}));
        move_credentials(&mut store, &mut credentials);
        assert_eq!(credentials["gateway"]["key"], "updated-from-agentd");
    }
}
