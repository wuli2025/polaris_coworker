use super::*;

// ───────────────────────── Commands: 用量看板 ─────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBucket {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub total: u64,
    pub requests: u64,
    pub cost: f64,
}

impl TokenBucket {
    fn add(&mut self, u: &Usage, cost: f64) {
        self.input += u.input;
        self.output += u.output;
        self.cache_read += u.cache_read;
        self.cache_creation += u.cache_creation;
        self.total += u.input + u.output + u.cache_read + u.cache_creation;
        self.requests += 1;
        self.cost += cost;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsage {
    pub date: String,
    pub label: String,
    pub total: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub available: bool,
    pub today: TokenBucket,
    pub week: TokenBucket,
    pub month: TokenBucket,
    pub year: TokenBucket,
    pub daily: Vec<DailyUsage>,
}

struct Usage {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_creation: u64,
}

/// 模型 → (input, output, cache_write, cache_read) USD / 1M tokens。估算用。
fn model_price(model: &str) -> (f64, f64, f64, f64) {
    let m = model.to_ascii_lowercase();
    if m.contains("opus") {
        (15.0, 75.0, 18.75, 1.5)
    } else if m.contains("haiku") {
        (0.8, 4.0, 1.0, 0.08)
    } else if m.contains("sonnet") {
        (3.0, 15.0, 3.75, 0.3)
    } else if m.contains("gpt") || m.contains("codex") || m.starts_with("o1") || m.starts_with("o3")
    {
        (1.25, 10.0, 1.5625, 0.125)
    } else if m.contains("gemini") {
        (1.25, 10.0, 1.625, 0.31)
    } else if m.contains("deepseek") {
        (0.27, 1.1, 0.027, 0.027)
    } else if m.contains("glm") {
        (0.6, 2.2, 0.11, 0.11)
    } else if m.contains("kimi") || m.contains("moonshot") {
        (0.6, 2.5, 0.15, 0.15)
    } else if m.contains("qwen") || m.contains("minimax") {
        (0.4, 1.2, 0.08, 0.08)
    } else {
        (3.0, 15.0, 3.75, 0.3) // 未知 → Sonnet 档
    }
}

fn line_cost(u: &Usage, model: &str) -> f64 {
    let (pin, pout, pcw, pcr) = model_price(model);
    (u.input as f64 * pin
        + u.output as f64 * pout
        + u.cache_creation as f64 * pcw
        + u.cache_read as f64 * pcr)
        / 1_000_000.0
}

// command(async): WalkDir 全量遍历 ~/.claude/projects 并逐行解析 jsonl(一年后可达
// 数百 MB), 同步命令会钉住 UI 主线程(v1.5.2 同族问题)。属性形式挪到工作线程,
// fn 保持同步签名(server dispatch 直调不受影响)。
#[cfg_attr(feature = "desktop", tauri::command(async))]
pub fn usage_summary() -> Result<UsageSummary, String> {
    // 共享 ~/.claude/projects + 隔离模式的私有账本, 两处都算 —— 深隔离只是把
    // 第三方会话从外部监控的视野里挪走, Polaris 自己的看板仍要看全。
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = claude_dir().map(|d| d.join("projects")) {
        dirs.push(d);
    }
    if let Some(d) = private_claude_home().map(|d| d.join("projects")) {
        dirs.push(d);
    }
    dirs.retain(|d| d.exists());
    let pi_dir = credential_file::provider_store_path().and_then(|p| p.parent().map(|p| p.join("pi-usage")));
    if dirs.is_empty() && !pi_dir.as_ref().is_some_and(|p| p.exists()) {
        return Ok(empty_summary());
    }

    let today_days = today_utc_days();
    let today_str = ymd_string(today_days);
    let week_cut = ymd_string(today_days - 6);
    let month_cut = ymd_string(today_days - 29);
    let year_cut = ymd_string(today_days - 364);

    // 14 天趋势窗
    let mut trend_window: Vec<(String, String)> = Vec::with_capacity(14);
    for off in (0..14).rev() {
        let d = today_days - off;
        let s = ymd_string(d);
        let label = s.get(5..).unwrap_or(&s).to_string();
        trend_window.push((s, label));
    }
    let trend_set: HashSet<String> = trend_window.iter().map(|(s, _)| s.clone()).collect();
    let mut by_day: HashMap<String, (u64, f64)> = HashMap::new();

    let mut today = TokenBucket::default();
    let mut week = TokenBucket::default();
    let mut month = TokenBucket::default();
    let mut year = TokenBucket::default();
    let mut seen: HashSet<String> = HashSet::new();

    // ── 先列文件,再多线程分头解析 ──────────────────────────────────────────
    // 这份账本会越滚越大:本机 2026-07-29 已是 **3,658 个 jsonl / 2.57GB**,
    // 原来的「单线程 WalkDir + 逐行 serde_json」实测每次要 5.4~7.4s(压测四次复测都 >5s),
    // 而看板一进页面就调、切回来还调 —— 用户感知就是「用量页每次都卡好几秒」。
    // 解析本身是纯 CPU + 顺序读,天然可并行:各线程只吐自己那份「候选记录」,
    // 汇总(含跨文件的 msg_id 去重)仍在主线程串行做 —— 结果与单线程逐字节一致。
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in dirs
        .iter()
        .flat_map(|d| WalkDir::new(d).into_iter().flatten())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        files.push(entry.path().to_path_buf());
    }

    /// 一行有效用量记录(解析阶段的中间产物;去重与分桶留给主线程)。
    struct Rec {
        msg_id: Option<String>,
        date: String,
        usage: Usage,
        cost: f64,
        line_tokens: u64,
    }

    // 并发宽度:默认按核数取(2~8)。`POLARIS_USAGE_SCAN_PAR=1` 可退回单线程 ——
    // 排查「看板数字对不对」时用它做 A/B,两档结果必须完全一致。
    let workers = std::env::var("POLARIS_USAGE_SCAN_PAR")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .clamp(2, 8)
        })
        .min(files.len().max(1));
    let cursor = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let files = std::sync::Arc::new(files);
    let year_cut_shared = std::sync::Arc::new(year_cut.clone());
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let files = std::sync::Arc::clone(&files);
        let cursor = std::sync::Arc::clone(&cursor);
        let year_cut = std::sync::Arc::clone(&year_cut_shared);
        handles.push(std::thread::spawn(move || {
            let mut out: Vec<Rec> = Vec::new();
            loop {
                let i = cursor.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(path) = files.get(i) else { break };
                let Ok(file) = fs::File::open(path) else {
                    continue;
                };
                let reader = BufReader::new(file);
                parse_lines(reader, &year_cut, &mut out);
            }
            out
        }));
    }
    // Pi wins over an explicitly identified legacy copy; no timestamp-only dedup.
    let mut records: Vec<Rec> = Vec::new();
    if let Some(dir) = pi_dir {
        for row in super::pi_usage::read(&dir)? {
            let u = Usage { input: row.usage.input as u64, output: row.usage.output as u64,
                cache_read: row.usage.cache_read as u64, cache_creation: row.usage.cache_write as u64 };
            records.push(Rec { msg_id: Some(row.dedup_key()), date: row.date,
                line_tokens: u.input + u.output + u.cache_read + u.cache_creation,
                cost: row.usage.cost.total, usage: u });
        }
    }
    for h in handles {
        if let Ok(mut v) = h.join() {
            records.append(&mut v);
        }
    }

    for r in records {
        if let Some(mid) = r.msg_id {
            if !seen.insert(mid) {
                continue;
            }
        }
        let (u, cost, date, line_tokens) = (r.usage, r.cost, r.date, r.line_tokens);
        if date.as_str() >= year_cut.as_str() {
            year.add(&u, cost);
            if date.as_str() >= month_cut.as_str() {
                month.add(&u, cost);
                if date.as_str() >= week_cut.as_str() {
                    week.add(&u, cost);
                    if date == today_str {
                        today.add(&u, cost);
                    }
                }
            }
        }
        if trend_set.contains(&date) {
            let e = by_day.entry(date).or_insert((0, 0.0));
            e.0 += line_tokens;
            e.1 += cost;
        }
    }

    // 逐行解析(从原地内联的循环原样搬出,判定顺序与取值一字未改)。
    // `year_cut`:一年窗口之外的行对四个桶和 14 天趋势都无贡献,直接在解析阶段丢掉,
    // 省下把它们搬进汇总阶段的内存与去重成本(旧实现是一路带到最后才被条件过滤掉)。
    fn parse_lines<R: BufRead>(reader: R, year_cut: &str, out: &mut Vec<Rec>) {
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() || !line.contains("\"usage\"") {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                continue;
            }
            let Some(msg) = v.get("message") else {
                continue;
            };
            let Some(usage_v) = msg.get("usage") else {
                continue;
            };
            // msg_id 去重挪到汇总阶段做:重复消息可能横跨两个文件(会话续写/镜像),
            // 只有在主线程汇总时才看得见全局,分线程各自去重会漏。重复行的 usage 值相同,
            // 谁先谁后不影响结果 —— 与旧的单线程「先到先得」等价。
            let u = Usage {
                input: usage_v
                    .get("input_tokens")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0),
                output: usage_v
                    .get("output_tokens")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0),
                cache_read: usage_v
                    .get("cache_read_input_tokens")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0),
                cache_creation: usage_v
                    .get("cache_creation_input_tokens")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0),
            };
            let line_tokens = u.input + u.output + u.cache_read + u.cache_creation;
            if line_tokens == 0 {
                continue;
            }
            let model = msg.get("model").and_then(|x| x.as_str()).unwrap_or("");
            let cost = line_cost(&u, model);

            let date = v
                .get("timestamp")
                .and_then(|t| t.as_str())
                .map(|s| s.chars().take(10).collect::<String>())
                .unwrap_or_default();
            if date.is_empty() {
                continue;
            }
            // 一年窗口之外的行对任何一个桶都无贡献 —— 提前丢,别搬进汇总阶段。
            if date.as_str() < year_cut {
                continue;
            }

            out.push(Rec {
                msg_id: msg
                    .get("id")
                    .and_then(|x| x.as_str())
                    .map(|s| format!("request:{date}:{s}")),
                date,
                usage: u,
                cost,
                line_tokens,
            });
        }
    }

    let daily: Vec<DailyUsage> = trend_window
        .into_iter()
        .map(|(date, label)| {
            let (total, cost) = by_day.get(&date).copied().unwrap_or((0, 0.0));
            DailyUsage {
                date,
                label,
                total,
                cost,
            }
        })
        .collect();

    Ok(UsageSummary {
        available: true,
        today,
        week,
        month,
        year,
        daily,
    })
}

fn empty_summary() -> UsageSummary {
    UsageSummary {
        available: false,
        today: TokenBucket::default(),
        week: TokenBucket::default(),
        month: TokenBucket::default(),
        year: TokenBucket::default(),
        daily: Vec::new(),
    }
}
