# P2.5b 执行报告（第 2 次运行）

2026-10-14。已按批复接通本模块，以下仅是执行结果，最终完成判定由 Claude 复跑与 Pi 独立审查决定。
本次无实现阻塞；全量 Rust 测试仍有 10 项已许可的基线/TCP 失败，未将全量命令记为通过。
无新依赖，无 commit/push。保留并复用上次的 images 加载器、配置与测试；未改动 `docs/execution/P2.5b-{1,2}/`。

## 改动文件与行为

Rust 路径以下以 `src-tauri/crates/polaris-kernel/src/` 为前缀。

| 文件 | 行数 | 行为 |
| --- | ---: | --- |
| `chat/types.rs` | 123 | ChatSendArgs 增加 serde 默认的 image_paths、thinking_level；IPC 字段为 imagePaths、thinkingLevel |
| `chat/attach.rs` | 294 | 提取 uploads_dir，上传与发送复用相同会话目录解析 |
| `chat/backend/chat_input.rs` | 36 | 公开 prepare：ChatSendArgs → 档位校验、images::load → TurnRequest；uploads 必须由宿主解析，不接受客户端指定根目录 |
| `chat/backend/chat_input/tests.rs` | 118 | 4 个行为测试：图片字节进入 TurnRequest、默认/空/null 兼容、六种合法档位与非法值、入口传播具名错误和配置额度 |
| `chat/backend/bridge.rs` | 65 | 实际发送调用 prepare；在 prompt 准备和 backend.start 前完成验证，复用同次加载的超时配置 |
| `chat/backend/mod.rs` | 134 | 导出 chat_input；TurnRequest.attachments 改为结构化 Attachment，新增有类型的 thinking_level |
| `chat/backend/pi/input.rs` | 28 | 将 thinking_level 传入 session options；移除适配器内再次读取图片路径的逻辑 |
| `chat/backend/pi/mod.rs` | 144 | prompt 直接使用 TurnRequest 中的已编码图片 |
| `chat/backend/pi/extension_tests.rs` | 85 | JSON ChatSendArgs → prepare → Pi fake daemon；断言 prompt 图片及文本、open/configure 思考档位；删除原文件后仍能发送 |
| `chat/backend/pi/tests.rs` | 402 | 既有 TurnRequest fixture 补 thinking_level: None |
| `headless.rs` | 75 | 仅为 TurnRequest 补 thinking_level: None |
| `integrations/feishu.rs` | 1157 | 仅为 ChatSendArgs 补两个 None，净增 2 行，按范围例外保留既有文件 |
| `chat/backend/images.rs` | 85 | 保留复用：路径边界、数量、单图/总字节、MIME 与 base64 加载 |
| `chat/backend/images/tests.rs` | 133 | 保留复用：5 个路径/符号链接/额度/类型/配置行为测试 |
| `chat/backend/config.rs` | 128 | 保留复用：agentd.attachments 默认配置、类型解析与校验 |

前端及文档：

| 文件 | 行数 | 行为 |
| --- | ---: | --- |
| `src/lib/chatImages.ts` | 7 | 从成功上传的 image 附件提取路径；空列表省略字段；不根据扩展名静默过滤不支持图片，交后端具名拒绝 |
| `src/lib/chatImages.test.ts` | 22 | 2 个测试：旧文本调用缺省、混合附件筛选和顺序/路径保留、输入不变 |
| `src/stores/chat.ts` | 667 | 增加 helper 导入与 imagePaths 调用，共净增 2 行；prompt 仍原样传入 |
| `src/tauri.ts` | 2633 | ChatSendArgs 增加 imagePaths 与枚举 thinkingLevel 类型，共净增 2 行 |
| `docs/feature-tree.md` | — | 更新入口、依赖、配置、迁移、验证及既有入口阻塞的解决情况 |
| `docs/pi-migration/p25b-result.md`、`p25b-validation/run2-*.log` | — | 本报告与本次真实验证日志 |

前端没有现成思考档位选择器（搜索 thinkingLevel/thinking_level/reasoningEffort 等无已有命中），未增加 UI。
`src/stores/chat.ts` 原 665 行、`src/tauri.ts` 原 2631 行属于既有技术债，各净增 2 行，符合各 ≤10 行例外。
飞书原 1155 行，仅增加批准的默认字段，未拆分。其余本模块自有文件 ≤500 行；新增行及所有改动文件每行 ≤500 字符。
新增 Rust 代码无 unwrap/expect；旧文件已有调用未改写。

## 输入、兼容与具名错误

会话 uploads 根由 `attach::uploads_dir(conversation_id)` 决定，与上传完全一致；无会话时沿用 scratch。
图片只来自显式 imagePaths，绝不从 prompt 猜路径。校验绝对路径、拒绝 `..`，规范化后按路径组件判断边界；
拒绝 uploads 本身的符号链接、文件/子目录符号链接逃逸、目录及非普通文件。通过后按内容签名识别 MIME 并编码。
签名识别沿用 P2.5，不声明实现完整图片解码验证。Unix 最终打开使用现有 libc 的 O_NOFOLLOW/O_NONBLOCK；
这不是 P3 OS 沙箱，不保证抵御并发恶意替换父目录。

错误码：ATTACHMENT_PATH_INVALID、ATTACHMENT_PATH_OUTSIDE_UPLOADS、ATTACHMENT_NOT_FILE、
ATTACHMENT_READ_FAILED、ATTACHMENT_COUNT_LIMIT、ATTACHMENT_SIZE_LIMIT、ATTACHMENT_TYPE_UNSUPPORTED、
THINKING_LEVEL_INVALID。非法配置沿用 InvalidAgentConfig（serde 类型错误在配置解析时返回）。
错误向上传播；chat_send 本来异步返回 reqId，因此 bridge 验证失败由现有管线发 chat:stream error + done，不会静默丢弃。

旧前端缺省/null 字段兼容，图片路径文本仍在 prompt 中；Claude CLI 继续使用 Read，新增思考档位不映射为 CLI 参数。
显式提供 imagePaths 时两种后端均进行相同的边界/额度/MIME 验证，Pi 同时收到结构化图片。
Pi thinkingLevel 省略时新会话用 daemon 默认值，已有会话保留当前值；显式值经 open/configure 下发，SDK 可按模型能力钳制。

Rust 调用迁移：TurnRequest.attachments 从 Vec<PathBuf> 改为 Vec<protocol::Attachment>，先调用加载器/prepare；
新增 thinking_level 为 Option<protocol::ThinkingLevel>，旧调用方填 None。协议版本仍为 P2.5 v2，无协议改动。

## 配置与切换示例

宿主从 `POLARIS_AGENT_CONFIG` 指定 JSON 加载 `agentd.attachments`；省略使用默认值：
4 张、单图 262144 字节、总计 524288 字节，MIME 为 image/png、image/jpeg、image/gif、image/webp。
maxCount 为非负整数，maxBytes/maxTotalBytes 为正整数，mimeTypes 只允许这四种值；maxCount=0 禁用显式图片。
旧配置无需迁移。上限均来自已有配置；本次没有增加硬编码额度。

收紧为最多一张 PNG：

```json
{
  "agentd": {
    "attachments": {
      "maxCount": 1,
      "maxBytes": 131072,
      "maxTotalBytes": 131072,
      "mimeTypes": ["image/png"]
    }
  }
}
```

启动宿主时设置 `POLARIS_AGENT_CONFIG=/absolute/path/host-agent.json`。
宿主与 daemon 为独立校验层，提高额度时需同步 daemon attachments 与两端 maxLineBytes，为 base64、prompt、JSON 留空间。
公开 prepare/load 接口也接受显式 ImageLimits，便于测试或可信调用方切换。

调用示例（imagePaths 必须使用当前会话上传 API 返回的绝对路径）：

```ts
await chat.send({
  prompt: `请描述图片：${uploaded.path}`,
  permissionMode: "auto_current",
  conversationId,
  imagePaths: [uploaded.path],
  thinkingLevel: "high",
});
```

合法档位为 off/minimal/low/medium/high/xhigh，区分大小写；删除 thinkingLevel 即使用默认/当前值。

## Pi 内置 read 图片兜底核查

重新核查本地安装 `@earendil-works/pi-coding-agent` 1.0.4：

- `agentd/node_modules/@earendil-works/pi-coding-agent/dist/core/tools/read.d.ts:16` 的输出类型支持 image 块（type/data/mimeType/note）；ReadOperations 支持 MIME 检测，默认本地文件系统，自动缩放默认开启。
- 同目录 `read.js:53` 描述支持 jpg/png/gif/webp/bmp；实现读取二进制并 processImage，成功时返回 image 内容块，处理失败只返回说明。
- `agentd/config/agent.default.json` 的 default/readonly/workspace/full 工具档均包含 read。

结论：只有 prompt 路径时，Pi 可以通过模型调用 read 读取图片兜底；需要 read 启用、模型实际调用、路径可读、处理成功和模型支持图片。
路径文本本身不会自动成为图片附件。read 支持 BMP 不扩大结构化附件的四种 MIME 白名单。
此结论来自安装包 d.ts/实现核查；未运行真实模型/read 图片端到端测试。

## 实际执行的验证命令

Rust 命令在 `src-tauri`，前端命令在仓库根目录。下表是本次运行，不复用上次日志充当通过证据。

| 命令 | 真实结果 | 日志 |
| --- | --- | --- |
| `cargo test -p polaris-kernel --lib chat::backend --offline` | exit 0；58 通过 | [backend](p25b-validation/run2-backend-tests.log) |
| `cargo check --offline -p polaris-app` | exit 0；桌面壳通过 | [app](p25b-validation/run2-app-check.log) |
| `cargo check --offline -p polaris-cli --bin polaris-server --features collab-net` | exit 0；服务端壳通过 | [server](p25b-validation/run2-server-check.log) |
| `cargo test -p polaris-kernel --lib --tests --offline` | exit 101；158 通过、10 失败、2 忽略 | [kernel](p25b-validation/run2-kernel-tests.log) |
| `cargo test -p polaris-kernel --test '*' --offline` | exit 0；补跑上条 lib 失败后未执行的集成测试，4 通过 | [integration](p25b-validation/run2-integration-tests.log) |
| `npx vue-tsc --noEmit` | exit 0，无输出 | [vue-tsc](p25b-validation/run2-vue-tsc.log) |
| `npx vitest run src/lib/chatImages.test.ts` | exit 0；2 通过 | [frontend](p25b-validation/run2-frontend-tests.log) |
| `git diff --check` | exit 0 | 无输出 |

全量失败名单与上次/P2.5 的许可例外一致，无新增失败：

```text
doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix
doctor::path::tests::prime_and_prepend_behaviour
doctor::verify::tests::install_root_groups_npm_entries_together
doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts
integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp
integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp
provider::claude_login::claude_oauth_tests::loopback_run_end_to_end_local
provider::claude_login::claude_oauth_tests::loopback_run_rejects_wrong_state
provider::store::local_route_tests::openai_provider_routes_via_local_router
provider::store::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

前四项为已知 doctor 基线，后六项为许可的沙箱 TCP 例外；保留失败日志供主控比对。构建有既有 doctor 警告，未改范围外代码。
额外运行 git status/diff/numstat、rg/read.d.ts 源码核查、Python 行数/行长/新增 unwrap/expect 扫描，见 [审计日志](p25b-validation/run2-audit.log)。
未运行：真实模型、完整聊天 UI 自动化、Windows 实机/符号链接、agentd npm 测试、cargo fmt；不将 Rust fake daemon 测试称为真实模型验证。
本模块无未完成实现和待批范围问题；上述未运行项及最终复核由 Claude 决定，不扩展 P3/P4 等范围。

上次仅附件加载器准备阶段的原验证日志（无 run2 前缀）仍保留。上次由范围限制导致的入口阻塞已按本次批复解除。
