# R5 执行报告

2026-10-14。执行范围：P4a-1/2/3/5/6、P4b-4。依据 `docs/reviews/P4-round1-triage.md`。

实现与新增定向测试已交付；**完整验收 BLOCKED**：指定全量 npm/kernel 测试仍有环境或基线失败，不能宣称模块完成。
未改驳回项对应业务逻辑，未新增依赖，未 commit/push。保留会话开始已有的 `docs/execution/R5-1/`、`tests/`。
完整文件清单见 [files.md](files.md)，维护入口见 [feature-tree](../../feature-tree.md)。

## 实现行为

- P4a-1：两端目录锁存 hostname、PID、进程启动标识、获取时间；同主机进程不存在/启动身份不同，或年龄严格大于最大持有时长时回收。
  Linux 身份是 boot ID + 启动 tick；macOS 用 C locale `ps lstart`；Windows 用 PowerShell UTC ticks。无法确认进程状态时保守等待年龄上限。
- 候选目录在持有者信息完整后原子发布。布局为 `providers.json.lock/owner-<随机代次>/owner.json`。
  回收将**已观察到的唯一代次子目录**重命名到唯一墓碑，再删除墓碑；外层容器只用非递归 rmdir。
  这比直接重命名固定 `.lock` 路径多一层代次标识，避免延迟回收者把新持有者误当旧锁移走；正常释放也按同一代次操作。
  元数据损坏时等待代次目录年龄过期。写前检查租约所有权，超时回收后的旧释放者不能删除替代锁。
- P4a-2：仅补 FileCredentials 登录/刷新磁盘测试；断言 access/refresh 真实更新、0600、原打开 inode 保留旧 JSON、无临时文件；不重写 SDK OAuth。
- P4a-3：用量持久化失败保持原轮次错误事件，并写 daemon warn 日志、累计 usagePersistFailures。
  ping 带计数，Rust 在握手与周期 ping 后同步到 agent_backend_status；重启 daemon 后计数重新开始。日志遵循既有 logLevel。
- P4a-5：apply 成功时同步替换 runtime 与 builtinCatalog；内置提供商的配置覆盖进入新目录，自定义提供商仍不混入。
- P4a-6：协议升至 v6；增加 CatalogSchema/CatalogModelSchema 与 Rust Catalog/CatalogModel/ModelCost 类型，Rust 实际消费时解析验证。
  保留 SDK 中空 baseUrl、未知/动态价格负值及可选 tiers；价格字段验证数值类型，模型上下文/输出上限验证正整数。
  同步 ping 类型、版本常量、握手、源 SHA256、共享 JSONL 样例与所有假 daemon。
- P4b-4：提供商 store 命令及 Claude/Codex OAuth IPC 命令改为 Tauri `command(async)`。
  PROVIDER_MUTATION 只保护内存选择/配置状态与代次；文件持久化与 agentd IPC 移出此锁。
  独立下发锁维持 IPC 次序，发送前、catalog 响应后、apply 响应后检查代次；旧结果不更新缓存或回滚选择。
  下一轮准备遇到过期下发会重新读取当前选择。旧 Claude CLI 的进程环境/设置切换仍用独立锁串行。
- server 壳已检查：`src-tauri/src/apihub/invoke.rs` 的 spawn_blocking → dispatch_sync → chat_services → provider 命令。
  原路径已经离开 async worker，无需修改注册代码；桌面注册宏继续调用原公开函数名。

## 配置与迁移

- Rust 默认值：`provider/pi-default.json` 中 lockTimeoutMs=30000、lockPollMs=25、lockMaxHoldMs=600000。
- 三项均须正安全整数（1..9007199254740991）。Rust 校验 JSON 类型，Node 校验最终数值。
- Rust 切换示例：将 `{"lockMaxHoldMs":900000,"lockTimeoutMs":45000}` 保存为 JSON，设置 `POLARIS_PROVIDER_CONFIG=/path/provider.json`。
- 独立 agentd 示例：`POLARIS_CREDENTIAL_LOCK_MAX_HOLD_MS=900000 POLARIS_CREDENTIAL_LOCK_TIMEOUT_MS=45000 node dist/main.js`。
  Rust 启动 agentd 时自动传递三项锁参数。FileCredentials 显式 options 也支持 maxHoldMs，省略为 600000。
- 最大持有时长是租约上限，活持有者超过上限也可回收；上限内的正常慢刷新不会被抢。配置应大于正常刷新最大耗时。
- 锁格式与协议需两端一起升级，停掉旧进程再启动新版；不支持新旧锁实现混跑。空旧锁可自动回收，无须迁移 credentials JSON。
- provider_switch 先记录并持久化选择，再下发；当前代次失败返回错误，保留选择供重试/下一轮准备下发，不会由旧响应回滚新选择。
- v5 daemon 与 v6 Rust 会握手失败并熔断，须同时部署。

## 实际检查结果

命令在表内 cwd 执行；日志位于同目录 `logs/`。检查期间出现过 schema 与测试类型错误，已修正后重跑；下表列交付状态。

| cwd | 实际命令 | 结果 / 日志 |
| --- | --- | --- |
| agentd | `npm run typecheck && npm test` | typecheck exit 0；npm test exit 1，23 文件通过、2 失败；[typecheck](logs/typecheck.log)、[npm](logs/agentd-tests.log) |
| src-tauri | `cargo check --offline` | exit 0，保留既有 warning；[desktop](logs/desktop-check.log) |
| src-tauri | `cargo check --offline --no-default-features --features server` | exit 0，保留既有 warning；[server](logs/server-check.log) |
| src-tauri | `cargo test -p polaris-kernel --lib --tests --offline` | exit 101；最后一次普通运行 177 通过、11 失败、2 ignored；含一次额外时序超时，见下文；[原命令](logs/kernel-tests.log) |
| src-tauri | `cargo test -p polaris-kernel --lib --tests --offline --no-fail-fast` | exit 101；lib 178 通过、10 失败、2 ignored；4 个 integration target 共 8 项通过；[完整继续运行](logs/kernel-all.log) |
| 根目录 | `npx vue-tsc --noEmit` | exit 0，无输出；[vue-tsc](logs/vue-tsc.log) |
| 根目录 | `npx vitest run` | exit 0，8 文件 / 25 测试通过；[vitest](logs/vitest.log) |

补充检查：

- agentd：`node --import tsx --test test/credential-lock.test.ts test/provider-credentials.test.ts test/provider-login.test.ts test/provider-usage.test.ts test/catalog.test.ts`：exit 0，5 文件通过；[日志](logs/focused-agentd.log)。
- Rust：`cargo test -p polaris-kernel --test provider_switch --test catalog_protocol --offline`：exit 0，2 项通过；[日志](logs/new-tests.log)。
- Rust：`cargo test -p polaris-kernel --test credential_lock_interop --offline`：exit 0，2 项通过（含 child probe）；[日志](logs/lock-interop.log)。
- Rust：原完整测试命令追加 `-- --skip <名称>`，逐一跳过下文 10 个固定失败项：exit 0；lib 178 通过、2 ignored，integration 当时已有 6 项通过；[日志](logs/kernel-filtered.log)。反向锁测试添加后已由 no-fail-fast 全量覆盖。
- `git diff --check`：exit 0。
- 自有改动文件行数/行宽扫描：所有文件 ≤500 物理行、所有行 ≤500 字符；新增 Rust 代码未使用 `.unwrap()` / `.expect()`。

## 未通过与阻塞原因

agentd 分开记录：

1. `web.test.ts`：真实本地 HTTP 测试 `listen EPERM 127.0.0.1`，沙箱禁止端口绑定；无端口替身测试通过。[单文件日志](logs/web.log)
2. `e2e.test.ts`：真实 main.ts 子进程首个 ping 超时，子进程 exit=0、stderr 为空。不能归因于具体权限错误。
   使用 `git archive HEAD agentd` 解出未修改源到 `/tmp/r5-baseline`，链接相同 node_modules，执行 `node --import tsx test/e2e.test.ts`，相同失败复现。
   [当前日志](logs/e2e.log)、[HEAD 基线日志](logs/e2e-baseline.log)。未修改生产入口规避此失败。

Rust 10 个固定失败项：

- `doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix`
- `doctor::path::tests::prime_and_prepend_behaviour`
- `doctor::verify::tests::install_root_groups_npm_entries_together`
- `doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts`

以上是 Linux 上 Windows 路径/多安装断言失败，文件未在本次范围内修改；未推断它们已经通过。

- `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`
- `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`
- `provider::claude_login::tests::loopback_run_end_to_end_local`
- `provider::claude_login::tests::loopback_run_rejects_wrong_state`
- `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`
- `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`

以上依赖端口监听：直接 EPERM，或路由端口范围全部无法绑定。

另一次普通全量运行中，`chat::backend::pi::tests::config_apply_is_not_busy_after_circuit_or_thread_exit` 报 AgentdRequestTimeout。
后续过滤固定 10 项及 no-fail-fast 两次均通过此项；保留原失败日志，不把偶发失败隐藏成首次通过。

## 未运行与待核实

未运行真实 OAuth 登录、付费模型调用、Windows/macOS 实机、NAS 共享目录、断电测试、桌面 GUI 交互验收。
已运行 Linux SIGKILL、两个 Node 进程并发回收、Node→Rust / Rust→Node 崩溃回收、活慢持有者、旧释放者、损坏元数据过期恢复。
慢 IPC 测试通过实际公开 provider_switch、真实 JSONL 子进程门控 catalog/apply 两阶段，检查并发 provider_list 与最后生效模型。
当前剩余阻塞是指定全量测试无法全绿；本报告仅是执行证据，Claude 需重跑、Pi 独立审查后再决定模块是否完成。
