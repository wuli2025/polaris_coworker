//! Environment doctor: Pi runtime diagnostics by default.
//!
//! `pi` checks Node, the daemon entry, protocol/version handshake and P4 shared
//! credential readiness. It never installs Claude. Legacy CLI probes, installers
//! and verification are retained behind the explicit `claude-cli` backend.
//! Optional bundled Python/uv and platform PATH utilities remain shared.

pub mod pi;
pub mod autopilot;
pub mod bundled;
pub mod check;
pub mod install;
pub mod path;
pub mod probe;
pub mod types;
pub mod update;
pub mod uv_cache;
pub mod verify;

// tauri::command 生成的 __cmd__* / __tauri_command_name_* 隐藏宏项会被 glob 一并带出,
// 故 lib.rs 的 generate_handler!(doctor::xxx) 路径零改动。原 doctor.rs 的全部对外符号
// (crate::doctor::xxx) 经下列 glob re-export 保持不变。
pub use autopilot::*;
pub use check::*;
pub use install::*;
pub use path::*;
pub use probe::*;
pub use types::*;
pub use update::*;
pub use uv_cache::*;
pub use verify::*;
