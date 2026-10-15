use super::*;

#[test]
fn pi_aggregation_complete_partial_unknown_and_legacy_records() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("usage-contract-{}", std::process::id()));
    let date = ymd_string(today_utc_days());
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let run = (|| {
        for flags in [[true, true], [true, false], [false, false]] {
            for (i, priced) in flags.iter().enumerate() {
                let row = json!({ "date": date, "provider": "custom", "model": format!("model-{i}"),
                    "requestId": format!("request-{i}"), "priceAvailable": priced,
                    "usage": { "input": 100, "output": 20, "cacheRead": 0, "cacheWrite": 0,
                        "totalTokens": 120, "cost": { "input": 0, "output": 0, "cacheRead": 0,
                            "cacheWrite": 0, "total": if *priced { 2.5 } else { 0.0 } } } });
                fs::write(dir.join(format!("{i}.json")), row.to_string()).map_err(|e| e.to_string())?;
            }
            let summary = summarize(vec![], Some(dir.clone()))?;
            let unknown = flags.iter().filter(|v| !**v).count() as u64;
            for bucket in [&summary.today, &summary.week, &summary.month, &summary.year] {
                assert_eq!(bucket.requests, 2);
                assert_eq!(bucket.unpriced_requests, unknown);
                assert_eq!(bucket.has_unknown_costs, unknown > 0);
                assert_eq!(bucket.cost, (2 - unknown) as f64 * 2.5);
                assert_eq!(bucket.models.len(), 2);
            }
            let daily = summary.daily.iter().find(|d| d.date == date).ok_or("missing today")?;
            assert_eq!(daily.unpriced_requests, unknown);
            assert_eq!(daily.cost, summary.today.cost);
        }
        // Pre-contract Pi rows have no evidence of completeness. Preserve their amount.
        let path = dir.join("0.json");
        let mut historical: Value = serde_json::from_str(&fs::read_to_string(&path)
            .map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        historical.as_object_mut().ok_or("invalid fixture")?.remove("priceAvailable");
        historical["usage"]["cost"]["total"] = json!(1.25);
        fs::write(path, historical.to_string()).map_err(|e| e.to_string())?;
        let summary = summarize(vec![], Some(dir.clone()))?;
        assert_eq!(summary.today.cost, 1.25);
        assert_eq!(summary.today.unpriced_requests, 2);
        Ok(())
    })();
    let _ = fs::remove_dir_all(dir);
    run
}

#[test]
fn pi_price_flag_is_typed_optional_and_independent_of_amount() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("usage-price-flag-{}", std::process::id()));
    let date = ymd_string(today_utc_days());
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let run = (|| {
        let path = dir.join("request.json");
        for flag in [Some(true), Some(false), None] {
            for cost in [0.0, 1.25] {
                let mut row = json!({ "version": 1, "date": date, "provider": "custom", "model": "model",
                    "requestId": "request", "usage": { "input": 100, "output": 20,
                        "cacheRead": 0, "cacheWrite": 0, "totalTokens": 120,
                        "cost": { "input": cost, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": cost } } });
                if let Some(priced) = flag { row["priceAvailable"] = json!(priced); }
                fs::write(&path, row.to_string()).map_err(|e| e.to_string())?;
                let rows = super::super::pi_usage::read(&dir)?;
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].price_available, flag);
                let summary = summarize(vec![], Some(dir.clone()))?;
                let unknown = u64::from(flag != Some(true));
                for bucket in [&summary.today, &summary.week, &summary.month, &summary.year] {
                    assert_eq!(bucket.requests, 1);
                    assert_eq!(bucket.total, 120);
                    assert_eq!(bucket.cost, cost);
                    assert_eq!(bucket.unpriced_requests, unknown);
                    assert_eq!(bucket.has_unknown_costs, unknown != 0);
                    let model = bucket.models.first().ok_or("missing model")?;
                    assert_eq!(model.has_price, flag == Some(true));
                    assert_eq!(model.priced_requests, 1 - unknown);
                    assert_eq!(model.unpriced_requests, unknown);
                }
                let daily = summary.daily.iter().find(|d| d.date == date).ok_or("missing today")?;
                assert_eq!(daily.cost, cost);
                assert_eq!(daily.unpriced_requests, unknown);
                assert_eq!(daily.has_unknown_costs, unknown != 0);
                row["priceAvailable"] = json!("true");
                fs::write(&path, row.to_string()).map_err(|e| e.to_string())?;
                assert!(matches!(summarize(vec![], Some(dir.clone())), Err(error) if error == "PiUsageInvalid"));
            }
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(dir);
    run
}
