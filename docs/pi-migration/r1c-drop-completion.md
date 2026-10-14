# R1c：Route 与 legacy 读取任务丢弃收尾执行报告

2026-10-14。仅修改授权的 `src-tauri/crates/polaris-kernel/src/chat/backend/` 与 docs。
无新依赖、无提交或推送；保留已有未跟踪 `docs/execution/R1c-1/`。
本报告是执行证据，由 Claude 重跑测试、Pi 独立审查，最终由 Claude 决定模块是否完成。

## 文件与行为

以下 Rust 路径相对 `src-tauri/crates/polaris-kernel/src/chat/backend/`。

| 文件 | 改动 |
| --- | --- |
| `pi/routes.rs` | Route Drop 清理 active/reservation；未 finished 时直接向 Mailbox 保留终态槽写入 AgentdSupervisorExited error + done。新增满队列、未确认、未排出终态及已完成不重复的测试。 |
| `pi/supervisor.rs` | State Drop 先设置 healthy=false、pid=null、lastError，再丢弃 routes、清理孤立 active。控制通道关闭走异常退出；显式 Stop 保留正常排空路径。 |
| `pi/client.rs` | 仅 cfg(test) 的退出/panic/关闭控制通道钩子；生产协议和配置不变。 |
| `pi/tests.rs` | 两个真实在途轮次分别覆盖 supervisor 提前返回、线程 panic 和控制接收端关闭；有界接收 error→done，并验证状态与登记清理。 |
| `claude_cli/completion.rs`（新增） | TurnGuard 持有 mailbox，与 worker/job 同生存期；丢弃时未完成则写入 ClaudeCliWorkerExited error + done，清 capture/取消标记并记录故障状态。包含一次性 worker panic/返回、满 mailbox 与正常终态单测。 |
| `claude_cli/mod.rs` | 准备线程既有 catch_unwind 的 panic 使用同一具名错误并记录状态；status 返回故障记录。 |
| `claude_cli/pipeline/process.rs` | 一次性 stdout 线程持有 TurnGuard；正常 done 后丢弃无副作用。 |
| `claude_cli/pool/mod.rs` | TurnJob 持有 TurnGuard；保护队列中尚未被 reader 取出的轮次。 |
| `claude_cli/pool/lifecycle.rs` | 派发 job 前建立保护；send 失败时解除保护，保留原准备线程重试/报错责任。 |
| `claude_cli/pool/reader.rs` | reader 生命周期保护清 alive/active_req/watchdog；读取迭代器可在单测注入 panic。 |
| `claude_cli/pool/reader_tests.rs`（新增） | 实际常驻读取循环 panic 时 active 与 queued job 都收尾；队列接收端丢弃和发送拒绝后重试行为。 |
| `docs/feature-tree.md` | 增加入口、依赖、配置迁移与验证链接。 |
| `docs/pi-migration/r1c-drop-completion.md`（新增） | 本执行报告。 |

## 收尾责任与边界

Pi 的外部消费者仍通过 AgentBackend/EventStream 接收事件，bridge 轮询超时继续等待。
Route 的异常 Drop 不等待 acknowledged、不等待 consumer 腾出普通队列；已进入 mailbox 的事件仍先于终态。
尚在 Route 内部的 pending/合批文本可能丢失，终态明确报告 supervisor 故障，不伪装成功。
正常完成、取消或已排出的错误终态不会因 Drop 再产生 error/done。
State 在唤醒消费者前发布故障，`agent_backend_status` 直接委托同一 `AgentBackend::status()`。
Pi 状态测试调用这个后端方法，不修改全局 selected 单例来强制切换配置。
显式 Stop 正常完成不写 SupervisorExited；若 Stop 期间 future 被丢弃且仍有 Route，仍报告异常。

legacy 原先并非必然收尾：准备线程 catch_unwind 无法捕获已移交的一次性 stdout/常驻 reader 线程 panic。
新增 guard 覆盖读取线程正常返回但未完成、panic unwind 和接收队列丢弃；常驻 job 发送失败仍允许原逻辑重试。
正常 EOF、取消与 result 收尾沿用原 TurnStream::finish；headless 同步读取已有准备线程 catch_unwind/finish 保护。
异常 guard 仅直接操作 Mailbox，不在析构中执行产物扫描/落库，也不等待事件队列可写。
legacy lastError 在进程内保留，发生 worker/preparation panic 后 healthy=false；重启宿主重置记录。
这表示曾发生后台执行故障，不仅仅是 Claude 可执行文件是否存在。
常驻 reader 退出会标记会话不可复用，沿用原池的后续检出/收割流程；不新增进程回收策略。

## 配置、依赖与迁移

无新增配置或迁移要求；具名错误为生命周期契约，不提供关闭收尾保护的开关。
依赖已有 Mailbox::finish、Translator、Control、AgentBackend 和 legacy capture；无 Cargo 清单/锁文件改动。
既有后端配置默认为 `pi`，类型为字符串，仅允许 `pi` / `claude-cli`，未知值返回 InvalidBackend。
通过 `POLARIS_AGENT_CONFIG` 指向 JSON 切换示例：

```json
{"agent":{"backend":"claude-cli"}}
```

改为 `{"agent":{"backend":"pi"}}` 恢复默认；selected 为进程单例，切换后重启宿主。
legacy 常驻模式继续默认启用；沿用 `POLARIS_PERSISTENT_AGENT=0` 切回一次性 stdout 路径。
此旧环境开关按原字符串语义解析，未新增或加强其校验。
测试钩子仅在 cfg(test) 编译，不是可部署配置；测试截止时间显式使用 1–3 秒，Pi 空闲预算设 60 秒以排除超时误收尾。

## 实际验证

Cargo 命令在 `src-tauri/` 运行，日志位于 `/tmp/polaris-r1c-*.log`。

| 实际命令 | 真实结果 |
| --- | --- |
| `cargo check -p polaris-app --offline` | 退出 0；app-check.log。 |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0；server-check.log。 |
| `cargo test -p polaris-kernel --lib --tests --offline chat::backend` | 首轮 41 通过；补齐 legacy 测试后 46 通过，退出 0；backend.log。集成测试被过滤。 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 退出 101；146 通过、10 失败、2 忽略；kernel-tests.log。10 项为下列已有例外。lib 失败后 Cargo 未执行集成测试。 |
| 下方精确排除例外的完整测试命令 | 退出 0；lib 146 通过、2 忽略、10 过滤；backend_config 集成测试 4 通过；kernel-exceptions.log。 |
| `git diff --check` | 退出 0，无输出。 |
| Python 改动文件约束检查 | 全部 13 个改动文件位于授权目录；每文件 ≤500 物理行、每行 ≤500 字符；新增 Rust 无 unwrap/expect 调用；无依赖清单/锁文件变化。 |

完整测试命令的四项既有基线失败：

```text
doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix
doctor::path::tests::prime_and_prepend_behaviour
doctor::verify::tests::install_root_groups_npm_entries_together
doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts
```

六项沙箱 TCP 失败：

```text
integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp
integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp
provider::claude_login::claude_oauth_tests::loopback_run_end_to_end_local
provider::claude_login::claude_oauth_tests::loopback_run_rejects_wrong_state
provider::store::local_route_tests::openai_provider_routes_via_local_router
provider::store::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

前四项 TCP 报 PermissionDenied / Operation not permitted；后两项无法绑定本地 8765–8789 端口。
随后实际执行（命令换行为可复制形式）：

```bash
cargo test -p polaris-kernel --lib --tests --offline -- \
  --skip doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix \
  --skip doctor::path::tests::prime_and_prepend_behaviour \
  --skip doctor::verify::tests::install_root_groups_npm_entries_together \
  --skip doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts \
  --skip integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp \
  --skip integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp \
  --skip provider::claude_login::claude_oauth_tests::loopback_run_end_to_end_local \
  --skip provider::claude_login::claude_oauth_tests::loopback_run_rejects_wrong_state \
  --skip provider::store::local_route_tests::openai_provider_routes_via_local_router \
  --skip provider::store::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

## 未运行与限制

未运行真实模型/Claude CLI、桌面 UI 端到端、Windows、panic=abort 或整个宿主被强杀的场景。
Rust Drop 保障适用于正常析构和 panic unwind，不可能在进程 abort/强杀后向同进程消费者发终态。
常驻 reader 测试使用真实工作循环及注入迭代器，一次性 stdout guard 使用线程退出/panic；未启动真实 legacy CLI。
未运行前端/Node agentd 测试（未改这些模块），未运行全仓 rustfmt。
无剩余实现阻塞；完整无排除测试仍受既有四项基线及沙箱 TCP 环境阻塞，不把例外计作通过。
