# 任务 R3：P3 审查意见修复

先读 `docs/reviews/P3-round1-triage.md`。本任务修复：C1、P3a-1、P3a-2、P3b-8、P3b-9，并按 P3a-3 / P3b-6 补文档。P3b-2 配置化、P3b-4、P3b-5 并入后续 P3b 任务，本任务不做。驳回项不要改代码。

| 条目 | 要求 |
| --- | --- |
| C1（高） | 沙箱 bash 回传真实退出码（含被信号终止、超时的区分），agentd `remoteBash.exec` 原样返回；Rust `host_tools`/`docker_tools` 与协议 `sandbox.respond` 同步（协议变更则 `protocolVersion` 升 4，同步样例集/哈希/握手）。测试：失败命令返回非零退出码、超时有明确标记。 |
| P3a-1（高） | `permissions.ts` 的"目录内"判定：不先做词法规整；路径含 `..` 段一律判"非目录内"（回落人工确认）；只对实际存在的最长前缀 realpath，不存在的尾部组件不得是链接。测试必须包含：工作区内 `link → 外部目录`、路径 `link/../x` 判为非目录内；`./a/b` 正常；绝对路径指向工作区内/外；Windows 风格分隔符。 |
| P3a-2（中） | 开启沙箱的会话在首次工具调用前做快速可用性探测（超时进配置，默认 5000ms），失败立即 `SANDBOX_UNAVAILABLE`；结果短期缓存（时长进配置）。测试：后端无响应时在探测超时内报错，而非整体工具超时。 |
| P3b-8（低） | 授权请求过期或所在轮次结束时，agentd 发出可让前端关闭弹窗的事件（复用或新增，协议同步）；前端弹窗显示剩余时间并据事件关闭。前端新增逻辑放 `src/features/permissions/`，既有超长文件不加逻辑。 |
| P3b-9（低） | `set_sandbox_executor` 失败时记录警告日志。 |
| 文档 | 在 feature-tree 与协议说明写明 allowAlways 作用域（同一会话、完全相同工具与参数）。 |

允许修改：`agentd/`、`src-tauri/crates/polaris-kernel/src/chat/backend/`、`src-tauri/crates/polaris-kernel/tests/`、`src-tauri/src/wiring.rs`、`src/features/permissions/`、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；可变值进配置。
必须通过：agentd `npm run typecheck && npm test`（沙箱内端口/子进程类失败单独列出）；两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`；`npx vue-tsc --noEmit`；`npx vitest run src`。不 commit。
