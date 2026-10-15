# R8 第二次执行报告

日期：2026-10-15。执行依据：`docs/reviews/P5cd-round1-triage.md` 及 Claude 第二次运行范围例外批复。
本轮补齐后端 #4，保留前端 #5、后端 #3 与 #5 已有实现和测试。未修改驳回项的生产逻辑。
未添加依赖，未 commit / push，未修改 polaris-collab、mesh、desktop_commands 或其他 P6a 文件。
代码与定向测试已落地；内核全量测试仍 **BLOCKED**，本报告不代表最终验收完成。

## 改动文件及行为

下表 K 表示 `src-tauri/crates/polaris-kernel/src`；“保留”指第一次运行已完成、本轮没有重写。

| 文件 | 行为 |
| --- | --- |
| `src/features/runtime/components/ContextFileEditor.vue` | 保留受控项目选择，确认前恢复原选项，取消保留编辑 |
| `src/features/runtime/useContextFileEditor.ts` | 保留脏编辑确认、保存目标绑定、读取失败清理、并发保护 |
| `tests/frontend/runtimeContextEditor.test.ts` | 保留 4 个切换、确认、取消和保存目标行为用例 |
| `K/chat/backend/pi/experts.rs` | 保留字节上限与尾部空白规范化；恢复职责交给 supervisor，显式设置等待恢复事务，未同步时报具名错误 |
| `K/chat/backend/pi/experts_tests.rs` | 保留 8 个长度校验、规范化、持久化及回滚测试 |
| `K/chat/backend/pi/experts_sync.rs` | 新增 supervisor 恢复事务：读取持久覆盖、比较 ping、config.apply、再次 ping 核验；失败单独记录并在后续 ping 重试 |
| `K/chat/backend/pi/mod.rs` | shared 模式构造时启动监督线程，不等待 daemon I/O；移除首轮聊天的同步恢复阻断 |
| `K/chat/backend/pi/supervisor.rs` | 初次/周期 ping 接入恢复；恢复期间排队普通请求，完成或失败后释放；daemon 失败、退出、空闲回收时标为未同步 |
| `K/chat/backend/pi/supervisor_tests.rs` | 原状态构造补初始化字段，原断连/退出测试行为保留 |
| `K/chat/backend/pi/experts_recovery_tests.rs` | 新增 5 个行为测试，覆盖不可达恢复、无效配置修复、apply 拒绝/核验不匹配、空闲重启、设置调用与启动恢复顺序 |
| `K/chat/backend/mod.rs` | BackendStatus 增加专家同步布尔状态和独立错误字段 |
| `src/types/agentRuntime.ts` | 对应前端状态类型与预览默认值 |
| `K/provider/usage_costs.rs` | 保留仅测试改动：同模型混合有价/无价三种顺序均 false，仅有价窗口 true |
| `docs/feature-tree.md` | 更新入口、依赖、配置、迁移、验证和剩余阻塞 |
| `docs/pi-migration/r8-review-fixes.md` | 保留第一轮报告，增加指向本轮报告的历史说明 |
| `docs/pi-migration/r8-review-fixes-run2.md` | 本报告 |

## 状态、配置与迁移

- `agent_backend_status.expertsSynced`：Pi 初始/不可达/退出/空闲回收为 false；确认持久配置一致或没有持久覆盖为 true；非 Pi 为 null。
- `expertSyncError`：同步相关错误字符串或 null；成功恢复后清空。与 daemon 的 healthy 分开，不将专家配置拒绝或校验错误解释成 daemon 不健康。
- 后端构造返回不依赖 daemon 是否可达；agentd 不可达期间模型请求本身仍会失败，不伪造模型服务可用。
- 初次健康 ping 后自动恢复。恢复事务内排队普通命令，apply 成功还必须 ping 验证；恢复失败后普通命令继续可用，专家 setter 返回 `ExpertConfigUnsynced`。
- 后续周期 ping 自动重试未同步状态，不需要再次调用专家 getter/setter；重启/空闲唤醒后重新恢复。无持久覆盖时保留 daemon 部署默认值。
- `POLARIS_EXPERT_PROMPT_MAX_BYTES` 默认 32768 UTF-8 字节，必须为正 usize；零、负数、非整数、溢出、非 UTF-8 环境值报 `ExpertPromptLimitInvalid`。
- 持久文件与 daemon prompt 均先校验原始字节数，超限报 `ExpertPromptTooLong`；随后去除尾部空白再比较/保存，保留开头空白与内部换行。
- 切换上限：`POLARIS_EXPERT_PROMPT_MAX_BYTES=65536 ./polaris-server`；`unset POLARIS_EXPERT_PROMPT_MAX_BYTES` 恢复默认。
- 继续使用现有 `agentd.pingIntervalMs`（默认 30000）、`restartBaseMs`（200）、`restartMaxMs`（30000）、`restartJitterMs`（100）、`failureThreshold`（5）；既有配置校验要求周期、退避和阈值为正，jitter 可为零。
- 示例：在 `/tmp/r8-agent.json` 写入 `{"agentd":{"isolation":"shared","pingIntervalMs":1000}}`，通过 `POLARIS_AGENT_CONFIG=/tmp/r8-agent.json ./polaris-server` 切换。shared 仅用于明确接受共享 daemon 的部署。
- 不改变既有熔断策略：达到 failureThreshold 后 supervisor 停止自动重启，须恢复后端实例；本轮恢复测试显式提高阈值，验证熔断前的重启与 ping 链路。
- 无新依赖。持久 JSON 结构和协议版本不变；旧超限 prompt 需缩短或显式提高上限。状态接口新增字段，前端类型同步更新；费用聚合生产逻辑不变。

## 实际验证命令与结果

Rust 命令目录为 `src-tauri/`，前端命令目录为仓库根。日志在本执行环境 `/tmp/`，没有伪造成功或跳过失败测试。

| 命令 | 真实结果 | 日志 |
| --- | --- | --- |
| `cargo test -p polaris-kernel --lib --offline chat::backend::pi::experts` | 初次因缺少 trait 方法作用域编译失败；改用 client 状态快照后重跑退出 0，11 测试通过；随后新增 2 测试由全量覆盖 | `/tmp/r8-2-experts.log`（修正后） |
| `cargo check --offline` | 退出 0，桌面壳，0 编译警告 | `/tmp/r8-2-desktop-check.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0，服务端壳，0 编译警告；本轮未复现之前的 P6a 编译错误 | `/tmp/r8-2-server-collab-check.log` |
| `cargo check -p polaris-cli --bin polaris-server --offline` | 额外组合退出 0，但有 4 条范围外 mesh 警告，不能记为零警告通过 | `/tmp/r8-2-server-check.log` |
| `cargo test -p polaris-kernel --lib --tests --offline` | 执行两次，均退出 101；首轮 216 通过，最后 218 通过、6 失败、2 ignored。13 个专家测试、3 个费用测试全部通过，无编译警告 | `/tmp/r8-2-kernel-tests.log`（最后一次） |
| `cargo test -p polaris-kernel --offline --test backend_config --test provider_switch --test catalog_protocol --test credential_lock_interop` | 退出 0，8 个集成测试通过，补足全量因 lib 失败未执行的部分 | `/tmp/r8-2-kernel-integration.log` |
| `npx vue-tsc --noEmit` | 退出 0，无输出 | `/tmp/r8-2-vue-tsc.log` |
| `npx vitest run` | 退出 0，15 文件、65 测试通过 | `/tmp/r8-2-vitest.log` |
| `git diff --check -- <R8 文件清单>` | 退出 0，无空白错误 | 工具输出 |
| Python 文件物理行数、每行字符数检查；新增 Rust unwrap/expect 扫描 | 退出 0，16 个文件均 ≤500 行且每行 ≤500 字符；新增 Rust 无 `.unwrap()` / `.expect()`；同次 diff 空白检查通过 | 工具输出 |

## 未完成与阻塞

内核全量命令未通过，以下 6 个测试仍失败：

- `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`：bind PermissionDenied / Operation not permitted。
- `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`：同上。
- `provider::claude_login::tests::loopback_run_end_to_end_local`：PermissionDenied / Operation not permitted。
- `provider::claude_login::tests::loopback_run_rejects_wrong_state`：同上。
- `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`：无法绑定 8765–8789 本地端口。
- `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`：同上。

运行环境不允许申请提权，未更改网络测试或范围外实现来规避失败。需要 Claude 在允许本地 TCP bind 的环境重跑。
额外无 collab-net 服务壳的 4 条警告来自 `src/mesh.rs`：474、1159 不可达语句；330 的 c、1121 的 access 未使用。本任务未修改这些并行 P6a 文件。
浏览器手工/E2E、真实 agentd 端到端以及 agentd 全量测试未运行；新增恢复测试使用现有 Node 协议夹具和内嵌可控夹具，不访问外部模型。
最终模块状态由 Claude 重新运行验证、Pi 独立审查后决定。
