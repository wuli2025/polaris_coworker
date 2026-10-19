//! Built-in metadata is persisted; the build credential only lives in memory.
use super::*;
pub const ID: &str = "minimax";
pub const BASE_URL: &str = "https://api.minimaxi.com/anthropic";
pub const MODEL: &str = "MiniMax-M3";
pub const CODEX_MODEL: &str = "gpt-6-astra";
pub fn key() -> &'static str {
    option_env!("POLARIS_BUNDLED_MINIMAX_KEY").unwrap_or("").trim()
}
pub fn available() -> bool { !key().is_empty() }
pub(super) fn seed(store: &mut Store, enabled: bool) -> bool {
    if !enabled { return false; }
    if !store.current_id.is_empty() && store.current_id != ID { return false; }
    if !store.items.iter().any(|p| p.id == ID) {
        store.items.push(StoredProvider {
            id: ID.into(), name: "MiniMax".into(), note: "内置".into(),
            token_field: DEFAULT_TOKEN_FIELD.into(), protocol: "anthropic".into(),
            settings_config: config_with_model(default_config(BASE_URL, DEFAULT_TOKEN_FIELD, ""), MODEL),
            ..Default::default()
        });
    }
    store.current_id = ID.into();
    true
}
pub(super) fn mark(view: &mut ProviderView, enabled: bool) {
    if enabled && view.id == ID && view.auth_token.is_empty() {
        view.has_key = true;
        view.note = "内置".into();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_start_seeds_metadata_without_credentials() {
        let mut store = Store::default();
        assert!(seed(&mut store, true));
        assert_eq!(store.current_id, ID);
        assert_eq!(store.items.len(), 1);
        assert!(seed(&mut store, true));
        assert_eq!(store.items.len(), 1);
        let mut view = build_views(&store).into_iter().find(|v| v.id == ID).unwrap();
        mark(&mut view, true);
        mask_view(&mut view);
        assert_eq!(view.note, "内置");
        assert!(view.auth_token.is_empty());
        assert!(view.settings_config["env"].get(DEFAULT_TOKEN_FIELD).is_none());
        let translated = pi_translate::translate(&view, &[], &super::super::super::pi_config::load().unwrap()).unwrap();
        assert_eq!(translated["api"], "anthropic-messages");
        assert_eq!(translated["models"][0]["id"], MODEL);
    }
    #[test]
    fn user_selection_and_settings_are_preserved() {
        for id in ["custom", "claude-official", "codex"] {
            let mut store = Store { current_id: id.into(), ..Default::default() };
            assert!(!seed(&mut store, true));
            assert_eq!(store.current_id, id);
            assert!(store.items.is_empty());
        }
        let mut store = Store::default();
        assert!(!seed(&mut store, false));
        seed(&mut store, true);
        store.items[0].settings_config = json!({"models":["user-model"]});
        seed(&mut store, true);
        assert_eq!(store.items[0].settings_config["models"][0], "user-model");
    }
}
