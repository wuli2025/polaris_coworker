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
# 任务 F1：消除偶发失败测试（时序依赖）

全量并行运行时偶发失败（单独运行都通过），说明测试依赖固定时长而非确定性信号：
- agentd `test/e2e.test.ts` "main.ts subprocess speaks JSONL ..."：全量 `npm test` 约 3/6 次失败（首次 ping 超时）；单独运行 4/4 通过。
- 内核 `chat::backend::pi::experts::recovery_tests::idle_daemon_reopen_restores_experts_before_releasing_requests`：全量时偶发，耗时 5s；单独 0.2s 通过。
- 内核 `chat::backend::pi::tests::idle_reaping_and_reopen_after_config_apply`：全量约 1/5 失败。

要求：
1. 找出根因并在结果中写明（固定 sleep、过短 deadline、与其他测试共享全局状态/端口/目录、并发争用等）。
2. 改为确定性同步：等待明确的就绪/状态事件，deadline 作为兜底且足够宽（可配置常量，并说明取值理由）；共享的全局状态隔离到每个测试。不得通过跳过测试、标 ignore、降低断言来"修复"。
3. 同时排查同目录下其他使用固定 sleep/短超时的测试，一并加固并列出清单。
4. 证据：agentd `npm test` 连续 15 次、`cargo test -p polaris-kernel --lib` 连续 15 次，上述测试 0 失败（在沙箱里受 TCP 限制的项单独列出）。

允许修改：`agentd/test/`、`agentd/src/`（仅当根因是生产代码的就绪信号缺失时，需说明）、`src-tauri/crates/polaris-kernel/src/chat/backend/`（测试与测试辅助；生产改动需说明根因）、`docs/`。
无新依赖；≤500 行/≤500 字符；三种构建组合 0 警告。不 commit。

--- END TASK DATA ---
