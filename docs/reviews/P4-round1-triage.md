# P4 第 1 轮 Pi 审查处置（Claude）

原始报告：`docs/reviews/P4a-round1/`、`docs/reviews/P4b-round1/`。

## P4a（agentd 凭据/模型 + provider 新逻辑）

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 严重 | 锁崩溃残留永久阻塞 | **采纳（高）** | 主控已在 brief 中提出，Pi 独立确认。`file-credentials.ts` 的 `locked()` 与 `credential_file.rs` 的 `FileLock` 都只在正常退出时删除 `.lock`；持锁进程被杀/崩溃/断电后，所有凭据读写永久 `CredentialStoreBusy`。 |
| 2 严重 | OAuth 刷新后的 token 未持久化 | **驳回，补测试** | 生产 `main.ts:33` 注入 `FileCredentials`；Pi 的登录经注入的 `CredentialStore` 写入（`test/provider-login.test.ts:21` 证明登录凭据落在注入存储），刷新经 `pi-ai/src/auth/resolve.ts:132` 的 `credentials.modify` 写回。缺口是测试只用内存存储：补一个用 `FileCredentials` 的测试，断言登录与刷新后**文件内容**更新。 |
| 3 中 | 用量持久化失败只发给当前轮次 | **采纳（低）** | daemon 级日志告警 + 失败计数进入 `ping`/状态，便于诊断。 |
| 4 中 | Pi 后端故障切换不触发 | **有证据驳回** | `chat/backend/pi/translate.rs:93` 在轮次结束时调用 `crate::provider::pi_outcome`，Pi 未获该文件。 |
| 5 低 | `builtinCatalog` 在 apply 后过期 | **采纳（低）** | `apply` 更新运行时时同步重算目录；测试改配置后 `models.catalog` 反映新值。 |
| 6 低 | `models.catalog` 响应无 schema | **采纳（低）** | 补响应 schema 与 Rust 类型、协议样例。 |

## P4b（其余 Rust/前端接线）

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 中 | 同名模型选择返回非首选提供商 | **驳回** | `input.rs`：`preferred.unwrap_or(model)` 在 `preferred` 为 Some 时返回首选；只有无首选时才取首个匹配，且多于一个时已被 `ensure!` 拒绝为歧义。Pi 对 `unwrap_or` 语义理解有误。 |
| 2 高 | 删除 reopenRequired 处理后空闲会话仍用旧提供商 | **有证据驳回** | 重开在 agentd 内完成：`agentd/src/sessions/registry.ts:363-369` 的 `reopenRequired()` 对空闲会话调用 `entry.rebuild()`（复用 SessionManager、保留历史），Rust 无需再关闭。 |
| 3 中 | `pi_outcome` 线程无 join | **驳回** | 故障统计为"发后不管"的旁路，不应阻塞轮次收尾；实现见 `provider` 模块（P4a 包）。 |
| 4 中 | 持 `PROVIDER_MUTATION` 锁做 IPC | **采纳并提为中-高（改述）** | 更严重的是：`provider/store/commands.rs:3` 的 `provider_switch` 是**同步** Tauri 命令（在主线程执行），切换时对 agentd 做 2 次 IPC（catalog + 配置下发，最长受请求超时 15 秒约束），agentd 重启或繁忙时**整个界面冻结**。修：改为 `#[tauri::command(async)]`（仓库已有先例，见 `chat/attach.rs` 注释）；锁只保护选择状态，用代次号丢弃过期的下发结果，IPC 不在锁内。 |
| 5 低 | 以 PID 作缓存键，进程重启后冗余下发 | **驳回** | 新 agentd 进程没有配置，PID 变化触发重新下发恰恰是正确行为；改成"启动时间戳"效果相同。 |

## 并入修复的锁设计要求（P4a-1）
- 锁目录内写入持有者信息文件：主机名、PID、进程启动时间（防 PID 复用）、获取时间。
- 回收条件（满足其一）：同主机且 PID 不存在或启动时间不符；或锁年龄超过配置的最大持有时长（默认远大于正常刷新耗时，用于跨主机/NAS 共享路径）。
- 回收必须原子：先把残留锁重命名为唯一墓碑名再删除，避免两个进程同时回收后都以为自己拿到锁。
- Node 与 Rust 两侧语义完全一致，共用一份测试向量（例如 JSON 夹具描述"持有者信息 → 是否可回收"）。
- 测试：子进程拿锁后被强杀（不清理），另一进程在超时前成功回收并完成读写；活着的慢持有者不被抢锁；两进程同时回收只有一个成功。

## 第 2 轮（复审 `P4-round2/`）—— 结论 INCOMPLETE（如实记录）
Pi 判 **INCOMPLETE**：主控为控制包体积排除了测试文件，Pi 无法确认锁回收、OAuth 落盘、目录刷新、代次号的测试覆盖。此为送审材料不足导致，**不是 PASS**；本模块已达 2 轮上限，不再开第 3 轮。

逐条处置（主控轻量自检，仅作收尾核实，不代替独立审查）：
- #1 中"owner.json 丢失时锁永久残留"：**驳回**。`agentd/src/models/credential-lock.ts:30` 在持有者信息无效时按目录年龄回收，超过 `maxHoldMs`（默认 600000，`:8`）即重命名为墓碑；最坏 10 分钟自愈，不会永久残留。
- #2 低"切换进行中提前返回 superseded"：**不处理**。Pi 自认只多一次 IPC，正确性不受影响。
- #3 低"assertOwned 与写入之间的竞态窗口"：**记为已知限制**。仅当活着的持有者持锁超过 10 分钟才可能被回收；正常刷新为秒级本地 I/O。
- #4 信息"协议指纹是否同步"：**已核实**。`tests/fixtures/protocol-schema.sha256` 与当前 `agentd/src/protocol/schema.ts`、`events.ts` 的 SHA-256 完全一致，且指纹测试随内核测试通过。
- 测试证据（主控实跑）：`agentd/test/credential-lock.test.ts:28` 以 SIGKILL 杀死持锁子进程后另一进程回收；`src-tauri/crates/polaris-kernel/tests/credential_lock_interop.rs` 验证 Node↔Rust 同一把锁互通；全部通过。

结论：P4 **标记完成，独立审查状态 = INCOMPLETE（非 PASS）**，无已知阻塞。
流程改进：后续送审包必须包含测试文件；体积超限时拆包，而不是剔除测试。
