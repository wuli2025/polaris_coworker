# 任务 R5：P4 审查意见修复

先读 `docs/reviews/P4-round1-triage.md`。修复：P4a-1（高，按文末"锁设计要求"）、P4a-2（补测试）、P4a-3、P4a-5、P4a-6、P4b-4（中-高）。驳回项不要改代码。

- P4a-1：`agentd/src/models/file-credentials.ts` 与 `src-tauri/crates/polaris-kernel/src/provider/credential_file.rs` 实现可回收的锁：持有者信息（主机名、PID、进程启动时间、获取时间）；同主机持有者已死或超过最大持有时长（配置）可回收；回收用"重命名为唯一墓碑再删除"保证原子；两侧语义一致并共用 JSON 测试向量。测试：强杀持锁子进程后可回收；活着的慢持有者不被抢；并发回收仅一方成功。
- P4a-2：用 `FileCredentials` 的测试，断言 OAuth 登录与刷新后磁盘文件内容更新（0600 权限、原子写）。
- P4a-3：用量持久化失败写 daemon 日志并计数，计数出现在 `ping` 结果与 Rust `agent_backend_status`。
- P4a-5：`apply` 后重算内置目录；测试。
- P4a-6：`models.catalog` 响应 schema + Rust 类型 + 协议样例/哈希同步（如协议变更则升版本并同步握手）。
- P4b-4：`provider_switch` 及其他会触发 agentd IPC 的提供商命令改为异步命令（`#[tauri::command(async)]`，server 壳调用路径同步检查）；`PROVIDER_MUTATION` 只保护选择状态，IPC 移出锁外，用代次号丢弃过期下发；测试：agentd 慢响应时切换命令不阻塞调用线程之外的其他提供商查询，且最终生效的是最后一次选择。

允许修改：`agentd/`、`src-tauri/crates/polaris-kernel/src/provider/`、`src-tauri/crates/polaris-kernel/src/chat/backend/`、`src-tauri/crates/polaris-kernel/tests/`、`src-tauri/src/` 中提供商命令注册处、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect。
必须通过：agentd `npm run typecheck && npm test`（沙箱内子进程/端口类失败单独列出）；两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`；`npx vue-tsc --noEmit`；`npx vitest run`。不 commit。
