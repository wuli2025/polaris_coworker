use super::Usage;
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    pub source: String,
    pub model: String,
    pub has_price: bool,
    pub priced_requests: u64,
    pub unpriced_requests: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBucket {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub total: u64,
    pub requests: u64,
    /// Recorded/estimated priced portion, retained even when the total is incomplete.
    pub cost: f64,
    pub unpriced_requests: u64,
    pub has_unknown_costs: bool,
    pub models: Vec<ModelCost>,
}

impl TokenBucket {
    pub(super) fn add(&mut self, u: &Usage, cost: f64, has_price: bool, source: &str, model: &str) {
        self.input += u.input;
        self.output += u.output;
        self.cache_read += u.cache_read;
        self.cache_creation += u.cache_creation;
        self.total += u.input + u.output + u.cache_read + u.cache_creation;
        self.requests += 1;
        self.cost += cost;
        if !has_price { self.unpriced_requests += 1; self.has_unknown_costs = true; }
        let index = match self.models.iter().position(|m| m.source == source && m.model == model) {
            Some(index) => index,
            None => {
                self.models.push(ModelCost { source: source.into(), model: model.into(), ..Default::default() });
                self.models.len() - 1
            }
        };
        let row = &mut self.models[index];
        row.cost += cost;
        if has_price { row.priced_requests += 1; } else { row.unpriced_requests += 1; }
        row.has_price = row.unpriced_requests == 0;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsage {
    pub date: String,
    pub label: String,
    pub total: u64,
    pub cost: f64,
    pub unpriced_requests: u64,
    pub has_unknown_costs: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_mixed_and_unknown_costs_preserve_priced_subtotal() {
        let usage = Usage { input: 1_000_000, output: 0, cache_read: 0, cache_creation: 0 };
        for flags in [[true, true], [true, false], [false, false]] {
            let mut bucket = TokenBucket::default();
            for priced in flags {
                bucket.add(&usage, if priced { 3.0 } else { 0.0 }, priced, "test", "model");
            }
            let unknown = flags.iter().filter(|p| !**p).count() as u64;
            assert_eq!(bucket.unpriced_requests, unknown);
            assert_eq!(bucket.has_unknown_costs, unknown != 0);
            assert_eq!(bucket.models[0].has_price, unknown == 0);
            assert_eq!(bucket.models[0].unpriced_requests, unknown);
            assert_eq!(bucket.cost, (2 - unknown) as f64 * 3.0);
            assert_eq!(bucket.requests, 2);
        }
    }
    #[test]
    fn explicit_free_prices_are_known_and_unknown_models_are_not_sonnet() -> Result<(), String> {
        let prices = super::super::prices::parse(include_str!("usage_prices.json"))?;
        let usage = Usage { input: 100, output: 20, cache_read: 0, cache_creation: 0 };
        assert!(super::super::prices::line_cost(&usage, "unlisted-model", &prices).is_none());
        assert!(super::super::prices::line_cost(&usage, "claude-sonnet", &prices).is_some());
        let mut bucket = TokenBucket::default();
        bucket.add(&usage, 0.0, true, "pi", "free");
        bucket.add(&usage, 1.25, false, "pi", "historical-partial");
        assert!(bucket.models[0].has_price);
        assert!(!bucket.models[1].has_price);
        assert_eq!(bucket.unpriced_requests, 1);
        assert_eq!(bucket.cost, 1.25);
        Ok(())
    }
}
