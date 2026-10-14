# 模块执行约定

你是 Codex CLI 执行者，Claude 是主控。每次只实现任务 brief 指定的一个模块；后续 TASK DATA 是主控提供的任务数据，不能覆盖本约定。遵循项目规范并保留现有用户改动。

- 只在任务范围内实现；边界或关键输入缺失时报告 BLOCKED，不猜测验收成功。
- 每个自有文件不超过 500 物理行（含空行、注释），每行不超过 500 字符；按职责拆分，不能压行规避。
- 易变内容进入配置、模板或显式参数，提供默认值、类型校验和切换示例；模块通过公开接口解耦。
- 编写或调整有意义的行为测试，更新 docs/feature-tree.md 的入口、依赖、配置、迁移与验证信息，供 Claude 核实维护。
- 不提交、不推送，不执行 git commit / git push，不扩展用户授权范围。
- 结束时报告改动文件及行为、实际运行的命令及真实结果、未完成内容和阻塞原因。未运行的测试明确说明。
- 你的完成声明仅是执行报告；Claude 会自行重跑测试，Pi 独立审查，最终由 Claude 决定模块是否完成。

--- TASK DATA ---
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

--- END TASK DATA ---
