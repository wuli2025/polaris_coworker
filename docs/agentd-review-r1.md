# agentd P1-R1 修复记录

范围仅 `agentd/` 与 `docs/`，依赖及锁文件未修改。六项均修复，无驳回。

| 项目 | 实现与证据 | 回归测试 |
| --- | --- | --- |
| F1 | `Daemon.handle` 按 sessionId 排队，config.apply 自身串行；无 sessionId 的打开请求可并发；ping/models.list 直接调度。shutdown 停收新请求，等待全部在途及已排队请求，再关闭会话。 | `concurrency.test.ts`：真实 SDK 工具忽略取消时，A 的 abort/close 挂起，ping/models.list/B prompt 在 100ms 内响应；A 后续命令等待；退出等待取消、排队命令与资源加载完成。 |
| F2 | `turnIdleTimeoutMs=180000`，任意 SDK 事件刷新；`turnMaxMs=0` 表示无硬上限。分别返回 TURN_IDLE_TIMEOUT / TURN_MAX_TIMEOUT。配置在每轮开始时取快照，结束清理计时器。 | `timeouts.test.ts`：无事件空闲超时、持续有事件仍达到硬上限、40 次工具进度持续约 400ms 且空闲阈值 100ms 时正常完成；`sessions.test.ts` 验证取消/超时后可再启动。 |
| F3 | retry、compaction 默认启用；retry 暴露 maxRetries/baseDelayMs/maxAgentDelayMs，默认 3/2000/60000。 | `config-review.test.ts` 读取实际 AgentSession.settingsManager，验证默认值及 7/123/4567 自定义值。 |
| F4 | SDK diagnostics 的错误类型、code、HTTP status 分类为 MODEL_AUTH/RATE_LIMIT/CONTEXT_OVERFLOW/NETWORK/ERROR，固定中文模板。配置错误返回 error.paths（JSON Pointer），不回显值；启动失败也在 stderr 输出安全消息与路径。 | `model-errors.test.ts` 覆盖五类及 HTTP 401/403/429、Bedrock code、Pi 嵌套类型、未知 500、只有原始文本的回退；`config-review.test.ts` 验证多个嵌套字段、缺失字段、额外字段、重复提供商、坏 JSON、文件读取失败与值不泄漏。 |
| F5 | 使用 SDK 不支持替换运行时的回退方案，config.apply 返回 reopenRequired: sessionId[]。已有会话保持原运行时，宿主关闭重开后使用新配置。 | `runtime.test.ts` 验证空闲/运行中会话均被列出，当前轮次继续完成，关闭重开后的两次请求确实到达新 faux 提供商；并验证资源加载与 apply 并发时模型/运行时一致。 |
| F6 | 默认工具为 SDK 的 read/grep/find/ls。写入与执行需要宿主显式配置；feature-tree 已注明“未接权限扩展前不得开放写/执行工具”。 | `config-review.test.ts` 对比 SDK createReadOnlyTools 的工具名，并验证实际会话默认只读及显式工具白名单。 |

SDK 依据均来自安装的 `@earendil-works/pi-coding-agent` / `pi-ai` 1.0.4：

- `dist/core/settings-manager.d.ts` 的 RetrySettings 含 enabled/maxRetries/baseDelayMs/maxAgentDelayMs/provider；本轮仅公开前三个数值设置并原样传入 SettingsManager。
- `dist/core/agent-session.d.ts` 有 setModel 和只读 modelRuntime getter；`agent-session.js` 的 setModel 继续使用原 _modelRuntime 做鉴权，不会替换运行时。未写 SDK 私有成员。
- `dist/core/tools/index.js` 的 createReadOnlyToolDefinitions 包含 read/grep/find/ls。
- `pi-ai/dist/types.d.ts` 的 AssistantMessage 包含 diagnostics/errorMessage；`utils/diagnostics.d.ts` 定义结构化 error 与 details；Bedrock/Pi/Codex 适配器分别提供 status/errorCode、error.code、transport failure。没有结构化信息时返回 MODEL_ERROR，不从提供商文本猜测类别。

`config.apply` 当前仍每次重建 ModelRuntime，因此保守地列出所有持有旧运行时的已打开会话，包括只改凭据或其他配置的请求。并发打开尚未返回 sessionId 时，不能列入 apply 的响应；其 `session.open` 响应会返回布尔值 `reopenRequired`，宿主也需检查。无历史消息的空会话尚未被 SDK 持久化，宿主关闭它后应新开会话；有历史的会话按原 sessionId 恢复。

旧 `turnTimeoutMs` 已移除；宿主配置需迁移到两个新字段。配置对象按顶层覆盖，retry 对象中的数值字段可省略，此时采用 Pi 自身默认值。

验证：`npm run typecheck` 通过；完整 `npm test` 执行全部 11 个测试文件，10 个通过，只有未修改的 `e2e.test.ts` 在沙箱内失败（首个 ping 等待超时，子进程 exit=0、stderr 为空；本次输出不是 EPERM）。任务已说明沙箱子进程限制，该项需 Claude 在沙箱外复跑，未改测试或添加跳过逻辑。指定 check_lines.py 无超限；另查每行长度 ≤500 字符，git diff --check 通过。
