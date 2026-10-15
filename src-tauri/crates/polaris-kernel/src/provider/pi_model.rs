//! Shared translated-model constraints. Paths refer to the provider editor input.
use serde_json::Value;

pub fn validate(model: &Value, index: usize) -> Result<(), String> {
    let invalid = |field: &str| format!("ProviderModelInvalid [paths: /settingsConfig/models/{index}/{field}]");
    for field in ["id", "name"] {
        if model[field].as_str().is_none_or(|s| s.trim().is_empty()) { return Err(invalid(field)); }
    }
    if !model["reasoning"].is_boolean() { return Err(invalid("reasoning")); }
    for field in ["contextWindow", "maxTokens"] {
        if model[field].as_u64().is_none_or(|n| n == 0 || n > 2147483647) { return Err(invalid(field)); }
    }
    if model["maxTokens"].as_u64() > model["contextWindow"].as_u64() { return Err(invalid("maxTokens")); }
    if model["input"].as_array().is_none_or(|a| a.is_empty() || a.iter().any(|s| s != "text" && s != "image")) {
        return Err(invalid("input"));
    }
    validate_cost(&model["cost"], false).map_err(|field| invalid(&format!("cost/{field}")))?;
    if let Some(tiers) = model["cost"].get("tiers") {
        let tiers = tiers.as_array().ok_or_else(|| invalid("cost/tiers"))?;
        for (tier, value) in tiers.iter().enumerate() {
            validate_cost(value, true).map_err(|field| invalid(&format!("cost/tiers/{tier}/{field}")))?;
        }
    }
    Ok(())
}

fn validate_cost(value: &Value, tier: bool) -> Result<(), String> {
    let object = value.as_object().ok_or_else(|| "".to_string())?;
    let rates = ["input", "output", "cacheRead", "cacheWrite"];
    for field in rates {
        // SDK negative rates explicitly mean unknown/dynamic pricing (P5d).
        if object.get(field).and_then(Value::as_f64).is_none_or(|n| !n.is_finite()) {
            return Err(field.into());
        }
    }
    if tier && value["inputTokensAbove"].as_f64().is_none_or(|n| !n.is_finite() || n < 0.0) {
        return Err("inputTokensAbove".into());
    }
    for field in object.keys() {
        if !rates.contains(&field.as_str()) && field != if tier { "inputTokensAbove" } else { "tiers" } {
            return Err(field.replace('~', "~0").replace('/', "~1"));
        }
    }
    Ok(())
}
