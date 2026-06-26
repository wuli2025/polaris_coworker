//! 本地开源嵌入 / 重排（fastembed-rs · ONNX）—— 绕开硅基流动 API 的限速与网络往返。
//!
//! 模型与硅基流动**同源**：嵌入 `BAAI/bge-m3`(dense 1024 维)、重排 `bge-reranker-v2-m3`，
//! 故本地产出的向量与既有索引「同空间、兼容」——无需重嵌全库，存量 5309 条照用，
//! 新文件本地灌、查询本地算。
//!
//! 仅 `feature = "local-embed"` 时编译；运行时还需 `POLARIS_LOCAL_EMBED=1` 才真正启用
//! （否则即便编进也走云 API，便于灰度/回退）。首次用时模型下载到 `FASTEMBED_CACHE_DIR`
//! （默认 ~/Polaris/models/fastembed），之后离线加载。
//!
//! 并发：fastembed 推理对象用 `Mutex` 单例守护（ONNX 内部已多线程，外层串行无碍且省内存）。

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use fastembed::{
    EmbeddingModel, RerankInitOptions, RerankerModel, TextEmbedding, TextInitOptions, TextRerank,
};

/// 启用标记文件:UI「启用本地嵌入」开关落盘于此 → 重启仍生效(不必每次设环境变量)。
fn enable_marker() -> Option<PathBuf> {
    directories::UserDirs::new()
        .map(|u| u.home_dir().join("Polaris").join("data").join("local_embed.on"))
}

/// 运行时开关:置 `POLARIS_LOCAL_EMBED=1` **或** UI 勾了「启用本地嵌入」(落盘标记)即切到本地;
/// 否则仍走云 API(便于灰度/回退)。环境变量优先(可被运维强制)。
pub fn enabled() -> bool {
    if std::env::var("POLARIS_LOCAL_EMBED").map(|v| v == "1").unwrap_or(false) {
        return true;
    }
    enable_marker().map(|p| p.exists()).unwrap_or(false)
}

/// 持久化「启用本地嵌入」开关(供 UI 切换)。写/删标记文件。
pub fn set_enabled(on: bool) -> Result<(), String> {
    let p = enable_marker().ok_or("无法定位数据目录")?;
    if on {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("建目录失败: {e}"))?;
        }
        std::fs::write(&p, b"1").map_err(|e| format!("写启用标记失败: {e}"))?;
    } else if p.exists() {
        std::fs::remove_file(&p).map_err(|e| format!("清启用标记失败: {e}"))?;
    }
    Ok(())
}

/// 模型缓存目录(绝对路径,UI 展示用)。
pub fn cache_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("FASTEMBED_CACHE_DIR") {
        return PathBuf::from(v);
    }
    directories::UserDirs::new()
        .map(|u| u.home_dir().join("Polaris").join("models").join("fastembed"))
        .unwrap_or_else(|| PathBuf::from("/root/Polaris/models/fastembed"))
}

/// 模型是否已下载就位:缓存目录里有非空内容(fastembed 首次 init 会把 onnx+tokenizer 落于此)。
pub fn ready() -> bool {
    let dir = cache_dir();
    std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().any(|e| {
            // 任一子项里有体量像样的文件即视为已下过(onnx 动辄数百 MB)
            walk_has_big_file(&e.path())
        }))
        .unwrap_or(false)
}

fn walk_has_big_file(p: &Path) -> bool {
    if let Ok(meta) = std::fs::metadata(p) {
        if meta.is_file() {
            return meta.len() > 5 * 1024 * 1024; // >5MB 视为模型权重
        }
        if meta.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                return rd.flatten().any(|e| walk_has_big_file(&e.path()));
            }
        }
    }
    false
}

/// 强制下载/就位本地模型(嵌入 + 重排)。首次会从 HuggingFace(国内走 hf-mirror)拉权重到
/// [`cache_dir`];已就位则秒回。供 UI「下载本地引擎」按钮调用 —— 把「报告里的头号提速杠杆」
/// (绕开云限速)变成一键可下。返回就位后的缓存目录。
pub fn download() -> Result<String, String> {
    ensure_cache_env();
    // 触发两个模型的 OnceLock 初始化 = 触发下载(fastembed 内部带断点续/校验)。
    embedder()?;
    reranker()?;
    Ok(cache_dir().to_string_lossy().into_owned())
}

/// 模型缓存目录:落数据根,避免每次重下;FASTEMBED_CACHE_DIR / HF_HOME 可覆盖。
fn ensure_cache_env() {
    if std::env::var_os("FASTEMBED_CACHE_DIR").is_none() {
        let dir = cache_dir();
        let _ = std::fs::create_dir_all(&dir);
        std::env::set_var("FASTEMBED_CACHE_DIR", dir);
    }
    // 国内直连 HuggingFace 常被墙 → 默认走 hf-mirror(已显式设过则尊重)。fastembed/hf-hub
    // 读 HF_ENDPOINT 选下载源;这一行让「下载本地模型」在国内也能拉得动。
    if std::env::var_os("HF_ENDPOINT").is_none() {
        std::env::set_var("HF_ENDPOINT", "https://hf-mirror.com");
    }
    cap_onnx_threads();
}

/// 限速旋钮：`POLARIS_EMBED_THREADS` 限制 ONNX Runtime 的推理线程数。本地 bge-m3 嵌入/重排是
/// CPU 密集，ORT 默认会吃满全部逻辑核 → 百万级大库后台索引连续跑时，UI 线程几乎拿不到 CPU
/// 时间片，消息泵错过 5s 窗口被 Windows 判无响应(AppHangB1)。把它设成「核数 - 2」之类给 UI
/// 留头寸。必须在模型(ORT session)创建**之前**设进环境；OpenMP 构建的 onnxruntime 读
/// `OMP_NUM_THREADS`，故同时写入这两个变量(已被显式设过则尊重用户值，不覆盖)。
fn cap_onnx_threads() {
    let Ok(v) = std::env::var("POLARIS_EMBED_THREADS") else { return };
    let Ok(n) = v.trim().parse::<usize>() else { return };
    if n == 0 {
        return;
    }
    let n = n.to_string();
    if std::env::var_os("OMP_NUM_THREADS").is_none() {
        std::env::set_var("OMP_NUM_THREADS", &n);
    }
    if std::env::var_os("ORT_INTRA_OP_NUM_THREADS").is_none() {
        std::env::set_var("ORT_INTRA_OP_NUM_THREADS", &n);
    }
}

static EMBED: OnceLock<Result<Mutex<TextEmbedding>, String>> = OnceLock::new();
static RERANK: OnceLock<Result<Mutex<TextRerank>, String>> = OnceLock::new();

fn embedder() -> Result<&'static Mutex<TextEmbedding>, String> {
    EMBED
        .get_or_init(|| {
            ensure_cache_env();
            TextEmbedding::try_new(TextInitOptions::new(EmbeddingModel::BGEM3))
                .map(Mutex::new)
                .map_err(|e| format!("本地嵌入模型加载失败(BAAI/bge-m3): {e}"))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

fn reranker() -> Result<&'static Mutex<TextRerank>, String> {
    RERANK
        .get_or_init(|| {
            ensure_cache_env();
            TextRerank::try_new(RerankInitOptions::new(RerankerModel::BGERerankerV2M3))
                .map(Mutex::new)
                .map_err(|e| format!("本地重排模型加载失败(bge-reranker-v2-m3): {e}"))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

/// 批量本地嵌入 → 与 `index::embed_texts` 同形(Vec<Vec<f32>>，dense 1024)。
pub fn embed(texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
    let m = embedder()?;
    let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    let mut g = m.lock().map_err(|_| "本地嵌入锁中毒".to_string())?;
    g.embed(refs, None).map_err(|e| format!("本地嵌入失败: {e}"))
}

/// 本地重排 → 与 `index::rerank` 同形((原 index, 分数) 降序,截到 top_n)。
pub fn rerank(query: &str, docs: &[String], top_n: usize) -> Result<Vec<(usize, f32)>, String> {
    let m = reranker()?;
    let refs: Vec<&str> = docs.iter().map(|s| s.as_str()).collect();
    let mut g = m.lock().map_err(|_| "本地重排锁中毒".to_string())?;
    let res = g
        .rerank(query, refs, false, None)
        .map_err(|e| format!("本地重排失败: {e}"))?;
    let mut out: Vec<(usize, f32)> = res.iter().map(|r| (r.index, r.score)).collect();
    out.truncate(top_n);
    Ok(out)
}
