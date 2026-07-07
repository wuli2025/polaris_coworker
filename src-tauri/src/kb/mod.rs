//! 板块 ② 维基知识库 — MVP 实现
//!
//! 设计依据: PRD-v6 §8 + v5.1 §3-§7
//! - 三层目录铁律: raw/ output/ wiki/ (新建空 KB 时创建)
//! - 关键词加权评分搜索 (PRD §8.8): 标题 +10, 课程标签 +8, 正文 +1
//! - 双链 [[wiki-link]] 解析 -> 图谱节点+边
//! - YAML frontmatter 提取 category (PRD §8.5)
//!
//! MVP 缩水:
//! - 不做 Embedding (Karpathy 论点: 结构化 wiki + 长上下文 > 向量)
//! - 不做 SimHash 去重 (留 §8.6, 后续接入)
//! - 索引常驻内存, 进程重启时重扫 (后续走 SQLite)


// 模块拆分(纯移动): 原 `crate::kb::xxx` 公有路径经 `pub use 子模块::*` 门面保持零变化,
// lib.rs generate_handler! 与 server/echo/fable 等外部引用一律不用改。

pub mod scan;
pub mod packs;
pub mod compile;
pub mod enrich;
pub mod access;
pub mod search;
pub mod ingest;
pub mod graph;
pub mod threat;

// 共享依赖统一在此升为 pub(crate) 供子模块 `use super::*` 取用(与原单文件同一作用域语义)。
pub(crate) use crate::convert;
pub(crate) use anyhow::Result;
pub(crate) use directories::{ProjectDirs, UserDirs};
pub(crate) use once_cell::sync::Lazy;
pub(crate) use parking_lot::RwLock;
pub(crate) use regex::Regex;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use std::collections::HashMap;
pub(crate) use std::fs;
pub(crate) use std::io::{BufRead, BufReader};
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::process::{Command, Stdio};
pub(crate) use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
pub(crate) use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(feature = "desktop")]
pub(crate) use tauri::{AppHandle, Emitter, Manager};
#[cfg(not(feature = "desktop"))]
pub(crate) use crate::host::AppHandle;
pub(crate) use walkdir::WalkDir;


pub use scan::*;
pub use packs::*;
pub use compile::*;
pub use enrich::*;
pub use access::*;
pub use search::*;
pub use ingest::*;
pub use graph::*;
pub use threat::*;
