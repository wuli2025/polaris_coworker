# 任务 P1-R1：agentd 审查意见修复（第 1 轮）

Claude 已审查 `agentd/`（提交 P1）。21 个测试通过、结构良好；以下 6 条需要修复。逐条修，每条都要有对应测试证明修好了。只改 `agentd/` 与 `docs/`，依赖不变。

| # | 级别 | 位置 | 问题 | 修复要求 |
| --- | --- | --- | --- | --- |
| F1 | 高 | `src/main.ts` 命令链 `commands = commands.then(...)` | 所有命令全局串行。`abort`/`close` 会 `await run.done`，`session.open` 要加载资源；期间其他会话的命令和 `ping` 全部被堵。工具卡住时整个服务看起来像挂了，Rust 健康检查会误判并重启，连累所有会话。 | 跨会话并发、同会话串行（按 sessionId 的队列）；`ping`、`models.list` 永远立即响应。测试：会话 A 的 abort 卡住时，`ping` 与会话 B 的 prompt 仍在毫秒级返回。关闭时仍要等全部在途命令完成。 |
| F2 | 高 | `turnTimeoutMs` 单一总时长（默认 5 分钟） | 编码类任务正常就会超过 5 分钟，被误杀。 | 改为两个配置：`turnIdleTimeoutMs`（无任何 SDK 事件多久判卡死，默认 180000）与 `turnMaxMs`（硬上限，默认 0 表示不限）。超时码区分 `TURN_IDLE_TIMEOUT` / `TURN_MAX_TIMEOUT`。测试两种超时与"持续有事件的长轮次不被误杀"。 |
| F3 | 中 | `config/agent.default.json` | `retry`、`compaction` 默认关闭：长对话会撑爆上下文，偶发网络错误直接失败。 | 默认开启；重试次数/退避进配置（映射到 Pi SettingsManager 支持的字段，先查 `node_modules/@earendil-works/pi-coding-agent/dist` 中 Settings 类型，只用真实存在的字段）。测试：配置值确实传到会话设置。 |
| F4 | 中 | `events/mapper.ts` 的 `MODEL_ERROR`；`config/load.ts` 的 catch-all | 错误一律"Model request failed"/"Cannot load configuration file"，用户无从知道是 Key 无效、限流、上下文超长、断网，还是配置哪个字段错了。 | 模型错误按 Pi 消息上的结构化信息（错误类型/HTTP 状态，若存在）归类为 `MODEL_AUTH`、`MODEL_RATE_LIMIT`、`MODEL_CONTEXT_OVERFLOW`、`MODEL_NETWORK`、`MODEL_ERROR`，消息用固定中文模板，**仍不转发原始提供商文本**。配置错误返回出错字段路径列表（只给路径，不回显值）。测试每一类。 |
| F5 | 中 | `models/runtime.ts` `apply` | `config.apply` 换了提供商后，已打开会话仍持有旧 `ModelRuntime`。 | 先查 SDK 是否支持给已有会话换模型/运行时；支持就对空闲会话生效，正在运行的会话在本轮结束后生效；不支持就在响应里返回 `reopenRequired: [sessionId...]`。测试所选行为。 |
| F6 | 高 | 默认 `tools: ["read","bash","edit","write"]` | P3 的权限闸还没做，默认就给了 bash/写文件，宿主一接上就是无限制执行。 | 默认工具改为只读（以 SDK 实际内置只读工具名为准）；需要写/执行时由宿主显式配置。在 `docs/feature-tree.md` 的 agentd 节点注明"未接权限扩展前不得开放写/执行工具"。 |

## 规则
- 每文件 ≤500 行、每行 ≤500 字符；`python3 "/mnt/c/Users/mi/Desktop/项目skill/scripts/check_lines.py" agentd --limit 500` 无超限。
- `cd agentd && npm run typecheck && npm test` 必须通过。注意：你的沙箱禁止子进程（spawn 报 EPERM），`e2e.test.ts` 在沙箱内会失败——这是环境限制，不要为此改测试或跳过它；结束时说明即可，Claude 会在沙箱外重跑。
- 结束时输出：每条 F1–F6 的处理（修复/驳回+证据）、改动文件及行数、测试结果摘要。
