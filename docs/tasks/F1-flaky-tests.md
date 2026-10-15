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
