//! 横切基建(runtime):路径解析 / 子进程池 / HTTP 客户端的唯一入口。
//! 双壳(desktop/server)共用,不依赖 tauri,任何模块都可安全引用。
pub mod http;
pub mod paths;
pub mod procs;
