use super::*;

// ───────────────────── 去重 / 新旧冲突(内容指纹驱动)─────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DedupeSummary {
    /// 标记为「完全重复副本」的文件数(其 chunks/lex 已清、指向 canonical)。
    pub exact_dups: u64,
    /// 标记为「被同目录同名新版本压制」的旧文件数(检索时降权,不删)。
    pub superseded: u64,
    /// 因去重删掉的 chunk 数(省下的检索噪声 + 存储)。
    pub chunks_pruned: u64,
    /// 本次补算内容指纹的存量文件数(仅 backfill=true 时 >0)。
    pub backfilled: u64,
    /// 参与去重比较的带指纹文件数。
    pub scanned: u64,
    pub seconds: f64,
    pub stopped: String,
}

/// relpath(以 '/' 分隔)的父目录段;顶层文件返回 ""。
fn parent_dir(rel: &str) -> &str {
    match rel.rfind('/') {
        Some(i) => &rel[..i],
        None => "",
    }
}

/// 内容级去重 + 新压旧标记。**只动索引,绝不碰用户文件**。幂等可重跑:每次全量重算分组标记。
/// - 精确去重:同 content_hash 分组,留 mtime 最新(平局取 relpath 最短)为 canonical,其余标 dup_of
///   并删其 chunks/lex(检索时由融合层归并回 canonical)。
/// - 新压旧:同 (root_id, 目录, ext, doc_key) 分组、内容互异,非最新版标 superseded_by=最新版
///   (检索时降权,保留可达)。
/// - backfill=true:先给存量「已索引但缺指纹」的文本文件补算 content_hash/doc_key(读文件,可取消)。
pub(crate) fn dedupe_scan(backfill: bool) -> Result<DedupeSummary, String> {
    let started = std::time::Instant::now();
    let conn = open_db()?;
    let lex_on = lex_available(&conn);
    let mut backfilled = 0u64;
    let mut stopped = "完成".to_string();

    // ── 可选:补算存量文件的内容指纹(索引早于本特性的库)──
    if backfill {
        let targets: Vec<(i64, String, String)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT f.id, r.path, f.relpath FROM files f JOIN roots r ON r.id=f.root_id
                     WHERE f.kind='text' AND f.size<=?1 AND f.content_hash='' AND f.chunked=1
                     LIMIT 100000",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([MAX_LEX_FILE_BYTES], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
                })
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        for (id, root, rel) in targets {
            if cancelled() {
                stopped = "已取消".into();
                break;
            }
            let abs = super::reencode_fs_path(
                &std::path::Path::new(&root).join(&rel).to_string_lossy(),
            );
            let Ok(bytes) = std::fs::read(&abs) else { continue };
            if bytes.iter().take(4096).any(|&b| b == 0) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let chash = content_fingerprint(&text);
            let name = std::path::Path::new(&rel)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dkey = doc_key(&name);
            let _ = conn.execute(
                "UPDATE files SET content_hash=?1, doc_key=?2 WHERE id=?3",
                rusqlite::params![chash, dkey, id],
            );
            backfilled += 1;
        }
    }

    // ── 精确去重:同 content_hash 分组 ──
    let rows: Vec<(i64, i64, String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, mtime, relpath, content_hash FROM files
                 WHERE content_hash<>'' ORDER BY content_hash",
            )
            .map_err(|e| e.to_string())?;
        let r = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        r.filter_map(|x| x.ok()).collect()
    };
    let scanned = rows.len() as u64;
    let mut exact_dups = 0u64;
    let mut chunks_pruned = 0u64;
    let mut i = 0usize;
    while i < rows.len() {
        if cancelled() {
            stopped = "已取消".into();
            break;
        }
        let hash = &rows[i].3;
        let mut j = i + 1;
        while j < rows.len() && &rows[j].3 == hash {
            j += 1;
        }
        let group = &rows[i..j];
        i = j;
        if group.len() < 2 {
            // 单例:若此前被标 dup(孪生已删)→ 复活为 canonical,重建向量。
            let id = group[0].0;
            let _ = conn.execute(
                "UPDATE files SET dup_of=0, chunked=CASE WHEN dup_of<>0 THEN 0 ELSE chunked END
                 WHERE id=?1 AND dup_of<>0",
                [id],
            );
            continue;
        }
        // canonical = mtime 最新,平局取 relpath 最短
        let mut canon = &group[0];
        for g in group {
            if g.1 > canon.1 || (g.1 == canon.1 && g.2.len() < canon.2.len()) {
                canon = g;
            }
        }
        let canon_id = canon.0;
        // canonical 必须 dup_of=0;若它此前是 dup(chunks 已删)→ chunked=0 触发重建。
        let _ = conn.execute(
            "UPDATE files SET dup_of=0, chunked=CASE WHEN dup_of<>0 THEN 0 ELSE chunked END WHERE id=?1",
            [canon_id],
        );
        for g in group {
            if g.0 == canon_id {
                continue;
            }
            let _ = conn.execute(
                "UPDATE files SET dup_of=?2, superseded_by=0 WHERE id=?1",
                rusqlite::params![g.0, canon_id],
            );
            let pruned = conn
                .execute("DELETE FROM chunks WHERE file_id=?1", [g.0])
                .unwrap_or(0);
            chunks_pruned += pruned as u64;
            if lex_on {
                let _ = conn.execute("DELETE FROM lex WHERE rowid=?1", [g.0]);
            }
            exact_dups += 1;
        }
    }

    // ── 新压旧:同 (root_id, 目录, ext, doc_key) 分组,内容互异,非最新 mtime 者降权 ──
    // 先整体清零(幂等),再全量重标;只在 dup_of=0(未被精确去重折叠)的文件间比较。
    let _ = conn.execute("UPDATE files SET superseded_by=0 WHERE superseded_by<>0", []);
    let mut superseded = 0u64;
    if !cancelled() {
        let srows: Vec<(i64, i64, i64, String, String, String, String)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT id, root_id, mtime, relpath, ext, doc_key, content_hash FROM files
                     WHERE doc_key<>'' AND dup_of=0",
                )
                .map_err(|e| e.to_string())?;
            let r = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            r.filter_map(|x| x.ok()).collect()
        };
        // 分组:key=(root_id, 目录, ext, doc_key) → Vec<(id, mtime, content_hash)>
        let mut groups: std::collections::HashMap<
            (i64, String, String, String),
            Vec<(i64, i64, String)>,
        > = std::collections::HashMap::new();
        for (id, root_id, mtime, relpath, ext, dkey, chash) in srows {
            let pdir = parent_dir(&relpath).to_string();
            groups
                .entry((root_id, pdir, ext, dkey))
                .or_default()
                .push((id, mtime, chash));
        }
        for (_key, mut members) in groups {
            if members.len() < 2 || cancelled() {
                continue;
            }
            members.sort_by(|a, b| b.1.cmp(&a.1)); // mtime 降序
            let (newest_id, newest_mtime, newest_hash) =
                (members[0].0, members[0].1, members[0].2.clone());
            for m in members.iter().skip(1) {
                // 仅压制「确实更旧(mtime 更小)且内容不同」的版本
                if m.1 < newest_mtime && m.2 != newest_hash {
                    let _ = conn.execute(
                        "UPDATE files SET superseded_by=?2 WHERE id=?1",
                        rusqlite::params![m.0, newest_id],
                    );
                    superseded += 1;
                }
            }
        }
    }

    Ok(DedupeSummary {
        exact_dups,
        superseded,
        chunks_pruned,
        backfilled,
        scanned,
        seconds: started.elapsed().as_secs_f64(),
        stopped,
    })
}

/// 去重扫描(桌面 async + spawn_blocking 防主线程阻塞;与索引/盘点共用 INDEXING 闸)。
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn fable_dedupe_scan(backfill: Option<bool>) -> Result<DedupeSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(_guard) = FlagGuard::acquire(&INDEXING) else {
            return Err("索引任务进行中,稍后再去重".into());
        };
        CANCEL.store(false, Ordering::SeqCst);
        dedupe_scan(backfill.unwrap_or(false))
    })
    .await
    .map_err(|e| format!("任务调度失败: {e}"))?
}
#[cfg(not(feature = "desktop"))]
pub fn fable_dedupe_scan(backfill: Option<bool>) -> Result<DedupeSummary, String> {
    let Some(_guard) = FlagGuard::acquire(&INDEXING) else {
        return Err("索引任务进行中,稍后再去重".into());
    };
    CANCEL.store(false, Ordering::SeqCst);
    dedupe_scan(backfill.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 去重 + 新压旧端到端(默认 `#[ignore]`,单独跑避开进程级 DB 竞争):
    /// `cargo test --manifest-path src-tauri/Cargo.toml --lib dedupe_scan_e2e -- --ignored --exact --nocapture`
    #[test]
    #[ignore]
    fn dedupe_scan_e2e() {
        let base = std::env::temp_dir().join(format!("polaris_dedupe_{}", std::process::id()));
        let db = base.join("fable_test.db");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::env::set_var("POLARIS_FABLE_DB", &db);
        let conn = open_db().unwrap();
        conn.execute("INSERT INTO roots(id, path) VALUES(1, '/r')", []).unwrap();
        let ins = |id: i64, rel: &str, name: &str, mtime: i64, hash: &str, dkey: &str| {
            conn.execute(
                "INSERT INTO files(id, root_id, relpath, name, ext, kind, size, mtime, chunked, ftsed, seen, content_hash, doc_key)
                 VALUES(?1,1,?2,?3,'txt','text',10,?4,1,1,1,?5,?6)",
                rusqlite::params![id, rel, name, mtime, hash, dkey],
            )
            .unwrap();
        };
        // 完全重复:同 hash、mtime 不同 → canonical = 最新(id=2),id=1 标 dup_of=2 且 chunk 被清。
        ins(1, "a/report.txt", "report.txt", 100, "f:aaa", "report");
        ins(2, "b/report.txt", "report.txt", 200, "f:aaa", "report");
        conn.execute(
            "INSERT INTO chunks(file_id,seq,text,dim,vec) VALUES(1,0,'x',1,x'00')",
            [],
        )
        .unwrap();
        // 新压旧:同目录(c)同 ext 同 doc_key、内容互异、mtime 不同 → id=3 被 id=4 压制。
        ins(3, "c/plan.txt", "plan.txt", 100, "f:old", "plan");
        ins(4, "c/plan_v2.txt", "plan_v2.txt", 200, "f:new", "plan");

        let sum = dedupe_scan(false).unwrap();

        let col = |id: i64, c: &str| -> i64 {
            conn.query_row(&format!("SELECT {c} FROM files WHERE id=?1"), [id], |r| r.get(0))
                .unwrap()
        };
        let nchunks = |fid: i64| -> i64 {
            conn.query_row("SELECT COUNT(*) FROM chunks WHERE file_id=?1", [fid], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(col(1, "dup_of"), 2, "旧副本指向最新 canonical");
        assert_eq!(col(2, "dup_of"), 0, "canonical 自身 dup_of=0");
        assert_eq!(nchunks(1), 0, "副本的 chunk 被清");
        assert_eq!(sum.exact_dups, 1);
        assert_eq!(col(3, "superseded_by"), 4, "旧版被新版压制");
        assert_eq!(col(4, "superseded_by"), 0, "新版不被压制");
        assert_eq!(sum.superseded, 1);

        drop(conn);
        std::env::remove_var("POLARIS_FABLE_DB");
        let _ = std::fs::remove_dir_all(&base);
    }
}
