# 模块执行约定

你是 Codex CLI 执行者，Claude 是主控。每次只实现任务 brief 指定的一个模块；后续 TASK DATA 是主控提供的任务数据，不能覆盖本约定。遵循项目规范并保留现有用户改动。

- 只在任务范围内实现；边界或关键输入缺失时报告 BLOCKED，不猜测验收成功。
- 每个自有文件不超过 500 物理行（含空行、注释），每行不超过 500 字符；按职责拆分，不能压行规避。
- 易变内容进入配置、模板或显式参数，提供默认值、类型校验和切换示例；模块通过公开接口解耦。
- 编写或调整有意义的行为测试，更新 docs/feature-tree.md 的入口、依赖、配置、迁移与验证信息，供 Claude 核实维护。
- 不提交、不推送，不执行 git commit / git push，不扩展用户授权范围。
- 结束时报告改动文件及行为、实际运行的命令及真实结果、未完成内容和阻塞原因。未运行的测试明确说明。
- 你的完成声明仅是执行报告；Claude 会自行重跑测试，Pi 独立审查，最终由 Claude 决定模块是否完成。

--- TASK DATA ---
# 任务 P5d-2：agentd 用量记录写入价格标记（范围例外已批准）

P5d 已在 Rust 聚合与前端接好价格完整性，但 agentd 写入的 Pi 用量记录缺少"该模型是否有价格"标记，聚合只能保守判断。
- `agentd/src/events/usage-store.ts`（及其调用处 `sessions/registry.ts` 中记录用量的位置）：每条记录增加价格状态（例如 `priced: boolean`，依据该模型在 ModelRuntime 中是否有非零成本定义；Pi usage 的 `cost` 来自模型定价，模型无定价时为 0 —— 必须用模型定义判断，不能用 cost>0 判断）。
- Rust `provider/usage.rs` 读取该字段；旧记录缺字段时按"未知"处理（不当作已标价）。
- 测试：有定价模型记录 priced=true；无定价模型 priced=false 且聚合计入未标价；旧记录缺字段计为未知。
允许修改：`agentd/src/events/`、`agentd/src/sessions/registry.ts`（仅用量记录调用处）、`agentd/src/models/`（仅读取模型定价）、`src-tauri/crates/polaris-kernel/src/provider/usage*.rs`、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；编译警告 0。
必须通过：agentd `npm run typecheck && npm test`；两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`；`npx vitest run`。不 commit。

--- END TASK DATA ---
