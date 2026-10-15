# P2.5 协议功能树详表

从 [功能树](feature-tree.md) 按职责拆出；保留既有协议、配置、迁移与验证记录。

## P2.5 协议 v2（执行报告，验收仍 BLOCKED）

唯一协议源：`agentd/src/protocol/schema.ts` / `events.ts`；Rust 镜像 `backend/pi/protocol.rs`。
两端必须一并升级；旧版/缺失 protocolVersion 的 ping 会触发 AgentdProtocolMismatch 并立即熔断。

| 入口 | 行为与依赖 | 配置、迁移与验证 |
| --- | --- | --- |
| `agentd/src/sessions/create.ts`、`options.ts`、`registry.ts` | session.open 可选 model/thinkingLevel/toolProfile；session.configure 仅空闲可用，SDK setModel/setThinkingLevel/setActiveToolsByName | defaultModel/thinkingLevel/defaultToolProfile；toolProfiles 名单由配置管理，旧 tools 只映射 default 档；`agentd/test/protocol-extension.test.ts` |
| `agentd/src/sessions/attachments.ts` | prompt.attachments 图片 base64 校验，经 SDK prompt.images 传入模型 | attachments.maxCount=4、maxBytes=262144、maxTotalBytes=524288，PNG/JPEG/GIF/WebP；faux 模型验证真实消息 |
| `agentd/src/protocol/version.ts`、`backend/pi/supervisor.rs` | ping 返回 agentd/npm 实际版本、Pi SDK VERSION、protocolVersion=6（R5 升级）；握手成功前阻止业务请求 | 沿用 requestTimeoutMs/channelCapacity；版本错误立即熔断；`pi/extension_tests.rs` 验证仅发出 ping |
| `backend/bridge.rs`、`pi/input.rs`、`pi/sessions.rs` | 每轮模型、权限及思考档位进 open/configure；已有会话每轮重新配置；TurnRequest 携带已校验图片内容；公开 PiBackend.configure_session 支持思考档位 | 复用 POLARIS_WORK_MODEL/POLARIS_FAST_MODEL，通过 models.list 唯一匹配 provider，歧义具名拒绝；Deny→readonly，其余→workspace |
| `tests/fixtures/protocol-samples.jsonl`、`protocol-schema.sha256` | 双端样例与 TS schema 哈希锁定，新增 configure/图片/版本样例 | TS 校验请求，Rust 逐条解析及往返；backend 定向测试含 R1c 回归 |

P2.5 当时缺失的 ChatSendArgs 图片/思考档位入口由下述 P2.5b 接通；P2.5 原报告保留为历史执行记录。
默认值、类型校验、切换示例、SDK d.ts 出处、命令真实结果与文件行数见 [P2.5 执行报告](pi-migration/p25-result.md)；无新依赖、未提交。

### P2.5b 聊天图片与思考档位入口（已接线，待主控复核）

入口：`src/stores/chat.ts` → `src/lib/chatImages.ts::uploadedImagePaths` → `src/tauri.ts::chat.send`。
成功上传的 image 附件传入 camelCase `imagePaths`；prompt 路径文本保持原样，旧前端省略字段可用。
`K/chat/types.rs::ChatSendArgs` 增加 serde 默认的 image_paths/thinking_level；没有现成档位选择器，不新增 UI。
`backend/bridge.rs` 经 `attach::uploads_dir` 定位当前会话 uploads，再调用公开 `backend/chat_input.rs::prepare`。
该接口复用 `images.rs::load`，拒绝相对路径、`..`、规范化越界及符号链接逃逸，验证普通文件、数量、字节和 MIME 后编码。
已知限制（R2 / 审查 #2）：规范化后、打开前，上级目录仍可能被替换为符号链接；攻击者须有会话 uploads 目录的本地写权限。
远程协作者只能提交路径字符串，不能写本机文件系统，此情形不在当前威胁模型内；fd 的 `file.metadata()` 本身使用 fstat。
若将来 uploads 对他人可写，应改用 Linux `openat2(RESOLVE_BENEATH)`；本轮仅记录限制，不修改附件代码。
`TurnRequest.attachments` 现为 `Vec<protocol::Attachment>`；Pi 直接下发内容，不再重新打开路径。
`thinking_level` 校验为 off/minimal/low/medium/high/xhigh，非法字符串报 THINKING_LEVEL_INVALID；Pi 每轮 open/configure 下发。
缺省沿用 daemon 默认/当前档位；Claude CLI 保留 prompt/Read 路径行为，不增加思考档位映射。
依赖已有 std/base64/anyhow/serde/libc 与 Pi 协议；无新依赖。headless/飞书仅补 None 默认值。
宿主配置 `POLARIS_AGENT_CONFIG` 的 `agentd.attachments` 默认与 P2.5 一致：4 张、262144/524288 字节、PNG/JPEG/GIF/WebP；
整数/正字节数/白名单均校验，maxCount=0 禁用；旧配置无需迁移。提高额度需同步 daemon attachments 和两端 maxLineBytes。
迁移：Rust TurnRequest 路径调用方应先经加载器转为 Attachment，并显式提供 thinking_level；IPC 仍使用可选 camelCase 字段。
验证：`images/tests.rs`、`chat_input/tests.rs`、`pi/extension_tests.rs` 覆盖边界及 ChatSendArgs→TurnRequest→协议；前端 `chatImages.test.ts`。
两壳 check、vue-tsc、58 个 backend 测试通过；kernel 全量 158 通过、10 项许可例外失败、2 忽略，另补跑 4 个集成测试通过。
前端两旧文件各净增 2 行；既有超长文件按例外保留。配置/调用示例、完整命令和日志见 [P2.5b 报告](pi-migration/p25b-result.md)。
Pi 安装版 read 工具支持图片内容块，可为仅有路径的 prompt 提供按需兜底；需要模型实际调用工具，不是自动附图。

