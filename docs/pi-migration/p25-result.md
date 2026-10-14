# P2.5：agentd 协议 v2 执行报告

2026-10-14。状态 **BLOCKED（部分实现已验证，未声明模块完成）**。
仅修改任务允许的 agentd、kernel/backend、kernel/tests 与 docs；无新依赖、无 commit/push。
原有未跟踪的 `docs/execution/P2.5-1/` 保留且未改动。
最终完成判定留给 Claude 复跑与 Pi 独立审查；R1c 并入送审，未另改其收尾逻辑。

## 逐项结果

| 要求 | 执行结果 |
| --- | --- |
| session.open 可选 model/thinkingLevel/toolProfile | 已实现；省略使用 agentd 默认配置，恢复会话也传入本次选择；响应包含实际生效设置 |
| session.configure | 已实现；registry 轮次/配置锁及 SDK isIdle 检查，忙碌报 SESSION_BUSY；模型、思考、工具使用真实 SDK API |
| 图片附件 | 协议和 TurnRequest 路径入口已实现；支持 PNG/JPEG/GIF/WebP；按数量、单图和合计解码字节限制；faux 断言图片实际进入模型消息 |
| ping / 协议握手 | 已实现；version 来自 agentd package.json，piVersion 来自 SDK VERSION；protocolVersion=2；Rust 业务请求等待握手，版本错误立即熔断 |
| Rust 类型、样例、哈希 | 已同步；fixture 保留旧的可选字段缺省样例，新增完整 open/configure/图片/ping 样例；TS 校验请求，Rust 逐条解析及往返 |
| 每轮模型与权限 | 已实现；TurnRequest.model 的 provider/id 进入请求，bridge 复用已有 POLARIS_WORK_MODEL/POLARIS_FAST_MODEL；仅模型 ID 时查目录唯一匹配，否则具名拒绝；Deny→readonly，其余→workspace |
| 聊天 UI 图片及思考档位入口 | **BLOCKED**：ChatSendArgs 无 attachments/thinkingLevel；TurnRequest 也无独立 thinkingLevel。未猜测 prompt 内路径、未修改范围外类型/调用点。PiBackend.configure_session 暴露有类型的思考档位配置 |
| 必过验证 | 两壳 check 通过；Rust 除许可的 4 基线+6 TCP 项之外均通过；agentd typecheck 通过，npm test 仍有原 e2e 子进程失败，故不满足完整验收 |

Polaris 的 providerId 属于宿主供应商标识，仍明确拒绝直接当作 Pi provider；P4 负责映射。
要补齐聊天 UI 结构化输入，需要另行授权 `src-tauri/crates/polaris-kernel/src/chat/types.rs`、相关调用方及前端传参路径。
若在 TurnRequest 新增必填结构字段，还需同步范围外的 `src-tauri/crates/polaris-kernel/src/headless.rs` 初始化；本次没有修改。

## SDK 能力核查（安装版本 1.0.4）

以下均来自 `agentd/node_modules/@earendil-works/pi-coding-agent/dist/` 的实际 d.ts：

| 出处 | 能力与使用方式 |
| --- | --- |
| `core/agent-session.d.ts:637` | setModel(model, options?) → Promise<void>；会校验凭据；本次 persist:false，失败映射 MODEL_CONFIGURE_FAILED，不回显 provider 错误中的秘密 |
| `core/agent-session.d.ts:654` | setThinkingLevel(level, options?)；persist:false；SDK 按模型能力钳制档位，响应返回实际 thinkingLevel |
| `core/agent-session.d.ts:423` | setActiveToolsByName(string[])；仅激活注册工具，未知/隐藏名会被忽略；本次先检查 getAllTools，再核对实际激活名单，失败 TOOL_PROFILE_UNAVAILABLE |
| `core/agent-session.d.ts:400` | isIdle；configure 同时检查宿主 run/configuring 状态与 SDK 空闲状态 |
| `core/agent-session.d.ts:174` | PromptOptions.images?: ImageContent[]；本次 prompt(text,{images,expandPromptTemplates:false}) |
| `core/sdk.d.ts:18,20,45` | createAgentSession 的 model、thinkingLevel、tools 参数；注册配置中的工具全集后选择当前档位，支持 readonly↔workspace 切换 |
| `index.d.ts:2`、`config.d.ts:99` | 公共 VERSION 导出；用于 piVersion，未硬编码安装版本 |

图片结构来源：`@earendil-works/pi-ai/dist/types.d.ts:277` 的 ImageContent（type/image、mimeType、data）。
三个会话字段都有真实 SDK API，无需伪造“不支持”的实现。模型不存在报 MODEL_NOT_FOUND；未知档位报 TOOL_PROFILE_NOT_FOUND。
配置中新增的工具若旧会话未注册，报 TOOL_PROFILE_UNAVAILABLE，需关闭重开。config.apply 原有 reopenRequired 机制保留。
这只是激活工具档位，不声称完成 P3 的工具权限或 OS 沙箱隔离。

## 配置、默认值与迁移

agentd 配置仍从默认 JSON、`--config` / `POLARIS_AGENTD_CONFIG`、`config.apply` 合并；新增字段均经 TypeBox 校验。

```json
{
  "defaultModel": { "provider": "anthropic", "id": "claude-sonnet-4-5" },
  "thinkingLevel": "medium",
  "defaultToolProfile": "default",
  "toolProfiles": {
    "default": ["read", "grep", "find", "ls"],
    "readonly": ["read", "grep", "find", "ls"],
    "workspace": ["read", "grep", "find", "ls", "edit", "write"],
    "full": ["read", "grep", "find", "ls", "edit", "write", "bash"]
  },
  "attachments": {
    "maxCount": 4,
    "maxBytes": 262144,
    "maxTotalBytes": 524288,
    "mimeTypes": ["image/png", "image/jpeg", "image/gif", "image/webp"]
  }
}
```

- model 要求非空 provider/id；thinkingLevel 枚举 off/minimal/low/medium/high/xhigh。
- toolProfile 是配置中的非空名字；协议不接受工具列表。toolProfiles 使用确切工具名，不使用通配符；名单元素非空、不重复，默认档必须存在。
- 兼容旧 `tools` 配置：当同一次 patch 未提供 toolProfiles 时，tools 更新名为 default 的档位；不会覆盖 readonly。两者同时出现时以 toolProfiles 为准。
- maxCount 是非负整数，0 禁用图片；maxBytes/maxTotalBytes 为正整数，按 base64 解码后字节计数；MIME 配置只能从四种支持格式中选择。
- 非规范 base64 报 ATTACHMENT_INVALID_BASE64；超数量 ATTACHMENT_COUNT_LIMIT；超字节 ATTACHMENT_SIZE_LIMIT；不支持类型 ATTACHMENT_TYPE_UNSUPPORTED；纯文本模型 MODEL_IMAGES_UNSUPPORTED。
- maxLineBytes（两端默认 1048576）仍是整个 JSONL 帧上限。提高附件额度时必须同步预留 base64、提示词和 JSON 开销；Rust 读路径也以该值限制内存。
- session.configure 未提供的字段保留当前值；session.open 未提供字段用当前 agentd 默认值。配置响应返回实际档位（包括 SDK 钳制后的 thinkingLevel）。
- Rust 新轮次在已有 UUID 上收到 SESSION_ALREADY_OPEN 后发送 configure，避免复用过期缓存的模型/权限；保存最新响应。

切换示例（先用真实的 sessionId 替换 `s`，model 需存在于 models.list）：

```json
{"id":1,"method":"session.open","params":{"thinkingLevel":"low","toolProfile":"readonly"}}
{"id":2,"method":"session.configure","params":{"sessionId":"s","model":{"provider":"anthropic","id":"claude-sonnet-4-5"},"thinkingLevel":"high","toolProfile":"workspace"}}
{"id":3,"method":"ping","params":{}}
```

Rust 公开入口：`PiBackend::configure_session(key, protocol::SessionOptions)` 使用 Polaris 会话键查 UUID；运行中的会话仍由 daemon 拒绝。
每轮环境选择沿用 `POLARIS_FAST_MODEL=<精确模型ID>` / `POLARIS_WORK_MODEL=<精确模型ID>`；无法唯一定位 provider 时返回 AgentdModelAmbiguous，无匹配返回 AgentdModelNotFound。
协议 v1 不能与 v2 混用；缺失、非整数或不同 protocolVersion 均 AgentdProtocolMismatch，立即锁定熔断，需升级匹配组件并重启宿主。

## 实际验证

命令在 agentd 或 src-tauri 目录执行；原日志见本表。没有跳过测试制造绿色结果。

| 命令 | 实际结果 |
| --- | --- |
| `npm run typecheck && npm test`（agentd） | typecheck 0；npm test 1，13 文件中 12 通过、1 失败（原 e2e）；[typecheck](p25-validation/typecheck.log)、[npm test](p25-validation/agentd-tests.log) |
| `node --import tsx test/protocol-extension.test.ts`（agentd） | 0，6 行为测试通过；[日志](p25-validation/extension-tests.log) |
| `node --import tsx test/e2e.test.ts`（agentd，单独诊断） | 1，首次 ping 超时，child exit=0、stderr 为空；[日志](p25-validation/e2e-diagnostic.log) |
| `cargo check --offline -p polaris-app` | 0；[日志](p25-validation/app-check.log) |
| `cargo check --offline -p polaris-cli --bin polaris-server --features collab-net` | 0；[日志](p25-validation/server-check.log) |
| `cargo test -p polaris-kernel --lib --tests --offline` | 101；149 通过、10 失败、2 忽略；失败只在下列许可项；lib 失败后未执行集成测试，已另跑；[日志](p25-validation/kernel-tests.log) |
| `cargo test -p polaris-kernel --lib chat::backend --offline` | 0；49 通过，包括 R1c 收尾、协议样例、版本熔断和实际请求字段；[日志](p25-validation/backend-tests.log) |
| `cargo test -p polaris-kernel --test backend_config --offline` | 0；2 通过；[日志](p25-validation/config-tests.log) |
| `git diff --check`、自有文件行数/行长/范围检查 | 0；文件均 ≤500 物理行，每行 ≤500 字符；见 [清单](p25-files.md) |

4 项既有基线失败：

```text
doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix
doctor::path::tests::prime_and_prepend_behaviour
doctor::verify::tests::install_root_groups_npm_entries_together
doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts
```

6 项沙箱 TCP 失败（前四项 PermissionDenied/Operation not permitted，后两项端口无法绑定）：

```text
integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp
integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp
provider::claude_login::claude_oauth_tests::loopback_run_end_to_end_local
provider::claude_login::claude_oauth_tests::loopback_run_rejects_wrong_state
provider::store::local_route_tests::openai_provider_routes_via_local_router
provider::store::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

agentd E2E 症状与 [既有 R1 记录](../agentd-review-r1.md) 一致；未修改/跳过该测试，也没有证据将原因确定为本次实现或某一种沙箱限制。
这一失败不属于任务允许的 Rust 例外，必须由 Claude 在可运行子进程的环境复跑，当前完整验收 BLOCKED。

实现中的失败记录：初次 Rust check 发现 Arc<Client> 引用类型错误，已修复；新图片测试最初用了无效 PNG，已改为有效 PNG 并验证模型消息。
首次新增恢复测试未写入任何轮次，SDK 没有持久化会话，已改为先写入一轮后恢复验证；修复后新测试全部通过。
另有工作目录误用：根目录 `npm run typecheck` 无脚本、src-tauri 内重复前缀 touch 失败、根目录执行 agentd 单文件测试找不到入口；随后均在正确目录执行，不计为通过证据。
两壳 check 有 6 个既有 doctor 警告，测试构建有 3 个既有 doctor 警告；本次不改范围外警告。

未运行：真实网络模型、Windows 实机、聊天 UI 结构化附件端到端测试、P3 权限/沙箱验收；未运行 cargo fmt（本环境既有记录无 rustfmt），不声称其通过。
逐文件行为和物理行数见 [改动清单](p25-files.md)。
