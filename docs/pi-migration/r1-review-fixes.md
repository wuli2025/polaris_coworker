# R1：P1/P2 第 1 轮采纳项执行报告

历史记录：下文 P2-5 的范围阻塞已在 R1b 获准解除，后续实现与验证见 [R1b 报告](r1b-idle-timeout.md)。

本报告仅为 Codex 执行证据，由 Claude 重跑验证、Pi 独立审查后决定模块是否完成。
仅处理 `docs/reviews/P1-P2-round1-triage.md` 的采纳/部分采纳项；未改驳回项、未提交或推送、未新增依赖。
保留任务开始前已有的未跟踪 `docs/execution/`。

## 条目结果

| 条目 | 执行结果 | 入口、行为和验证 |
| --- | --- | --- |
| P1-2 | 已实现并测试 | `agentd/src/models/credentials.ts` 的公开 `Redactor.add()` 同时登记原文、encodeURIComponent、UTF-8 base64、JSON 转义内容。`runtime.ts` 的 create 在构建运行时前登记已有凭据，apply 继续在构建前登记新 Key。新增 `credentials-review.test.ts` 覆盖两条入口，每种表示均经过实际 Logger、SDK 工具参数和结果事件并断言替换。 |
| P2-1 | 已实现并测试 | `pi/routes.rs` Drop 释放 reservation 和 active；`pi/supervisor.rs` State Drop 清空 active，覆盖正常退出、future 被取消和线程 unwind。测试覆盖未 drain 的 Route、熔断和 shutdown 后 config.apply 不再 ConfigBusy，以及 supervisor 退出清理模拟孤立 active 项。熔断/停止后仍可返回 CircuitOpen/SupervisorStopped，不宣称 daemon 可用。 |
| P2-2 | 已实现并测试 | Route 保存首个致命错误；fail 的后续原因写 secondary-error 日志，不覆盖或重复向前端发送错误。测试覆盖 daemon error/completed 和本地 fail 两种首错，再模拟 abort 超时，收到首错+done。工具错误不占用首个致命错误。 |
| P2-3 | 已加注释 | `pi/translate.rs` 明确当前前端契约无 reasoning/usage 事件，后续通过可选字段扩展；保留现有丢弃行为。原翻译行为测试通过。 |
| P2-4 | 已实现并测试 | `pi/process.rs` 区分队列满/写阻塞的 AgentdWriterBackpressure 与写管道关闭的 AgentdWriterClosed；异步写入失败和未完成写入状态传给 supervisor。`lastError` 和结构化 failure 日志保留根因，包括熔断时。caller 等待保留具体错误并区分 supervisor 断连；测试覆盖满队列、关闭管道、真实 stdin 阻塞后 caller/status/日志中的背压码。 |
| P2-5 | **BLOCKED，未修改实现** | `pi/mod.rs:85` 仍直接引用 `client.config.turn_timeout_ms`，负责把轮次 timeout 传给 Control::Register。它不在任务允许修改清单；删除旧配置字段需同步修改此入口。已请求这个最小范围例外，尚未获答复。未越界修改、未保留伪兼容字段冒充迁移完成。 |

## 依赖、配置与迁移

依赖和公开协议不变；Redactor 复用 Node Buffer/encodeURIComponent/JSON.stringify。
Route 通过 Translator/Mailbox 工作，Process 写入诊断经 supervisor 汇总到 BackendStatus；不新增前端事件。
脱敏处理完整字符串表示；没有新增跨 text.delta 分片缓存，也没有承诺识别任意 `user:key` Basic 组合。

P2-4 沿用 `requestTimeoutMs`、`supervisorTickMs`、`channelCapacity`、`failureThreshold`，默认分别为 15000、10、64、5，现有正数类型校验不变。
客户端兜底等待为 `2 * requestTimeoutMs + 2 * supervisorTickMs`：分别为排队/序列化和 actor 请求处理留预算，优先接收 actor 的具体失败。
actor 请求及单次管道写入仍使用 requestTimeoutMs；更新状态和回复后再等待子进程回收。
诊断切换示例（宿主 `POLARIS_AGENT_CONFIG` JSON 的片段）：

```json
{"agentd":{"requestTimeoutMs":3000,"supervisorTickMs":10,"channelCapacity":2,"failureThreshold":1}}
```

P2-5 尚未迁移，当前 Rust 仍支持 `turnTimeoutMs`（默认 600000），仍有固定总时长问题。
获准后应替换为 `turnIdleTimeoutMs: 240000`（正整数）与 `turnMaxMs: 0`（非负整数，0 不限）；
Rust 空闲兜底必须晚于 agentd 默认 180000，让 agentd 先执行自身超时清理，再处理失联情形。
每个有效轮次事件应刷新空闲计时；硬上限独立计时，bridge 不能因被丢弃的 reasoning/usage 没有 UI 事件而误判失联。
旧配置的迁移示例是删除 turnTimeoutMs，改填上述两项；**当前代码尚不接受这些 Rust 配置字段**。
持续有事件的长轮次、无事件 error+done 和可选硬上限测试尚未编写/运行，不能宣称 P2-5 通过。

## 实际验证

| 命令 | 真实结果 |
| --- | --- |
| `cd agentd && npm run typecheck && npm test` | 类型检查通过；npm test 退出 1：11 个测试文件通过，仅原有 e2e 文件失败。新增凭据测试通过。 |
| `cd agentd && node --import tsx test/credentials-review.test.ts` | 退出 0，2 项通过，覆盖 startup/apply × 4 种表示。 |
| `cd agentd && node --import tsx test/e2e.test.ts` | 退出 1；首次 ping 等待 5 秒后失败：`Timed out waiting for condition; exit=0; stderr=`。 |
| `/tmp/polaris-r1-agentd-baseline` 中 `node --import tsx test/e2e.test.ts` | 退出 1，同一 ping 失败。临时副本将本次修改的两个生产 TS 文件替换为 git HEAD 内容，未改工作区；确认修改前亦复现，未确定退出根因，不把它宣称为已验证的 TCP 限制。 |
| `cd src-tauri && cargo check -p polaris-app --offline` | 退出 0，有既有警告。 |
| `cd src-tauri && cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0，有既有警告。 |
| `cd src-tauri && cargo test -p polaris-kernel --lib --tests --offline` | 退出 101：134 通过、10 失败、2 忽略。如下分别列出基线和 TCP 受限项，不扩大 baseline。 |
| `cd src-tauri && cargo test -p polaris-kernel --test backend_config --offline` | 退出 0：2 通过。完整命令因 lib 失败未继续集成测试，因此单独运行。 |
| `cd src-tauri && cargo test -p polaris-kernel --lib chat::backend --offline -- --nocapture` | 34 通过；日志含 AgentdWriterBackpressure，首错测试日志含 secondary-error。 |
| `git diff --check`、改动文件行数/行长检查 | 通过；每个修改/新增自有文件不超过 500 行，每行不超过 500 字符。 |

四个既有 Windows 路径基线失败：

- `doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix`
- `doctor::path::tests::prime_and_prepend_behaviour`
- `doctor::verify::tests::install_root_groups_npm_entries_together`
- `doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts`

六个沙箱 TCP 绑定失败（前四个直接报 PermissionDenied/Operation not permitted，后两个报本地路由端口无法绑定，与 P2 记录一致）：

- `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`
- `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`
- `provider::claude_login::claude_oauth_tests::loopback_run_end_to_end_local`
- `provider::claude_login::claude_oauth_tests::loopback_run_rejects_wrong_state`
- `provider::store::local_route_tests::openai_provider_routes_via_local_router`
- `provider::store::local_route_tests::route_all_switch_reroutes_anthropic_provider`

调试过程确实运行过并遇到失败：新增脱敏测试最初使用 text.delta（发现编码串会跨分片，随后改为验证完整工具事件）；
背压测试首次与其他测试并行时兜底超时抢先返回（修正等待预算后完整测试及定向测试通过）；
Node 22 不接受 `--test-isolation=none`（退出 9，改用直接执行测试文件获取详细输出）。
没有运行真实模型、Windows 平台验证或 P2-5 未实现的超时迁移测试。
