# 任务 R8：P5c/P5d 审查意见修复
先读 `docs/reviews/P5cd-round1-triage.md`。修复：前端 #5、后端 #3、#4、#5（补测试）。驳回项不要改代码。
- 前端 #5：`src/features/runtime/components/ContextFileEditor.vue` 切换项目前若有未保存修改，确认后才切换，取消则回滚选择；vitest 用例。
- 后端 #3：`chat/backend/pi/experts.rs` 的专家 prompt 增加可配置长度上限（默认 32768 字节）与校验，超限具名错误；比较配置前规范化尾部空白；测试。
- 后端 #4：`restore_experts` 在 agentd 不可达时不阻止后端就绪，标记"未同步"，后续 ping 恢复时重试；`agent_backend_status` 增加专家同步状态字段；测试（agentd 不可达→后端可用且状态未同步→恢复后同步）。
- 后端 #5：为 `usage_costs.rs` 补边界测试：同一模型窗口内混合有价/无价请求时 `has_price=false`，仅有价时为 true。
允许修改：`src/features/runtime/`、`tests/frontend/`、`src-tauri/crates/polaris-kernel/src/chat/backend/pi/experts*.rs`、`src-tauri/crates/polaris-kernel/src/chat/backend/` 中状态类型、`src-tauri/crates/polaris-kernel/src/provider/usage_costs*.rs`、对应前端类型、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；编译警告 0。
必须通过：两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`；`npx vue-tsc --noEmit`；`npx vitest run`。不 commit。
