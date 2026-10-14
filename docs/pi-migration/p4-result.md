# P4 执行报告（待 Claude 重跑 / Pi 审查）

本次只实现 P4 及附带的 permission.resolved 修复；没有提交、推送或修改前端。
最终验收状态为 **BLOCKED**：agentd 全量测试仍有两组失败，详见验证表。
代码改动和定向测试不等于模块最终完成。

## 方案 A 与 SDK 证据

选择 **A，共享文件 CredentialStore**。Polaris 的 `~/Polaris/data/providers.json`
新增 `credentials` 对象；每个提供商的凭据只在这个对象中持久化。
原 `items[].settings_config.env` 的 key 在同一次原子写中迁出，不生成第二份凭据文件。
`claude-official` 的凭据 ID 为 `anthropic`，`codex` 为 `openai-codex`，其他 ID 保持不变。

核查版本为仓库已安装的 `@earendil-works/pi-ai` / `pi-coding-agent` **1.0.4**：

| 本地证据（相对 `agentd/node_modules/@earendil-works/`） | 结论 |
| --- | --- |
| `pi-ai/dist/auth/types.d.ts`：CredentialStore、OAuthCredential、OAuthAuth | read/list/modify/delete；OAuth 是 `{type:"oauth", access, refresh, expires, ...}`；expires 为毫秒；modify 回调必须在跨进程互斥内执行 |
| `pi-ai/dist/auth/credential-store.d.ts` | InMemoryCredentialStore 只有进程内串行，不能承担本任务的持久化和跨进程锁 |
| `pi-ai/dist/auth/resolve.d.ts` 及对应 `.js` | getAuth 在 modify 内再次判断过期并刷新；刷新完成后必须持久化才释放锁；SDK 默认提前 5 分钟刷新，刷新网络超时 15 秒 |
| `pi-ai/dist/auth/oauth/anthropic.d.ts` | 原生 anthropicOAuth，支持 Claude Pro/Max 登录、刷新和请求鉴权 |
| `pi-ai/dist/auth/oauth/openai-codex.d.ts` 及对应 `.js` | 原生 openaiCodexOAuth；browser/device_code 两种流程，含 token 刷新 |
| `pi-coding-agent/dist/core/model-runtime.d.ts` | login/getAuth/registerProvider/getModels；登录由 SDK 写入注入的 CredentialStore |
| `pi-coding-agent/docs/{providers,models,custom-provider}.md` | API 类型、模型配置、原生 OAuth 与自定义 provider 的公开接法 |

不选 B：SDK 已有受锁保护的刷新逻辑；B 会重复实现定时刷新、有效期与并发协调。
Rust 只下发不含 key/token 的模型配置，Pi 登录与刷新直接写共享文件。

## 各目标与入口

| 目标 | 实现与边界 |
| --- | --- |
| 提供商翻译 | `provider/pi_translate.rs` 的 PROTOCOL_APIS 将空串/anthropic→anthropic-messages、openai→openai-completions、responses→openai-responses、chatgpt→openai-codex-responses、google→google-generative-ai。选中提供商缺模型/URL或协议未知时返回具名错误。 |
| 模型优先级 | `settingsConfig.models`（字符串或完整模型对象）→ env 模型变量→同提供商 Pi 内置目录。去重保序；其他未配置预设保留在供应商坞，不加入有效 Pi 配置。不能为无元数据的自定义网关虚构模型。 |
| 单一凭据源 | `credential_file.rs` / `agentd/src/models/file-credentials.ts` 共享 mkdir 锁和原子 rename。Rust 保存配置时重读磁盘凭据，保留 Pi 的刷新结果；掩码回传代表保留当前凭据，不把旧快照写回。 |
| 切换 | `provider_switch`→AgentBackend.provider_apply→config.apply；每次启动对话检查配置和 daemon PID，崩溃重启后重下发。agentd 空闲会话复用 SessionManager 重建、保留历史；运行会话当前轮完成后重建；新会话立即用新默认模型。 |
| reopenRequired | 返回仍需处理的会话。运行会话由 registry 延后处理；重开失败保留 pending 标记并报错，下一轮 session.configure 重试；不静默使用旧 runtime。资源加载期间与 apply 竞争的新开会话返回 reopenRequired。 |
| 联动与代理 | Pi 初始化/切换不写 `~/.claude/settings.json`、不修改 Claude 进程 env、不启动 codex_proxy；联动/路由 setter 返回 Unsupported，列表返回 false。codex_proxy 文件原样留给 claude-cli。前端标注仍需配合。 |
| 登录 | 旧 shell 命令委托新 provider.auth 协议；Claude 使用 SDK URL/手工回贴，Codex 默认使用 SDK device code（可显式请求 browser）；取消/轮询也委托 SDK。Rust 的旧实现保留给 claude-cli。 |
| 用量 | `events/usage-store.ts` 在每条 assistant message_end 写权威 usage；子会话也单独记录。记录含会话、turn、请求、提供商、模型、时间、SDK 成本，无提示词和凭据；usage_summary 合并 Pi 和原有 Claude 历史。 |
| 故障转移 | Translator 按本轮真实模型提供商归因，最终成功清零；最终 error 且分类为 MODEL_AUTH/RATE_LIMIT/NETWORK/ERROR 时调用既有滑窗。上下文超限、取消、工具/daemon 错误不触发；SDK 预检 OAuth 异常也转 MODEL_AUTH。达到阈值沿用 provider_switch。 |
| P3 小修 | permission.resolved 发送或日志失败都不能改变本地 allow/deny/cancel/expiry；只记录固定文案，不记录原始异常。 |

## 配置、迁移与切换示例

- Rust 配置默认来自 `provider/pi-default.json`；`POLARIS_PROVIDER_CONFIG` 指向 JSON 覆盖文件。
  字段未知、数值/类型不合法时报 ProviderConfigInvalid。覆盖采用顶层替换，aliases/cost 需传完整对象。
- 默认 lockTimeoutMs=30000、lockPollMs=25、contextWindow=128000、maxTokens=8192、reasoning=false、input=[text]、cost 四项为 0。
  未标价模型的零成本表示未配置价格；应在显式 models 中提供价格，不能视为免费。
- 模型变量按 ANTHROPIC_MODEL、DEFAULT_OPUS、DEFAULT_SONNET、DEFAULT_HAIKU、SMALL_FAST、OPENAI_MODEL 顺序读取。
- aliases 提供内置目录的供应商对应关系，可通过配置替换；OAuth 的两个规范 ID 属协议身份，固定在代码内。
- Anthropic 兼容提供商的 ANTHROPIC_AUTH_TOKEN 通过 Pi authHeader 保持 Bearer 语义；API_KEY 保持 API key 语义。
- `models.catalog` 返回注册用户配置之前的 SDK 内置目录；`models.list` 返回当前可选目录。
  两者含 api/baseUrl/input/cost，Rust 翻译只使用前者，禁止把旧自定义注册项当内置回退。
  翻译拒绝带 userinfo、query、fragment 的 URL，防止端点中夹带凭据进入配置/目录响应。

保存提供商示例（现有 provider_save 的 input；key 只作占位示例）：

```json
{
  "id": "gateway-a",
  "name": "Gateway A",
  "protocol": "openai",
  "settingsConfig": {
    "env": {
      "ANTHROPIC_BASE_URL": "https://gateway.example/v1",
      "ANTHROPIC_AUTH_TOKEN": "REPLACE_WITH_KEY"
    },
    "models": ["model-a", "model-b"]
  }
}
```

调用 `provider_switch("gateway-a")` 切换；换成 `protocol: "anthropic"` 即使用 Messages API。
例如覆盖文件 `{"contextWindow":64000,"maxTokens":4096,"lockTimeoutMs":45000}`，
启动时设置 `POLARIS_PROVIDER_CONFIG=/absolute/path/provider-config.json`。

- Rust 启动 agentd 时设置 POLARIS_PROVIDER_STORE、POLARIS_CREDENTIAL_LOCK_TIMEOUT_MS、POLARIS_CREDENTIAL_LOCK_POLL_MS；锁参数必须正整数。
  agentd 单独启动且未传 POLARIS_PROVIDER_STORE 时仍用内存存储，便于原有 SDK 测试/嵌入场景。
- agentd 新配置 oauthTimeoutMs=600000（正整数）、usageDir=""（关闭记录）。
  Rust 通过 POLARIS_USAGE_DIR 设置 `~/Polaris/data/pi-usage`；独立运行可配置 usageDir 为绝对路径。
- 协议升至 **v5**，两端必须一起更新；新增 provider.auth / models.catalog，更新样例、指纹及 fake-agentd。
- API key 自动在同一 providers.json 内迁移；旧 Claude/Codex 外部 CLI OAuth 文件不复制、不删除、不再作为 Pi 的活跃凭据源。
  升级到 Pi 后需重新进行一次订阅登录，避免两个客户端共享会轮换的旧 refresh token。
- 回退旧二进制前须导出凭据恢复其旧 env 格式；旧二进制不认识 credentials 字段。当前代码的 claude-cli 后端可通过视图读取新 key 格式。
- mkdir 锁不会按超时强行窃取，避免慢刷新相互覆盖。异常杀进程遗留锁时返回 CredentialStoreBusy；
  必须确认相关 Rust/agentd 进程均已停止后，人工移除 `providers.json.lock` 空目录。自动陈旧锁恢复未实现。
- Codex status 的 installed 在 Pi 下表示内置 SDK 登录可用，不再探测外部 codex CLI。

## 用量合并规则

1. UTC 日期分目录，每个 Pi 请求一个 SHA-256 命名的 JSON；同一 `(UTC 日期, provider, responseId)` 重放原子覆盖。
   无 responseId 时使用 `(sessionId, turnId, 该轮消息序号)`，工具多轮和子会话各自计数。
2. Pi 合并键为日期/提供商/请求身份；Anthropic 且有 responseId 时额外记录 legacyMessageId。
3. 旧 Claude 行按 `(原 timestamp 日期, message.id)` 去重；没有 ID 时保持旧逐行计数。
   Pi 先入集合；只有日期和明确的 Anthropic message ID 都相同才覆盖旧副本，并使用 Pi 成本。
4. 时间接近、模型相同或 token 数相同不构成去重依据；不会按切换日期丢弃旧历史。
   继续保留原 UTC 今日、近 7/30/365 日桶和 14 日趋势。
5. 写入失败发 USAGE_PERSIST_FAILED；读取坏记录返回 PiUsageInvalid，不能把损坏账本报成零用量。

## 纯搬移与行为修改

- store.rs 拆为 store/{presets,state,failover,gifts,routing,persistence,secrets,views,global,listing,child,commands,tests}.rs。
  主门面保留原公开路径；辅助可见性调整为 pub(super)。旧测试分组名称多了 tests 层。
- usage.rs 的余额查询原样搬到 balance.rs；聚合逻辑只增加 Pi 数据源和身份去重。
- claude_login.rs 的原测试搬到 claude_login_tests.rs；codex_login.rs 的写文件/时间/浏览器辅助搬到 codex_support.rs。
- 其余行为修改详见上方入口表；所有自有文件物理行数和行宽检查均不超过 500。
  完整文件清单、行数和归类见 [p4-files.md](p4-files.md)。

## 验证

所有命令在仓库根执行，注明 agentd cwd 的除外；无真实付费模型请求。

| 实际命令 | 结果 | 日志 |
| --- | --- | --- |
| `npm run typecheck --prefix agentd` | 通过 | `/tmp/p4-ts-final.log`；最终联跑 stdout 同样通过 |
| `npm test --prefix agentd` | 23 文件组：21 通过，2 失败 | `/tmp/p4-agent-final.log` |
| agentd cwd：`node --import tsx --test test/provider-credentials.test.ts test/provider-usage.test.ts test/runtime.test.ts test/permissions.test.ts` | 4 文件组通过 | `/tmp/p4-agent-target.log` |
| agentd cwd：`node --import tsx --test test/provider-login.test.ts test/provider-credentials.test.ts` | 2 文件组通过 | `/tmp/p4-login-tests.log` |
| agentd cwd：`node --import tsx test/web.test.ts` | 2 通过 / 1 失败：监听 127.0.0.1 返回 EPERM | `/tmp/p4-web-direct.log` |
| agentd cwd：`node --import tsx test/e2e.test.ts` | 首个 ping 超时，exit=0、stderr 空；与历史 P3b/R4 记录一致 | `/tmp/p4-e2e-direct.log` |
| `cargo check --offline --manifest-path src-tauri/Cargo.toml` | 通过 | `/tmp/p4-desktop-final.log` |
| `cargo check --offline --no-default-features --features server --manifest-path src-tauri/Cargo.toml` | 通过 | `/tmp/p4-server.log` |
| `cargo test -p polaris-kernel --lib --tests --offline --manifest-path src-tauri/Cargo.toml` | 175 通过、10 失败、2 忽略；失败后未进入集成测试 | `/tmp/p4-kernel-final.log` |
| 同一 kernel 命令，追加下面 10 个完整名字的 `--skip` | 175 单测 + 4 集成通过；2 忽略 | `/tmp/p4-kernel-filtered.log` |
| `npx vue-tsc --noEmit` | 通过 | `/tmp/p4-vue-final.log` |
| `git diff --check`、自有文件行数/行宽扫描 | 通过（末尾空行在首轮扫描后修正） | 本次终端输出 |

定向行为测试包括：三种协议及模型优先级/错误输入；SDK 原生 auth 保留；真实 getAuth 使用假时钟/fetch 刷新、两实例并发与重新打开存储；登录与刷新互斥；登录 UI 不返回凭据；配置/事件脱敏；会话历史保留与运行轮延后切换；请求去重；错误分类和既有滑窗阈值；通知发送失败。
自动故障切换的真实网络端到端路径尚未运行；测试验证分类、滑窗及既有切换接线，不声称真实上游验收通过。

既有四项基线：

```text
doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix
doctor::path::tests::prime_and_prepend_behaviour
doctor::verify::tests::install_root_groups_npm_entries_together
doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts
```

沙箱 TCP 六项（搬移后的名称）：

```text
integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp
integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp
provider::claude_login::tests::loopback_run_end_to_end_local
provider::claude_login::tests::loopback_run_rejects_wrong_state
provider::store::tests::local_route_tests::openai_provider_routes_via_local_router
provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

首轮内核还失败了 supervisor_exit_clears_orphan_active_entries，因为旧断言要求运行中拒绝 config.apply；
P4 要求延后应用，已更新该断言，后续全量和过滤运行均通过该项。
早期编译的拆分边界/可见性问题，以及内存测试分支的两项掩码回填失败已经修复；不把修复前失败计作最终通过。

## 前端配合与未完成项

- ProviderDock / providers store：根据现有 agent_backend_status 隐藏或明确标注 Pi 的联动、代理开关无效。
- AddProviderModal：模型清单入口与新增协议选项；可先在现有 JSON 编辑区使用 settingsConfig.models。
- 订阅登录：标注 Pi 凭据位置和首次迁移需重登；Codex 不再提示必须安装 CLI。
  Codex browser 模式遇到回环不可达时缺少手工回贴 UI，当前默认 device code 避免这个依赖。
- UsageBoard：来源说明改为 Pi 权威记录 + Claude 历史；未标价模型成本不能展示为免费。
- 未运行真实 Anthropic/Codex OAuth、付费模型、真实跨进程刷新竞速、浏览器点击与 Windows/macOS 锁/文件替换验收。
- agentd 全量失败尚未消除；按任务“必须通过”的要求保持 BLOCKED，交由 Claude 重跑和 Pi 独立审查。
