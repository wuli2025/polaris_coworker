# R1b / P2-5：Rust 轮次空闲超时执行报告

2026-10-14。范围例外已批准，仅修改授权的七个 Rust 文件及 docs；无新依赖、无提交或推送。
本报告是执行证据，由 Claude 重跑测试、Pi 独立审查，最终由 Claude 决定模块是否完成。

## 文件与行为

Rust 路径前缀为 `src-tauri/crates/polaris-kernel/`。

| 文件 | 行为 |
| --- | --- |
| `src/chat/backend/config.rs` | 删除固定轮次配置；新增 turnIdleTimeoutMs / turnMaxMs 的默认值、类型与正数校验；旧字段在反序列化时具名拒绝，包括 null、错误类型和与新字段并存的情况；序列化不输出旧字段。 |
| `src/chat/backend/pi/mod.rs` | 通过既有 Control::Register.timeout 传递硬上限，0 表示不限；请求显式 timeout 与已启用配置上限取较小者。显式零请求预算仍立即到期。 |
| `src/chat/backend/pi/routes.rs` | 独立记录注册时间与最后有效轮次事件时间；所有有效事件（含 reasoning/usage）刷新空闲计时；终态不再判超时。 |
| `src/chat/backend/pi/supervisor.rs` | 注册时初始化两种计时；空闲报 AgentdTurnIdleTimeout，硬上限报 AgentdTurnMaxTimeout；沿用 Route.fail 的 error+done、首错保留与 daemon 回收流程。 |
| `src/chat/backend/bridge.rs` | 使用 requestTimeoutMs 作为接收轮询间隔，轮询到期继续等待 supervisor 的终态；隐藏事件不会因缺少 UI 输出而导致取消。 |
| `src/chat/backend/pi/tests.rs` | 定时 Node 协议夹具内嵌于授权文件，无新增依赖；覆盖持续隐藏事件、完全无事件但 ping 正常、硬上限、请求预算及配置上限约束。 |
| `tests/backend_config.rs` | 覆盖默认值、新配置序列化、无效类型/范围、零空闲值和旧字段迁移错误。 |
| `docs/feature-tree.md` | 更新入口、依赖、配置、迁移与验证链接，解除旧范围 BLOCKED 记录。 |
| `docs/pi-migration/p2-result.md` | 将操作示例更新为新字段，解释默认值差异并链接本报告。 |
| `docs/pi-migration/r1-review-fixes.md` | 保留 R1 历史证据，顶部标明 R1b 已获范围例外并补充后续报告链接。 |
| `docs/pi-migration/r1b-idle-timeout.md` | 本执行报告。 |

## 配置、依赖与迁移

宿主配置来源仍为 `POLARIS_AGENT_CONFIG` 指向的 JSON；不增加环境变量或 crate。
公开接口仍使用 Config / Agentd、AgentBackend、TurnRequest 与 EventStream；内部沿用 Control、Translator、Mailbox。
这里修改的是宿主 `agentd` 配置段，不是 Node agentd 的配置文件。

| 字段 | 默认值 | 类型与语义 |
| --- | --- | --- |
| `agentd.turnIdleTimeoutMs` | 240000 | u64 正整数；自当前轮次上一个有效 agentd 事件起计时，首个事件前从注册开始；ping/命令响应不刷新。 |
| `agentd.turnMaxMs` | 0 | u64 非负整数；0 不限，正值为注册起的硬上限，事件不能延长。 |

Rust 默认 240000ms 必须晚于 agentd 默认 180000ms，使 agentd 能先执行自身空闲超时清理并发送终态；Rust 只作失联兜底。
自定义部署若调整 agentd 空闲阈值，应同时将 Rust 阈值设得更大并留出传输/清理余量。
两份配置分别加载，Rust 不强制跨进程数值比较；本地只校验类型和正数，允许测试及部署显式使用更短阈值。
两种条件同时满足时优先报告硬上限；超时检查精度受既有 supervisorTickMs（默认 10ms）与调度影响。
沿用 shared daemon 超时后的进程回收策略，因此同进程其他在途轮次也会收尾；本任务不改变隔离模式。

删除旧 `turnTimeoutMs` 后使用默认策略：

```json
{"agentd":{"turnIdleTimeoutMs":240000,"turnMaxMs":0}}
```

若需要保留原来十分钟硬上限，显式切换为：

```json
{"agentd":{"turnIdleTimeoutMs":240000,"turnMaxMs":600000}}
```

旧字段不会兼容或静默忽略，错误内容为：

```text
InvalidAgentConfig: agentd.turnTimeoutMs was removed; use turnIdleTimeoutMs and turnMaxMs
```

## 实际验证

Cargo 命令均在 `src-tauri/` 执行；原始日志在本次工作环境 `/tmp/polaris-r1b-*.log`。

| 实际命令 | 真实结果 |
| --- | --- |
| `cargo check -p polaris-app --offline` | 退出 0；6 个既有 doctor 警告；app-check.log。 |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0；6 个既有 doctor 警告；server-check.log。 |
| `cargo test -p polaris-kernel --lib --tests --offline chat::backend` | 首轮退出 0，38 通过；集成测试因过滤器为 0 项；backend.log。随后增加的 Route 边界单测由下行完整命令覆盖。 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 退出 101；139 通过、10 失败、2 忽略；kernel-tests.log。10 项均为下列已知例外，无新增失败。 |
| `cargo test -p polaris-kernel --test backend_config --offline` | 退出 0；4 通过；config-tests.log。完整命令在 lib 失败后停止，故单独运行集成测试。 |
| `git diff --check` | 退出 0，无输出。 |
| Python 文件/增量检查 | 全部改动在授权范围；每文件 ≤500 物理行、每行 ≤500 字符；新增 Rust 行无 unwrap/expect 调用；无 Cargo 清单或锁文件变化。 |

行为证据：空闲阈值 300ms，定时夹具每 25ms 交替发送 reasoning/usage，1500ms 后完成；前端仅收到 meta、done。
静默夹具完全不发送轮次事件，但每 40ms 可回复 ping，仍收到 meta、AgentdTurnIdleTimeout、done。
持续事件夹具设置 650ms 硬上限时收到 meta、AgentdTurnMaxTimeout、done；显式请求预算及配置上限均有覆盖。
Route 单测另验证最后事件后的阈值边界、总时长不替代空闲计时、硬上限不刷新及完成后不再超时。

四项既有基线失败：

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

前四项 TCP 错误为 PermissionDenied / Operation not permitted，后两项为无法绑定 8765–8789 本地端口；与既有记录一致。
未修改或跳过这些测试；完整命令并非退出 0，不把例外计作通过。

## 未完成与限制

本任务无剩余实现阻塞。未运行真实模型、真实三/四分钟时长、桌面 UI 端到端或 Windows 测试；超时行为使用缩短阈值的真实子进程和 Route 单测验证。
未运行前端或 agentd 测试（本任务未修改这些代码）；未运行 rustfmt。
保留开始时已有的未跟踪 `docs/execution/R1b-1/` 与 `docs/tasks/P2.5-protocol-extension.md`，未改写用户文件。
