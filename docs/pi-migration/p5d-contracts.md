# P5d 执行报告

状态：**BLOCKED（部分实现）**。上下文与专家读写链路已实现，用量聚合/展示已实现；Pi 生产用量价格标记尚未接通，agentd 必须全通过的检查仍有环境失败。
本报告不是模块验收结论；Claude 自行重跑、Pi 独立审查，最终由 Claude 决定。未 commit/push；未改动已有 `docs/execution/P5d-1/`。

## 剩余阻塞

1. agentd `events/usage-store.ts` 当前记录模型名与数值 cost，不记录“价格是否已配置”。零费用不能区分免费和缺价；聚合端不能从金额猜完整性。
   TASK DATA 只允许修改 agentd 以暴露专家配置读取。已请求允许扩展到用量记录链路及测试，尚未收到授权，因此没有越界修改该链路。
   Rust 支持可选 `priceAvailable`，缺省保守计入未标价并保留已记录金额。测试中的显式标记为契约夹具，不代表生产已能写出标记。
   后续须将提供商显式价格/默认占位价的来源传至 agentd，并在请求发生时记录该标记；不能用当前价格反推历史或把零价默认值当免费。
2. agentd `npm test`：27 文件，25 通过、2 失败。web 本地监听 EPERM；e2e 首次 ping 超时、子进程 exit=0、stderr 空。
   与 `p4-result.md` / `p5a2-closeout.md` 既有记录一致；本轮未弱化断言或将这些失败改成通过。当前沙箱不允许提升权限。

## 实现与公开契约

上下文 `context_file_metadata(projectId, createFile?)` 使用 `conv::project_work_dir` 和实际 chat 的 `effective_cwd`，不是托管项目目录。
返回 `root/fileName/relativePath/agentsExists/claudeExists/exists`。选择顺序 AGENTS.md → CLAUDE.md → createFile 默认 AGENTS.md。
`context_file_read(projectId, selection)`、`context_file_write(projectId, selection, content)` 使用同一 selection，不重新按优先级选文件。
若项目 cwd 变更，返回 `ContextSelectionStale`；文件名仅接受两种固定名称，拒绝父路径、目录、符号链接（包含悬空链接），规范化后复用 `path_contains`。
Unix 写入还使用 O_NOFOLLOW；这不声称能防御拥有目录写权限的恶意进程并发替换整棵父目录。
不存在返回 `ContextNotFound`，权限不足 `ContextPermissionDenied`，其它 I/O `ContextIoFailed`；metadata 只把 NotFound 当缺失，不把读取失败回退到另一个文件。
前端缺文件时给空编辑区与创建说明；读取失败时不给可保存 selection。切换项目未重新读取时，保存仍绑定原加载项目。
旧 `project_claude_md_path` / `kb_claude_md_path` 及旧 list/read/write 保留原行为；Pi 使用独立新编辑器，旧后端保留旧面板。

用量 `TokenBucket` 新增 `unpricedRequests/hasUnknownCosts/models`；models 每项含 `source/model/hasPrice/pricedRequests/unpricedRequests/cost`。
`hasPrice` 表示该来源模型本窗口内所有请求均具价格依据；混合记录 false。每日趋势同样带未知请求数和布尔标记。
cost 始终保留已估价与历史已记录部分；未知模型不再默认套 Sonnet 价格。前端显示“已标价 / 已记录成本”和“另有 N 次请求未标价”。
Pi 缺标记的历史行一律未知，正金额仍保留；只有显式 true 才认为该 Pi 行完整，显式免费价格允许 0 成本。旧宿主缺字段显示“价格完整性未知”。
Claude 历史继续使用原来的模型族估价规则，已移到默认 JSON，未宣称为最新真实账单。

专家 `expert_enabled_get()` / `expert_enabled_set(enabled: boolean)` 已注册 Tauri 与 apihub；后者不使用宽松的默认布尔转换，非法类型直接拒绝。
实际状态来自 agentd ping 新增的可选 `experts` 字段；旧 daemon 缺该字段报 `ExpertConfigReadUnsupported`。
设置经 `config.apply`、再次读取校验后原子持久化；保留 daemon 已有 prompt，写失败回滚 runtime，回滚也失败时报告两项错误。
宿主重启后在首次状态查询或发轮次时恢复持久配置；复用 agentd 既有 idle reopen / busy deferred reopen，不中断正在执行的轮次。
前端共用 `ExpertRuntimeNotice`，只在实际返回成功后更新状态；失败显示未知与错误。浏览器预览的新读写接口明确拒绝，不返回伪成功。

## 配置与切换

- 上下文创建策略是 metadata 的显式参数：`{ "projectId": "真实项目ID", "createFile": "CLAUDE.md" }`；省略默认 AGENTS.md，无效名称拒绝。
- 宿主 `POLARIS_AGENT_CONFIG` 可配置 `{"agentd":{"expertSettings":"/absolute/path/expert-settings.json"}}`；默认 `~/Polaris/data/expert-settings.json`。
  字段类型 PathBuf，校验为绝对文件路径；文件内容为严格 `{"enabled":true,"prompt":"已有部署提示词"}`，未知字段或非布尔 enabled 拒绝。
  通常由 UI 切换，关闭传 false。文件不存在时沿用 daemon 实际部署值（agentd 默认 false），文件读失败不会冒充默认关闭。
- `POLARIS_USAGE_PRICES=/absolute/prices.json` 替换完整历史估价表；不设置使用 `provider/usage_prices.json`。
  示例：`[{"contains":["custom-model"],"rates":[1,2,0.1,0.05]}]`；rates 顺序 input/output/cache-write/cache-read，单位 USD/百万 tokens。
  可用 `prefixes`；规则按顺序匹配，必须有非空 matcher，四项价格必须有限且非负，未知键拒绝。空数组表示所有历史模型未标价。
  仅影响 Claude 历史估价；不覆盖 Pi 请求成本，也不补造 Pi 历史完整性。

## 实际验证

Rust 命令目录 `src-tauri/`，agentd 命令目录 `agentd/`，前端命令目录仓库根。

| 实际命令 | 结果 / 日志 |
| --- | --- |
| `npm run typecheck && npm test` | typecheck 0；test 1，25/27 文件通过；`/tmp/p5d-agentd-types-final.log`、`/tmp/p5d-agentd-test-final.log` |
| `cargo check --offline` | 0，0 警告；`/tmp/p5d-desktop-check-final.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 0，0 警告；`/tmp/p5d-server-check-final.log` |
| `cargo check --offline --no-default-features --features server` | 额外检查退出 0，有 4 条既有 mesh.rs 警告；`/tmp/p5d-server-check.log`；未为消警告越界修改 mesh |
| `cargo test -p polaris-kernel --lib --tests --offline` | 101；206 通过、6 个 TCP 沙箱失败、2 ignored，因 lib 失败未继续 integration；`/tmp/p5d-kernel-test-final.log` |
| 同上加下列 6 个 `--skip` | 0；206 单测 + 8 集成测试通过，6 filtered、2 ignored；`/tmp/p5d-kernel-filtered-final.log` |
| `npx vue-tsc --noEmit` | 0；`/tmp/p5d-types-final.log` |
| `npx vitest run` | 0，14 文件 / 61 用例通过；`/tmp/p5d-vitest-final.log` |
| `npx vite build` | 0；仍有 >500 kB chunk 提示；`/tmp/p5d-build-final.log` |
| `env -u NODE_TEST_CONTEXT node --import tsx test/expert-settings.test.ts` | 0，新增专家端到端 SDK 测试通过；`/tmp/p5d-experts-direct.log` |
| `env -u NODE_TEST_CONTEXT node --import tsx test/web.test.ts` | 1，2 通过、1 监听 EPERM；`/tmp/p5d-agentd-web-direct.log` |
| `env -u NODE_TEST_CONTEXT node --import tsx test/e2e.test.ts` | 1，首次 ping 超时、exit=0；`/tmp/p5d-agentd-e2e-direct.log` |
| `git diff --check`、Python 物理行/行长检查 | 通过；新增文件均 ≤500 行/每行 ≤500 字符，旧超长文件净增均 ≤20 |

排除的 6 个测试（使用完整命令末尾的 `-- --skip <name>` 逐个传入）：

- `e2e_anthropic_passthrough_over_real_tcp`
- `e2e_openai_route_over_real_tcp`
- `loopback_run_end_to_end_local`
- `loopback_run_rejects_wrong_state`
- `openai_provider_routes_via_local_router`
- `route_all_switch_reroutes_anthropic_provider`

探索/调试命令也实际执行：`pwd`、`git status/diff/show/ls-files`、`rg`、`find ... -name AGENTS.md`、`cat/sed/tail/wc`、Python 文件接线及统计；少数猜测路径不存在，随后已定位实际文件。
首轮内核 check/前端检查通过；新前端测试一轮 2 失败为 beforeEach 返回 mock 被 Vitest 当 cleanup 执行，改为不返回后 61 项通过。
`node --test --test-reporter=spec` 仍只给文件级失败；尝试 `--test-isolation=none` 被 Node 22.19.0 拒绝（退出 9），改为直接运行测试文件获得具体错误。
未运行：真实桌面/浏览器点击、Windows/macOS、真实提供商计费推理、宿主服务重启实机验收。fake daemon 持久化测试和 SDK faux-provider 测试不等于这些实机验收。

## 改动文件

下列均为本任务变更；没有新依赖。`K` = `src-tauri/crates/polaris-kernel/`。

| 文件 | 行为 |
| --- | --- |
| `K/src/claude_md.rs`、`K/src/claude_md/context.rs`、`context_tests.rs` | 新上下文公开契约；选择四组合、权限失败、原文件写入、创建策略、越界测试 |
| `K/src/provider/usage.rs`、`usage_costs.rs`、`usage_prices.rs`、`usage_prices.json`、`usage_tests.rs`、`pi_usage.rs` | 完整性聚合、配置化历史估价、Pi 可选标记与行为测试 |
| `K/src/chat/backend/config.rs`、`mod.rs`、`pi/mod.rs`、`pi/experts.rs`、`pi/protocol.rs` | 专家宿主契约、持久恢复、config.apply、实际读取、回滚测试 |
| `K/tests/fixtures/fake-agentd.mjs`、`protocol-schema.sha256` | 专家协议夹具和 schema 指纹同步 |
| `src-tauri/src/expert.rs`、`expert/settings.rs`、`desktop_commands.rs`、`apihub/chat_services.rs`、`apihub/engines.rs` | 两壳命令注册和严格参数分发 |
| `agentd/src/daemon.ts`、`protocol/schema.ts`、`agentd/test/expert-settings.test.ts` | 仅新增专家实际读取；默认关闭、开/关新会话、空闲 reopen、非法输入测试 |
| `src/features/runtime/contracts.ts`、`useExpertSettings.ts`、`components/ContextFileEditor.vue`、`components/ExpertRuntimeNotice.vue` | 独立前端读写逻辑及展示；ExpertCenter/ExpertTeam 已有引用自动接通，无需改旧文件 |
| `src/features/runtime/usageCosts.ts`、`copy.ts`、`src/types/usageCosts.ts` | 费用展示/完整性类型及文案 |
| `src/components/ClaudeMdPanel.vue`、`UsageBoard.vue`、`ProviderDock.vue`、`src/tauri.ts` | 接线；净增依次 +2、0、0、+7，保留原 CRLF 文件格式 |
| `tests/frontend/runtimeContracts.test.ts`、`runtimePreview.test.ts` | selection 原样写回、错误传播、开关成功/失败、费用三组合、预览拒绝 |
| `docs/feature-tree.md`、`docs/pi-migration/p5c-frontend.md`、本文 | 入口/依赖/配置/迁移/验证及剩余阻塞 |
