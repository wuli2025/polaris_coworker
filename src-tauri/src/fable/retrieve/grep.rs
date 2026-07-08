//! grep 车道:倒排(FTS5)+ 短词 LIKE + 磁盘兜底子串扫,多核回读候选精确算分抽行。

use super::*;

/// grep 车道单文件上限(超大文本不参与全文扫,靠 agent 的定向 Grep 工具)。
const MAX_GREP_FILE_BYTES: i64 = 4_000_000;
/// grep 车道单次检索的文件数/总字节预算(实时扫描兜底路才用;FTS 倒排路无此上限)。
const MAX_GREP_FILES: i64 = 20_000;
const MAX_GREP_TOTAL_BYTES: u64 = 800 * 1024 * 1024;
/// FTS 倒排命中后,最多回读多少个候选文件做精确算分 + 抽行(按 bm25 相关度优先)。
const FTS_CAND_LIMIT: i64 = 400;

pub(crate) struct GrepHit {
    pub(crate) path: String,
    pub(crate) abspath: String,
    pub(crate) line: usize,
    pub(crate) snippet: String,
    /// 命中行 ± 邻近若干行的上下文窗口(只给重排「读全文打分」用,不展示)。
    pub(crate) context: String,
    pub(crate) score: f32,
}

/// 多核回读候选文件 → 精确算分 + 抽命中行/上下文窗口。FTS 路与实时扫描路共用此算分口径。
/// `byte_budget=Some(n)` 时(实时扫描)按字节预算截断并回报 truncated;`None`(FTS 路)不截断。
fn scan_and_score(
    candidates: Vec<(String, String, i64)>,
    q_full: &str,
    tokens: &[String],
    byte_budget: Option<u64>,
) -> (Vec<GrepHit>, bool) {
    let stack = Mutex::new(candidates);
    let hits = Mutex::new(Vec::<GrepHit>::new());
    let spent = AtomicU64::new(0);
    let truncated = std::sync::atomic::AtomicBool::new(false);

    std::thread::scope(|s| {
        for _ in 0..worker_count() {
            let (stack, hits, spent, truncated) = (&stack, &hits, &spent, &truncated);
            let (q_full, tokens) = (&q_full, &tokens);
            s.spawn(move || loop {
                let item = { stack.lock().unwrap().pop() };
                let Some((root, rel, size)) = item else { break };
                if let Some(budget) = byte_budget {
                    if spent.fetch_add(size as u64, Ordering::Relaxed) > budget {
                        truncated.store(true, Ordering::Relaxed);
                        break;
                    }
                }
                let abs = std::path::Path::new(&root).join(&rel);
                let Ok(bytes) = std::fs::read(&abs) else {
                    continue;
                };
                if bytes.iter().take(4096).any(|&b| b == 0) {
                    continue; // 二进制伪文本
                }
                let text = String::from_utf8_lossy(&bytes);
                let lower = text.to_lowercase();
                let mut score = 0f32;
                if lower.contains(*q_full) {
                    score += 3.0;
                }
                for t in tokens.iter() {
                    if lower.contains(t.as_str()) {
                        score += 1.0;
                    }
                }
                if score <= 0.0 {
                    continue;
                }
                let lines: Vec<&str> = text.lines().collect();
                // 复用整文件的小写副本 `lower`(上面已算)逐行切片做命中判定,免去每行再
                // `to_lowercase()` 分配一个 String(大文件几千行 × 数百候选 = 数百万次分配)。
                // `to_lowercase` 不增删换行符,故 `lower.lines()` 与 `text.lines()` 行号对齐;
                // 仍用 `.get(i)` 防御任何极端不齐。展示/上下文取原文 `lines`(保留大小写)。
                let lower_lines: Vec<&str> = lower.lines().collect();
                // 取最多 2 条命中行做摘录(行号按原文);并截一段上下文窗口给重排读全文。
                let mut snippets = 0;
                for (i, line) in lines.iter().enumerate() {
                    let ll = lower_lines.get(i).copied().unwrap_or("");
                    let hit_full = ll.contains(*q_full);
                    let hit_tok = tokens.iter().any(|t| ll.contains(t.as_str()));
                    if hit_full || hit_tok {
                        let snippet: String = line.trim().chars().take(160).collect();
                        // 命中行 ±2 行拼成上下文窗口(P2-1:让重排专家读到的不只是孤零零一行)。
                        let lo = i.saturating_sub(2);
                        let hi = (i + 3).min(lines.len());
                        let context: String = lines[lo..hi].join("\n").chars().take(700).collect();
                        hits.lock().unwrap().push(GrepHit {
                            path: rel.clone(),
                            abspath: abs.to_string_lossy().into_owned(),
                            line: i + 1,
                            snippet,
                            context,
                            score: score + if hit_full { 0.5 } else { 0.0 },
                        });
                        snippets += 1;
                        if snippets >= 2 {
                            break;
                        }
                    }
                }
            });
        }
    });

    let mut out = hits.into_inner().unwrap();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(60);
    (out, truncated.load(Ordering::Relaxed))
}

/// 认字腿(P1-2 + CJK 修):两路候选合并 ——
/// - **倒排路**(快、可扩展):FTS5 trigram 取 ≥3 码点检索词的候选;自然句靠三元组 OR 也有候选。
/// - **实时扫描路**(子串、覆盖未索引文件):仅当查询含 trigram 服务不了的短词(独立 1~2 字
///   CJK 概念词、2 字拉丁词,如「模型 索引」)、或倒排无候选时才补扫,带字节预算护栏。
///
/// 旧实现的致命缺陷:整句当一个 FTS 短语 → 「模型 索引」「检索 重排」「<整句自然语言>」全部零召回
/// (实测 0 命中)。现在 split_query 把 CJK 句切成概念词、fts_query_expr 用三元组 OR 取候选,
/// scan_and_score 按概念词子串算分,自然句/双关键词都能命中。
pub(crate) fn grep_lane(query: &str) -> Result<(Vec<GrepHit>, bool), String> {
    let (q_full, terms) = split_query(query);
    if q_full.is_empty() {
        return Ok((Vec::new(), false));
    }
    let conn = open_db()?;

    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut fts_cand: Vec<(String, String, i64)> = Vec::new();
    let lex_ok = lex_available(&conn);

    // —— 倒排路 ——
    if lex_ok {
        if let Some(expr) = fts_query_expr(&q_full) {
            let mut stmt = conn
                .prepare(
                    "SELECT r.path, f.relpath, f.size FROM lex l
                     JOIN files f ON f.id=l.rowid JOIN roots r ON r.id=f.root_id
                     WHERE l.body MATCH ?1 ORDER BY bm25(lex) LIMIT ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(rusqlite::params![expr, FTS_CAND_LIMIT], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for row in rows.flatten() {
                if seen.insert((row.0.clone(), row.1.clone())) {
                    fts_cand.push(row);
                }
            }
        }
    }

    // 纯标点/纯空白等「无内容词」查询:倒排也没候选 → 直接收工,别空扫一整轮磁盘(实测省 ~1s)。
    if terms.is_empty() && fts_cand.is_empty() {
        return Ok((Vec::new(), false));
    }

    // —— 短词补召(trigram 服务不了的 ≤2 码点概念词,如「模型」「索引」)——
    // 优先 lex LIKE:读 DB 页(OS 缓存,快)、覆盖**全部**已索引文件,命中后只回读这几百个文件;
    // 比扫 2 万个磁盘文件快一两个数量级(实测 ~1s → 数十 ms)。
    let mut like_cand: Vec<(String, String, i64)> = Vec::new();
    if lex_ok && has_short_terms(&q_full) {
        let shorts: Vec<&String> = terms
            .iter()
            .filter(|t| t.chars().count() <= 2)
            .take(8)
            .collect();
        let mut stmt = conn
            .prepare(
                "SELECT r.path, f.relpath, f.size FROM lex l
                 JOIN files f ON f.id=l.rowid JOIN roots r ON r.id=f.root_id
                 WHERE l.body LIKE ?1 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        for t in shorts {
            // 检索词只含 CJK / 字母数字(atoms 已过滤),不含 LIKE 元字符,直接拼 %term%。
            let pat = format!("%{t}%");
            let rows = stmt
                .query_map(rusqlite::params![pat, FTS_CAND_LIMIT], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for row in rows.flatten() {
                if seen.insert((row.0.clone(), row.1.clone())) {
                    like_cand.push(row);
                }
            }
        }
    }

    // —— 磁盘兜底(有界子串扫盘)——
    // 仅当倒排 + LIKE 都没候选(目标可能落在**未索引**文件里),或 lex 整个没就绪时才扫;
    // 命中索引时绝不触发,把昂贵的全盘扫描留给真正必要的场景。
    let need_disk = !lex_ok || (fts_cand.is_empty() && like_cand.is_empty());
    let scan_cand: Vec<(String, String, i64)> = if need_disk {
        let mut stmt = conn
            .prepare(
                "SELECT r.path, f.relpath, f.size FROM files f JOIN roots r ON r.id=f.root_id
                 WHERE f.kind='text' AND f.size<=?1 ORDER BY f.size ASC LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([MAX_GREP_FILE_BYTES, MAX_GREP_FILES], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.flatten()
            .filter(|row| seen.insert((row.0.clone(), row.1.clone())))
            .collect()
    } else {
        Vec::new()
    };
    drop(conn);

    // 倒排 / LIKE 候选已有界(≤ 几百)→ 无预算、不截断;磁盘兜底候选带字节预算护栏。
    let mut hits = Vec::new();
    let mut truncated = false;
    let indexed_cand: Vec<(String, String, i64)> = fts_cand.into_iter().chain(like_cand).collect();
    if !indexed_cand.is_empty() {
        let (h, _) = scan_and_score(indexed_cand, &q_full, &terms, None);
        hits.extend(h);
    }
    if !scan_cand.is_empty() {
        let (h, t) = scan_and_score(scan_cand, &q_full, &terms, Some(MAX_GREP_TOTAL_BYTES));
        hits.extend(h);
        truncated = t;
    }
    // 两路命中合并后重排、截断(各路内部已 ≤60,合并后再收一次)。
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(60);
    Ok((hits, truncated))
}
