# R2：R1c/P2.5/P2.5b 审查修复执行报告

日期：2026-10-14。依据：`docs/reviews/P25-round1-triage.md`。
仅处理采纳的 #3/#4，并按 #2 处置记录附件限制；驳回项未改代码。
这是 Codex 执行报告，不是最终验收结论；Claude 重跑测试，Pi 独立审查，Claude 决定完成状态。
**完整验证 BLOCKED**：规定的 `npm test` 仍有既有 e2e 失败，不能声明所有必过项通过。

## 改动文件与行为

Rust 路径前缀为 `src-tauri/crates/polaris-kernel/src/chat/backend/pi/`。

| 文件 | 行为 |
| --- | --- |
| `supervisor.rs` | State Drop 先发布退出状态并清理路由，再遍历 deferred/pending，向每个回复通道显式发送 AgentdSupervisorExited；无回复通道的健康 ping 直接释放 |
| `client.rs` | 回复通道断开统一报 AgentdSupervisorExited，实际 recv_timeout 超时报 AgentdRequestTimeout；不再被旧 last_error 覆盖；actor 显式返回的具体错误仍原样传递；附带区分测试 |
| `supervisor_tests.rs`（新增） | 正常析构和 panic unwind 均覆盖各 3 个 deferred、pending 请求；保留 sender 克隆并使用 try_recv，证明收到显式错误且无需等待超时 |
| `tests.rs` | 调整旧退出断言：发送前 control 已关闭仍为 AgentdSupervisorStopped，发送后 reply 关闭或显式退出回复为 AgentdSupervisorExited |
| `agentd/src/sessions/options.ts` | 先校验字段类型、思考档枚举、模型存在、工具档存在及可用性、模型支持文本及既有消息图片；随后应用模型/思考/工具；异常按原模型→思考→工具恢复 |
| `agentd/test/configure-transaction.test.ts`（新增） | 使用真实 SDK 和 faux provider，注入模型/思考/工具修改后抛错及工具静默漏启用；确认模型、思考、工具、toolProfile 全部恢复，再验证同一配置可成功提交；无效输入预检断言 SDK setter 零调用 |
| `docs/feature-tree.md` | 附件节点补充 #2 已知限制；新增 R2 入口、依赖、配置、迁移与验证节点 |
| 本报告（新增） | 记录命令结果、范围与未完成验证 |

依赖仅使用已有 SDK、TypeBox、Rust mpsc；无新增依赖或锁文件修改。
`registry.ts` 既有 `configuring` 锁覆盖整个回滚阶段；toolProfile 仍只在 configureSession 成功后更新，不需要修改 registry。
用户已有的 `docs/execution/R2-1/` 未修改；未执行 commit/push。

## 配置、迁移、边界

无新配置。Rust 沿用 `POLARIS_AGENT_CONFIG` 内的 `agentd.requestTimeoutMs=15000`、
`supervisorTickMs=10`、`channelCapacity=64`，配置加载器校验为正整数。
切换示例：`{"agentd":{"requestTimeoutMs":20000,"supervisorTickMs":20,"channelCapacity":128}}`。
client 的兜底等待预算仍为 `2 * requestTimeoutMs + 2 * supervisorTickMs`。

agentd 沿用 `agentd/config/agent.default.json` / `--config`：
默认模型 `anthropic/claude-sonnet-4-5`、thinkingLevel=`medium`、defaultToolProfile=`default`。
configure 中省略字段不主动改该字段；指定模型时保留 SDK 原有思考档默认选择/能力钳制行为。
合法思考枚举为 off/minimal/low/medium/high/xhigh；model 的 provider/id 和 toolProfile 为非空字符串。
工具列表由 config.toolProfiles 提供，必须在会话已注册的工具集合内；无模型/协议/配置迁移。
例如已打开且空闲的会话可发送：

```json
{"id":1,"method":"session.configure","params":{"sessionId":"已有会话ID","thinkingLevel":"high","toolProfile":"workspace"}}
```

模型切换可额外提供 `"model":{"provider":"已配置供应商","id":"已有模型"}`。
预检模型输入能力基于目标模型元数据和当前消息中的图片内容块；尚未提交的 prompt 附件仍由 prompt 校验。
SDK setModel 会联动思考档，故失败恢复必须先还原模型，再还原思考档；工具 setter 部分生效后抛错也回滚。
所有恢复步骤都尝试执行；若 SDK 自身拒绝恢复，返回具名 `SESSION_CONFIGURE_ROLLBACK_FAILED`，要求重开会话，不伪报恢复成功。
本轮保障会话配置最终值；SDK 已发出的扩展事件、会话记录中的变更及恢复记录无法撤回，不属于外部副作用事务。

附件限制仅记录：规范化后、打开前，上级目录可能被本地可写者换成符号链接。
远程协作者仅能提交路径，不能写本机 uploads，不在当前威胁模型内；fd metadata/fstat 正确。
未来 uploads 若向他人开放写权限，应采用 Linux `openat2(RESOLVE_BENEATH)`。

## 实际运行的验证

Node 命令在 `agentd/`，Cargo 命令在 `src-tauri/`，约束检查在仓库根运行。
原始日志前缀 `/tmp/polaris-r2-`，文件名见表。未把文件级 Node runner 汇总误写为全部内部断言计数。

| 命令 | 真实结果与日志 |
| --- | --- |
| `npm run typecheck && node --import tsx --test test/configure-transaction.test.ts` | 退出 0；TypeScript 通过，runner 报告 1 个测试文件通过；随后直接执行该文件确认具体断言 |
| `node --import tsx test/configure-transaction.test.ts` | 退出 0；5 个行为测试全部通过；`configure-direct.log` |
| `npm run typecheck && npm test` | typecheck 通过；npm test 退出 1；14 文件中 13 通过、1 个既有 e2e 失败；`agentd.log`（npm test 输出） |
| `node --import tsx test/protocol-extension.test.ts` | 退出 0；6 个既有协议/成功配置/图片测试全部通过；`protocol-direct.log` |
| `node --import tsx test/e2e.test.ts` | 退出 1；首个 ping 等待超时，子进程 exit=0、stderr 为空；`e2e-direct.log` |
| `cargo check -p polaris-app --offline` | 退出 0；`app.log`；存在既有编译警告 |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0；`server.log`；存在既有编译警告 |
| `cargo test -p polaris-kernel --lib --tests --offline chat::backend::pi` | 首轮退出 101：30 通过、旧退出错误名断言 1 失败；已修断言，随后全量/例外过滤全量均覆盖通过；`pi.log` 保留首轮原始结果 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 退出 101：160 通过、10 项许可例外失败、2 忽略；lib 失败后集成测试未执行；`kernel.log` |
| 下方精确排除例外的完整命令 | 退出 0：lib 160 通过、2 忽略、10 过滤，backend_config 集成测试 4 通过；`kernel-filtered.log` |
| `git diff --check` | 退出 0 |
| Python 文件范围/行数/行长及新增 Rust 检查 | 修改与新增文件均在授权范围，≤500 物理行/≤500 字符；新增 Rust 无 unwrap/expect 调用；无依赖清单变化 |

另用 `node --import tsx --input-type=module -e 'console.log("before import"); await import("@earendil-works/pi-coding-agent"); console.log("after import")'`
检查 SDK 可导入：退出 0，两个标记均输出；不能据此证明 e2e 子进程正常。

四项基线及六项沙箱 TCP 例外严格沿用此前清单；本轮未新增豁免：

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

前四项为既有 Linux 上 Windows 路径断言失败；中间四项 TCP 报 PermissionDenied/Operation not permitted，
最后两项报告无法在 8765–8789 绑定本地端口。

## 未完成与未运行

实现无待补输入；完整必过验证仍 BLOCKED 于 agentd 既有 e2e。
失败现象与 P2.5/R1 历史报告一致，但本轮未确认根因；需主控在正常子进程环境重跑，不改范围外启动逻辑或跳过该测试。
未经例外过滤的 Rust 全量仍受上列 10 项已授权例外阻塞，不能写成无排除全通过。
未运行真实在线模型、桌面 UI、Windows、进程强杀/panic=abort、前端测试或全仓格式化。
未注入 SDK 回滚操作自身持续失败场景；该分支保留具名错误，不承诺 SDK 拒绝恢复时仍能恢复原值。
