use polaris_kernel::chat::backend::pi::protocol::{Catalog, check_ping};
use serde_json::{json, Value};
#[test]
fn v6_samples_and_invalid_catalog() -> anyhow::Result<()> {
    for line in include_str!("fixtures/protocol-samples.jsonl").lines() {
        let v: Value = serde_json::from_str(line)?;
        if v["id"] == "catalog-v6" {
            let mut sample = v["result"].clone();
            let catalog = Catalog::parse(sample.clone())?;
            assert_eq!(catalog.models.len(), 1);
            assert_eq!(catalog.models[0].cost.input, -1.0);
            assert_eq!(catalog.models[0].cost.tiers.as_ref().ok_or_else(|| anyhow::anyhow!("MissingTiers"))?[0].input_tokens_above, 1000.0);
            sample["models"][0]["cost"]["input"] = json!("invalid");
            assert!(Catalog::parse(sample).is_err());
        }
        if v["result"]["status"] == "ok" { assert_eq!(check_ping(v["result"].clone())?.usage_persist_failures, 0); }
    }
    assert!(Catalog::parse(json!({"models":[{"provider":"p","id":"m"}]})).is_err());
    assert!(check_ping(json!({"status":"ok","sessions":0,"version":"0.1.0","piVersion":"1.0.4","protocolVersion":5})).is_err());
    Ok(())
}
