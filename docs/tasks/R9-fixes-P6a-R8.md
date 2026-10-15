# 任务 R9：P6a 与 R8 第 2 轮审查采纳项
先读 `docs/reviews/P6a-round1-triage.md`。修复：P6a #5、#7、#3（仅文档）与 R8 #1、#3（仅文档）。驳回项不要改代码。
- P6a #5：吊销清单缓存验签失败时清除缓存并在下次心跳重拉；测试（篡改缓存 → 高风险操作被拒 → 下次心跳恢复）。
- P6a #7：删除已失效的冷静期逻辑（`grants/contract.rs` 的 `in_cooldown`、`COOLDOWN_SECS` 等）及其仅为冷静期存在的测试，确认无调用方。
- P6a #3：在 feature-tree 互联节点写明 relay 列表的信任边界（HTTPS + iroh 端到端加密，篡改最坏拒绝服务）。
- R8 #1：`BackendStatus` 生产代码统一显式构造；`Default` 改为仅测试可用，或默认 `experts_synced: Some(false)`，二选一并说明。
- R8 #3：专家 prompt 上限以字节计，写入配置注释与文档。
允许修改：`src-tauri/crates/polaris-collab/src/collab/`、`src-tauri/crates/polaris-kernel/src/chat/backend/mod.rs` 及受影响的构造处、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；三种构建组合 0 警告。
必须通过：三种 `cargo check --offline`（0 警告）；`cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`；`cargo test -p polaris-kernel -p polaris-wiki --lib --tests --offline`。不 commit。
