//! 多人协作模块族（v8 方案第六节）。
//!
//! 铁律:主 Agent 裁决内容,永不裁决权限——权限判断全部走 db.rs 的确定性授权表。
pub mod account_store;
/// 应用数据面 HTTP(invoke/upload/file/ws,双壳共用):server 壳与桌面 hosting 各自 merge。
#[cfg(feature = "collab-host")]
pub mod apihub;
pub mod auth;
/// 任务级多轮对话(协作者↔负责人↔主 Agent 的微调通道)。
pub mod chat;
/// 任务卡检查工作流(CI-lite):worktree 跑开源工具链+密钥扫描+大文件闸。
pub mod checks;
#[cfg(feature = "desktop")]
pub mod commands;
pub mod db;
pub mod gitea;
/// 桌面内嵌协作主机(一键当主机)。
#[cfg(feature = "desktop")]
pub mod hosting;
/// 协作 HTTP 路由(axum,双壳共用):server 壳 merge 它;桌面 hosting 内嵌它。
#[cfg(feature = "collab-host")]
pub mod http;
pub mod identity;
pub mod lead;
pub mod lead_ai;
pub mod mergectl;
pub mod projects;
pub mod tasks;
pub mod teams;
pub mod workset;
// iroh 组网隧道:依赖树大,只随 collab-net feature 编译。
/// 云机中继网关:注册桌面主机 + 单 Endpoint 多主机 iroh 客户端 + /h/:id HTTP 反代。
/// 需 iroh(collab-net)+ axum(collab-host);云端 `--features server,collab-net` 构建。
#[cfg(all(feature = "collab-net", feature = "collab-host"))]
pub mod gateway;
#[cfg(feature = "collab-net")]
pub mod tunnel;
