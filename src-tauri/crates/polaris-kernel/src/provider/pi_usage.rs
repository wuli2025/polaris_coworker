use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub date: String,
    pub provider: String,
    pub request_id: String,
    pub legacy_message_id: Option<String>,
    pub usage: crate::chat::backend::pi::protocol::Usage,
}
impl Row {
    pub fn dedup_key(&self) -> String {
        match &self.legacy_message_id {
            Some(id) => format!("request:{}:{id}", self.date),
            None => format!("pi:{}:{}:{}", self.date, self.provider, self.request_id),
        }
    }
}
pub fn read(dir: &Path) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for entry in WalkDir::new(dir).into_iter().flatten().filter(|e| e.file_type().is_file()) {
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") { continue; }
        let bytes = fs::read(entry.path()).map_err(|_| "PiUsageReadFailed")?;
        let row: Row = serde_json::from_slice(&bytes).map_err(|_| "PiUsageInvalid")?;
        let u = &row.usage;
        if row.date.len() != 10 || [u.input, u.output, u.cache_read, u.cache_write, u.total_tokens, u.cost.total]
            .iter().any(|n| !n.is_finite() || *n < 0.0) { return Err("PiUsageInvalid".into()); }
        if seen.insert(row.dedup_key()) { rows.push(row); }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copied_pi_rows_share_legacy_identity_but_other_requests_do_not() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("p4-usage-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut row = json!({"date":"2026-10-14","provider":"anthropic","requestId":"msg-1",
            "legacyMessageId":"msg-1","usage":{"input":10,"output":3,"cacheRead":2,"cacheWrite":1,"totalTokens":16,
                "cost":{"input":0.1,"output":0.2,"cacheRead":0.01,"cacheWrite":0.02,"total":0.33}}});
        let result = (|| {
            fs::write(dir.join("one.json"), row.to_string()).map_err(|e| e.to_string())?;
            fs::write(dir.join("copy.json"), row.to_string()).map_err(|e| e.to_string())?;
            let rows = read(&dir)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].dedup_key(), "request:2026-10-14:msg-1");
            assert_eq!(rows[0].usage.cost.total, 0.33);
            row["requestId"] = json!("msg-2");
            row["legacyMessageId"] = json!("msg-2");
            fs::write(dir.join("two.json"), row.to_string()).map_err(|e| e.to_string())?;
            assert_eq!(read(&dir)?.len(), 2);
            row["date"] = json!("2026-10-15");
            fs::write(dir.join("next-day.json"), row.to_string()).map_err(|e| e.to_string())?;
            assert_eq!(read(&dir)?.len(), 3);
            Ok(())
        })();
        let _ = fs::remove_dir_all(&dir);
        result
    }
}
