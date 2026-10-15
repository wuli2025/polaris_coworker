//! Legacy estimates are configurable; absence of a matching rule means unknown.
use super::Usage;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PriceRule {
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    prefixes: Vec<String>,
    /// USD per million: input, output, cache write, cache read.
    rates: [f64; 4],
}

pub(super) fn parse(text: &str) -> Result<Vec<PriceRule>, String> {
    let rules: Vec<PriceRule> = serde_json::from_str(text).map_err(|e| format!("UsagePricesInvalid: {e}"))?;
    for rule in &rules {
        if rule.rates.iter().any(|n| !n.is_finite() || *n < 0.0)
            || rule.contains.iter().chain(&rule.prefixes).any(|s| s.trim().is_empty())
            || (rule.contains.is_empty() && rule.prefixes.is_empty()) {
            return Err("UsagePricesInvalid: nonnegative rates and nonempty matchers required".into());
        }
    }
    Ok(rules)
}

pub(super) fn load() -> Result<Vec<PriceRule>, String> {
    let text = match std::env::var_os("POLARIS_USAGE_PRICES") {
        Some(path) => std::fs::read_to_string(path).map_err(|e| format!("UsagePricesReadFailed: {e}"))?,
        None => include_str!("usage_prices.json").into(),
    };
    parse(&text)
}

pub(super) fn line_cost(u: &Usage, model: &str, rules: &[PriceRule]) -> Option<f64> {
    let model = model.to_ascii_lowercase();
    let rule = rules.iter().find(|rule| {
        rule.contains.iter().any(|s| model.contains(&s.to_ascii_lowercase()))
            || rule.prefixes.iter().any(|s| model.starts_with(&s.to_ascii_lowercase()))
    })?;
    let [input, output, write, read] = rule.rates;
    Some((u.input as f64 * input + u.output as f64 * output
        + u.cache_creation as f64 * write + u.cache_read as f64 * read) / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn price_config_defaults_overrides_and_validation() -> Result<(), String> {
        assert!(!parse(include_str!("usage_prices.json"))?.is_empty());
        let usage = Usage { input: 1_000_000, output: 0, cache_read: 0, cache_creation: 0 };
        let rules = parse(r#"[{"contains":["custom"],"rates":[2,3,0,0]}]"#)?;
        assert_eq!(line_cost(&usage, "CUSTOM-model", &rules), Some(2.0));
        assert_eq!(line_cost(&usage, "unknown", &rules), None);
        for invalid in [r#"[{"contains":[""],"rates":[1,2,3,4]}]"#,
            r#"[{"prefixes":["model"],"rates":[-1,2,3,4]}]"#, r#"{}"#] {
            assert!(parse(invalid).is_err());
        }
        Ok(())
    }
}
