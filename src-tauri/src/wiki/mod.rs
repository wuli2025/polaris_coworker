//! llmwiki 库 · 知识网构建域 (分仓规划 v2 · 第 12 仓 polaris-wiki 的仓内雏形)
//!
//! 定位: 检索是「读」, 这里是「写的那一半」—— 摄入资料时让 LLM 抽实体/概念、headless
//! 写 wiki 词条、维护双链知识网(Karpathy LLM-Wiki 思路)。与检索引擎(kb/fable)的关系:
//! wiki 骑在检索之上(依赖层级 3→2), 是全体系唯一获批依赖其他引擎的板块。
//!
//! Phase 0: 先把构建管线(原 kb/compile.rs)归位到本目录; `kb::compile` 旧路径经
//! kb/mod.rs 的 re-export 保持零变化, 抽仓时删别名、调用方一次性切 `wiki::compile`。
//! 词条数据本体在用户数据目录(~/Polaris), 不随代码走。

pub mod compile;
