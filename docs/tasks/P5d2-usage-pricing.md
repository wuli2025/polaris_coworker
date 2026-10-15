# 任务 P5d-2：agentd 用量记录写入价格标记（范围例外已批准）

P5d 已在 Rust 聚合与前端接好价格完整性，但 agentd 写入的 Pi 用量记录缺少"该模型是否有价格"标记，聚合只能保守判断。
- `agentd/src/events/usage-store.ts`（及其调用处 `sessions/registry.ts` 中记录用量的位置）：每条记录增加价格状态（例如 `priced: boolean`，依据该模型在 ModelRuntime 中是否有非零成本定义；Pi usage 的 `cost` 来自模型定价，模型无定价时为 0 —— 必须用模型定义判断，不能用 cost>0 判断）。
- Rust `provider/usage.rs` 读取该字段；旧记录缺字段时按"未知"处理（不当作已标价）。
- 测试：有定价模型记录 priced=true；无定价模型 priced=false 且聚合计入未标价；旧记录缺字段计为未知。
允许修改：`agentd/src/events/`、`agentd/src/sessions/registry.ts`（仅用量记录调用处）、`agentd/src/models/`（仅读取模型定价）、`src-tauri/crates/polaris-kernel/src/provider/usage*.rs`、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；编译警告 0。
必须通过：agentd `npm run typecheck && npm test`；两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`；`npx vitest run`。不 commit。
