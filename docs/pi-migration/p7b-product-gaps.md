# P7b 执行报告：打包与运行中输入已实现，模块整体 BLOCKED

本报告仅为 Codex 执行结果；Claude 重跑、Pi 独立审查后决定验收。没有 commit/push，没有增加依赖，保留既有未跟踪 `docs/execution/P7b-1/`。

## 已实现入口与行为

- `node scripts/pack-agentd.mjs <out>`：用 agentd 已安装的 TypeScript 解析器递归收集相对 import/export/dynamic literal import；源图参与 stamp，编译图按目录结构复制。缺失依赖报错，不静默忽略。契约检查在生产依赖树运行；finally 删除 `.contract-tests`，产物不含 `dist/test`。
- Tauri：`chat_steer({reqId?, conversationId?, text})`、`chat_follow_up(...)`。
- HTTP：`POST /api/invoke`，`{"cmd":"chat_steer","args":{"reqId":"req-...","text":"改用中文"}}`；把 cmd 换成 `chat_follow_up` 即排队。也可只传 conversationId；两者都传时必须匹配。未知字段、错误类型拒绝，至少一个目标，文本非空且最多 2 MiB。
- `chat_send_owned` 在请求启动前登记 owner/reqId/conversationId；每个活动会话只能有一个请求。HTTP owner 来自鉴权上下文的 user_id/username，不从 JSON 接收；Tauri 为可信本地 desktop principal。HTTP 仍要求现有 owner 角色。不同 principal 的活动请求返回 `ChatForbidden`（403），不放宽原有权限。
- Pi supervisor 在 actor 内把 reqId 解析成当前 route 的 agentd session，再写 steer/followUp；不读取历史 session 映射来控制空闲会话。旧 reqId 不会命中新一轮。
- steer 在当前 agent 循环的消息边界注入；followUp 在当前回答及工具循环结束后处理；SDK 保持同一个宿主 turnId，最终只发一次 done。接受响应 `{turnId, disposition}` 仅表示已接收，随后 abort/故障仍可终止队列。
- 空闲或完成/取消的 route 返回 `ChatNotRunning`（409），不自动新建轮次。注册但尚未 started/bound 返回 `ChatTurnNotReady`（409）；尚未注册时可返回 `ChatNotRunning`。写入后由 agentd 最终判断，竞争可返回 `SESSION_NOT_RUNNING` / `TURN_NOT_READY`（409）；无自动重试或偷偷转 prompt。
- done 对外发送前解除活动绑定；RAII 覆盖错误/panic 收尾。旧 lease 释放不会删除新 reqId 的绑定。绑定是运行期状态，无数据库迁移；全局口令与开放模式仍是既有合成身份，不能区分共享该身份的客户端。
- 输入框运行时显示“插话”和“排队”；携带原请求 ID，成功才清除未改动草稿并显示用户气泡，失败保留草稿。切换会话不清除另一会话的新输入；有附件时禁用这两个纯文字入口。旧 claude-cli 后端返回 `ChatLiveInputUnsupported`。
- D1 两步骤使用真实 HTTP/权限审批屏障/原请求流/持久 session JSONL 断言同会话、消息消费顺序、单一 done 与空闲拒绝，不再直接输出 blocked。真实模型演练尚未运行，不宣称通过。

## 配置、依赖与迁移

没有新增产品开关。后端沿用 agent JSON 的 `agent.backend`（默认 pi）；例如 `{"agent":{"backend":"pi"}}`，旧 `claude-cli` 不支持这两个入口。沿用 agentd `maxQueuedMessages` 默认 8、非负整数校验（0 禁止排队）；默认文件为 `agentd/config/agent.default.json`。例如将 `{ "maxQueuedMessages": 16 }` 写入 `/path/agentd.json`，通过 `POLARIS_AGENTD_CONFIG=/path/agentd.json` 切换；宿主后端文件用 `POLARIS_AGENT_CONFIG=/path/agent.json`。队列满返回 `QUEUE_LIMIT`（409）。参数通过 Rust serde、前端发送函数及 agentd 原 schema 校验。

运行期权限绑定不落盘；agentd 原 session JSONL 持久化 queued user messages。UI 的即时用户气泡随现有 store 生命周期保留，本次没有重构 conv 历史镜像或拆分一个宿主 turn 的多段 assistant 历史。

## 自定义 CA：未实现，BLOCKED

没有写入一个无法兑现的 `authority.extraCaPem` 配置，也没有修改 D4 为假通过。

本地 ureq **2.12.1** 的 `AgentBuilder::tls_config` 可以接收 `ureq::rustls::ClientConfig`；问题不是 rustls 无法添加证书，而是保留原默认根：`src/rtls.rs::root_certs()` 和 `default_tls_config()` 为私有实现，默认根来自 `webpki_roots::TLS_SERVER_ROOTS`，该 crate 未被 ureq re-export。polaris-collab 的直接依赖没有 webpki-roots；Rust 不能直接引用仅在传递依赖中的 crate。

要精确保留 ureq 原默认根并追加 CA，最小后续方案是允许 polaris-collab 显式声明锁文件中已有、与 ureq 相同版本的 `webpki-roots`，使用 ureq re-export 的 rustls 和现有 base64 解析 PEM。它不需要新下载包，但仍是新增 Cargo 直接依赖，本次遵守“不得新增依赖”没有实施。

不新增依赖的替代是随仓维护 Mozilla 信任根快照并自行同步/审查更新，或 fork ureq 暴露默认根；两者都引入本 brief 未授权的信任材料/第三方维护范围。系统 CA 替换 webpki、只信私有 CA、跳过校验都不满足“仅追加且无配置时不变”，未采用。TLS 本地 HTTPS 行为测试和 D4 真实 TLS 因此未运行。

## 改动文件

| 文件（目录前缀省略时见上一项） | 行为 |
| --- | --- |
| `scripts/pack-agentd.mjs` | 动态源/编译依赖图与临时目录清理 |
| `scripts/runtime/contract-files.mjs`、`contract-files.test.mjs`、`pack-agentd.test.mjs` | 解析/复制、嵌套辅助模块与完整最小生产打包回归 |
| `agentd/test/sessions.test.ts` | steer 先消费，followUp 后消费，结束后两入口拒绝 |
| `src-tauri/crates/polaris-kernel/src/chat/live_input.rs` | 活动绑定、权限/参数检查、公开命令与生命周期测试 |
| `chat/pipeline.rs`、`chat/mod.rs` | owned send、lease 与模块公开接口 |
| `chat/backend/mod.rs`、`bridge.rs` | 后端接口、done 清绑定 |
| `chat/backend/claude_cli/pipeline/mod.rs` | finalizer 对外 done 清绑定 |
| `chat/backend/pi/{client,mod,supervisor}.rs` | Control::Input、actor 内原请求路由 |
| `chat/backend/pi/live_input_tests.rs` | 真实 JSONL 子进程关联、空闲/结束拒绝 |
| `src-tauri/src/apihub/{invoke,auth}.rs` | 双壳 HTTP 入口、服务端身份、403/409/400 |
| `src-tauri/src/desktop_commands.rs` | Tauri 两命令注册 |
| `src/components/chat/{LiveInputActions.vue,liveInput.ts,liveInput.test.ts}` | 新控件、发送接口与行为测试 |
| `src/components/chat/ChatComposer.vue`、`src/stores/chat.ts` | 分别净增 4 行、1 行，接入组件与原 reqId |
| `drills/d1.mjs`、`drills/lib/live-input.mjs` | 替换 blocked 步骤为真实断言 |
| `drills/test/harness.test.mjs` | 与既有 scripts 测试一致兼容 node:test/Vitest，避免全量 Vitest 无 suite 错误 |
| `docs/feature-tree.md`、本文件、`docs/drills/P7a-blockers.md` | 功能入口、边界、实际验证/阻塞更新 |

实际命令及结果见 [P7b 验证记录](p7b-validation.md)。所有新增文件及其它修改文件均不超过 500 行/行长 500；两处明确豁免的既有前端长文件只核对净增和新增行长。
