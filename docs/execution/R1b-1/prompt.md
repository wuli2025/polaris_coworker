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
# 任务 R1b：Rust 侧轮次超时改为空闲超时（P2-5）

范围例外已批准：本任务允许修改 `src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs`、`backend/config.rs`、`backend/bridge.rs`、`pi/supervisor.rs`、`pi/routes.rs`、`pi/tests.rs`、`tests/backend_config.rs` 与 `docs/`。

要求（来自 `docs/reviews/P1-P2-round1-triage.md` P2-5）：
- 把固定 `turnTimeoutMs` 改为 `turnIdleTimeoutMs`（自上一个 agentd 事件起多久无事件判定 agentd 失联，默认 240000，须大于 agentd 默认 `turnIdleTimeoutMs`=180000，在配置注释与文档写明原因）+ `turnMaxMs`（硬上限，默认 0=不限）。旧字段 `turnTimeoutMs` 若出现在配置里，报具名错误并提示新字段名（不静默忽略）。
- 超时收尾为 error+done，错误码区分空闲与硬上限。
- 测试：持续有事件的长轮次（超过空闲阈值数倍）不被误杀；无事件时按空闲超时收尾；硬上限生效；旧字段报错。
- 规则同前：无新依赖、≤500 行、≤500 字符、新代码无 unwrap/expect；`cargo check`（两壳）与 `cargo test -p polaris-kernel --lib --tests --offline` 通过（基线 4 项与沙箱 TCP 项除外并列出）。不 commit。

--- END TASK DATA ---
