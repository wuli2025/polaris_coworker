# 任务 P4：Polaris 提供商库 → Pi 模型与凭据

先读：`polaris-kernel/src/provider/`（`mod.rs` 头注释、`store.rs` 的 `ProviderView`/`route_target`/`scope_child_claude*`/`provider_switch`、`claude_login.rs`、`codex_login.rs`、`usage.rs`）、`integrations/codex_proxy.rs` 的职责说明、`docs/pi-migration/call-sites.md` 的 provider 部分、agentd 的 `models/` 与 `config.apply`、P2.5 的协议。Pi 侧：`agentd/node_modules/@earendil-works/pi-coding-agent/docs/{providers,models,custom-provider}.md` 与 `pi-ai` 的 `CredentialStore`、OAuth 相关 d.ts。

## 现状
提供商 = 一份给 claude CLI 的环境变量（`settings_config.env`），切换时注入子进程；有隔离/联动两档（联动会写 `~/.claude/settings.json`）。Claude 订阅 OAuth（PKCE）与 Codex 设备码登录由 Rust 自己实现。用量看板读 `~/.claude/projects/**/*.jsonl`。

## 目标
1. **提供商翻译**：把每个 Polaris 提供商翻译成 agentd 的 provider 配置（baseUrl、api 类型按 `protocol` 映射到 Pi 支持的 api、模型清单）。模型清单来源按优先级：提供商配置里显式列出的模型 → `settings_config.env` 中的模型变量 → Pi 内置目录中同提供商的模型。映射表进代码常量时须有名字和注释，易变部分进配置。
2. **凭据单一来源**：Key/Token 只存一份。先核查 Pi SDK 的 `CredentialStore` 接口与 OAuth 凭据形状（含刷新），然后在下列方案中选一个并在结果里写明理由与 d.ts 出处：
   - A：agentd 实现基于 Polaris 数据目录文件的 `CredentialStore`（与 Rust 共用同一文件 + 文件锁），Rust 的登录流程写入、Pi 刷新后写回；
   - B：Rust 为唯一持有者，经 `config.apply` 下发 api_key；OAuth 刷新由 Rust 负责，临近过期前主动下发新 token；
   不论哪种：明文不得出现在日志/事件；刷新后的 token 必须持久化，重启不丢；并发刷新不能互相覆盖。
3. **切换**：`provider_switch` 改为向 agentd 下发配置（P2.5 的 `config.apply` / `session.configure`），处理 `reopenRequired`（空闲会话自动重开并保留历史；运行中的会话本轮结束后生效）。**联动模式**（写 `~/.claude/settings.json`）只对旧 claude-cli 后端有意义：Pi 后端下隐藏或明确标注无效，不再写用户的 Claude 配置。
4. **登录**：Claude 订阅与 Codex 登录——核查 Pi 是否原生支持对应 OAuth 提供商（`openai-codex`、anthropic 订阅等）；支持则复用 Pi 的流程并把凭据落到方案 A/B 的存储；不支持则保留 Rust 流程并把结果接入存储。`codex_proxy.rs` 在 Pi 后端下是否仍需要，给出结论；不需要则从 Pi 主路径摘除（文件保留给旧后端）。
5. **用量**：Pi 每轮有权威 `usage`（P1 事件里已有）。新增用量记录（按会话/提供商/模型落库，位置复用 Polaris 数据目录），用量看板合并新旧两个来源，按时间与请求去重规则写清楚；旧 jsonl 读取保留给历史数据。
6. **故障切换**：`polaris-failover` 的滑窗与 `note_provider_success/failure` 要接上 Pi 的错误分类（P1 的 `MODEL_AUTH/RATE_LIMIT/...`），保持现有自动切换行为。

## 测试
两组以上不同协议提供商的翻译结果；切换后新会话用新提供商、空闲会话重开、运行会话本轮后生效；凭据不进日志；OAuth 刷新写回且并发安全（用假时钟/假刷新端点）；用量去重；故障切换触发。

## 规则
- 允许修改：`polaris-kernel/src/provider/`、`polaris-kernel/src/chat/backend/`、`polaris-kernel/src/integrations/codex_proxy.rs`（仅摘除主路径依赖）、`agentd/`、对应测试、`docs/`。触及的超长文件（`store.rs` 2292 行、`usage.rs` 562 行、`claude_login.rs` 514 行）按职责拆分到 ≤500 行，纯搬移与行为修改分开说明。前端如需改动（例如隐藏联动开关）先在结果里列出，不直接改。
- 无新依赖；≤500 行、≤500 字符；新 Rust 代码无 unwrap/expect；可变值进配置。
- 必须通过：agentd `npm run typecheck && npm test`；两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`（基线与沙箱 TCP 项除外并列出）；`npx vue-tsc --noEmit`。不 commit。
- 结束时输出：方案选择与证据、各目标完成情况、改动文件与行数、命令结果、需要前端配合的点。
