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
# 任务 R8：P5c/P5d 审查意见修复
先读 `docs/reviews/P5cd-round1-triage.md`。修复：前端 #5、后端 #3、#4、#5（补测试）。驳回项不要改代码。
- 前端 #5：`src/features/runtime/components/ContextFileEditor.vue` 切换项目前若有未保存修改，确认后才切换，取消则回滚选择；vitest 用例。
- 后端 #3：`chat/backend/pi/experts.rs` 的专家 prompt 增加可配置长度上限（默认 32768 字节）与校验，超限具名错误；比较配置前规范化尾部空白；测试。
- 后端 #4：`restore_experts` 在 agentd 不可达时不阻止后端就绪，标记"未同步"，后续 ping 恢复时重试；`agent_backend_status` 增加专家同步状态字段；测试（agentd 不可达→后端可用且状态未同步→恢复后同步）。
- 后端 #5：为 `usage_costs.rs` 补边界测试：同一模型窗口内混合有价/无价请求时 `has_price=false`，仅有价时为 true。
允许修改：`src/features/runtime/`、`tests/frontend/`、`src-tauri/crates/polaris-kernel/src/chat/backend/pi/experts*.rs`、`src-tauri/crates/polaris-kernel/src/chat/backend/` 中状态类型、`src-tauri/crates/polaris-kernel/src/provider/usage_costs*.rs`、对应前端类型、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；编译警告 0。
必须通过：两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`；`npx vue-tsc --noEmit`；`npx vitest run`。不 commit。

## 范围例外批复（第 2 次运行，Claude）
后端 #4 允许修改 `src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs`、`pi/supervisor.rs`（仅专家同步的启动/恢复接线与状态字段）。注意：P6a 正在并行修改 `polaris-collab` 与 `src-tauri/src` 中的互联代码，服务端壳 `collab-net` 组合的编译错误来自 P6a 进行中的改动，本任务不要触碰 collab 与互联文件；验证时桌面壳与内核测试为准，服务端壳结果单独说明。其余已完成部分保留。

--- END TASK DATA ---
