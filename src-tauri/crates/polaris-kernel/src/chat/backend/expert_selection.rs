//! Request-scoped expert selection. Loading must be read-only; errors fail the turn.
use super::host_tools::ExpertLoader;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ExpertSelection {
    Expert { id: String },
    Team { id: String },
}

pub fn load(selection: &ExpertSelection, loader: Option<&dyn ExpertLoader>) -> anyhow::Result<String> {
    let (kind, id) = match selection {
        ExpertSelection::Expert { id } => ("expert", id),
        ExpertSelection::Team { id } => ("team", id),
    };
    anyhow::ensure!(!id.trim().is_empty(), "EXPERT_INVALID_SELECTION: kind={kind} id={id:?}; id required");
    let loader = loader.ok_or_else(|| anyhow::anyhow!("EXPERT_LOADER_UNAVAILABLE: kind={kind} id={id:?}"))?;
    // Loader diagnostics can contain template text. Expose only selection metadata.
    loader.load(selection).map_err(|_| anyhow::anyhow!("EXPERT_LOAD_FAILED: kind={kind} id={id:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_selection_loads_exact_id_and_rejects_invalid_input() -> anyhow::Result<()> {
        struct ReadOnly;
        impl ExpertLoader for ReadOnly {
            fn load(&self, selected: &ExpertSelection) -> anyhow::Result<String> {
                match selected {
                    ExpertSelection::Expert { id } if id == "qa" => Ok("QA prompt".into()),
                    ExpertSelection::Team { id } if id == "review" => Ok("Review team prompt".into()),
                    _ => anyhow::bail!("unknown expert"),
                }
            }
        }
        for (kind, id, expected) in [("expert", "qa", "QA prompt"), ("team", "review", "Review team prompt")] {
            let selection = serde_json::from_value(serde_json::json!({"kind":kind,"id":id}))?;
            assert_eq!(load(&selection, Some(&ReadOnly))?, expected);
            assert!(load(&selection, None).is_err());
        }
        assert!(load(&ExpertSelection::Expert { id: "unknown".into() }, Some(&ReadOnly)).is_err());
        for value in [serde_json::json!({"kind":"wrong","id":"qa"}), serde_json::json!({"kind":"expert","id":2})] {
            assert!(serde_json::from_value::<ExpertSelection>(value).is_err());
        }
        Ok(())
    }
    #[test]
    fn failures_identify_selection_without_exposing_loader_prompt() -> anyhow::Result<()> {
        struct Failed;
        impl ExpertLoader for Failed {
            fn load(&self, _: &ExpertSelection) -> anyhow::Result<String> {
                anyhow::bail!("private prompt text");
            }
        }
        for kind in ["expert", "team"] {
            let selection = serde_json::from_value(serde_json::json!({"kind":kind,"id":"missing-id"}))?;
            for (loader, code) in [(None, "EXPERT_LOADER_UNAVAILABLE"),
                (Some(&Failed as &dyn ExpertLoader), "EXPERT_LOAD_FAILED")] {
                let error = load(&selection, loader).err().ok_or_else(|| anyhow::anyhow!("expected load failure"))?;
                let diagnostic = format!("{error:#}");
                assert!(diagnostic.contains(code), "{diagnostic}");
                assert!(diagnostic.contains(&format!("kind={kind}")), "{diagnostic}");
                assert!(diagnostic.contains("missing-id"), "{diagnostic}");
                assert!(!diagnostic.contains("private prompt text"));
            }
        }
        Ok(())
    }

}
