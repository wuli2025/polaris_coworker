use serde::Serialize;

#[derive(Serialize)]
pub struct ExpertState { enabled: bool }

#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn expert_enabled_get() -> Result<ExpertState, String> {
    polaris_kernel::chat::backend::selected()
        .and_then(|backend| backend.experts_enabled(None))
        .map(|enabled| ExpertState { enabled }).map_err(|e| e.to_string())
}

#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn expert_enabled_set(enabled: bool) -> Result<ExpertState, String> {
    polaris_kernel::chat::backend::selected()
        .and_then(|backend| backend.experts_enabled(Some(enabled)))
        .map(|enabled| ExpertState { enabled }).map_err(|e| e.to_string())
}
