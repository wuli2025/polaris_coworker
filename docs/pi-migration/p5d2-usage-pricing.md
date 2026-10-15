# P5d-2 执行报告

实现与定向行为测试已落地，全量验收 **BLOCKED**：agentd 全量有 2 个失败文件，kernel 全量有 6 个失败测试。
本报告不是验收结论；由 Claude 重跑测试、Pi 独立审查后决定模块是否完成。

## 改动文件与行为

| 文件 | 行为 |
| --- | --- |
| `agentd/src/events/usage-store.ts` | `recordUsage` 新增必传的 `Pick<ModelRuntime, "getModel">` 参数；根据记录的 provider/model 查询四项单价，写入布尔 `priceAvailable` |
| `agentd/src/sessions/registry.ts` | 仅改用量写入调用，传入 `session.modelRuntime`；配置替换不使正在运行的旧会话误用新模型价格 |
| `agentd/test/provider-usage-pricing.test.ts` | 新增 4 个行为测试，覆盖四类单价、缺模型、全零/非法价格、请求金额独立性、SDK 会话写盘和运行中配置切换 |
| `agentd/test/provider-usage.test.ts` | 更新写入接口，保留重放去重/跨日/写盘失败计数覆盖，检查缺模型写 false |
| `src-tauri/crates/polaris-kernel/src/provider/usage.rs` | 补充价格标记来源和旧记录语义说明，复用现有 `row.price_available == Some(true)` 判断 |
| `src-tauri/crates/polaris-kernel/src/provider/usage_tests.rs` | 新增 true/false/缺字段 × 零/正金额测试，检查反序列化、四个时间桶、模型明细、每日趋势和非法字段类型 |
| `docs/feature-tree.md`、本报告 | 更新入口、依赖、配置、迁移及真实验证结果 |

保留工作区原有未跟踪的 `docs/execution/P5d2-1/`；无新增依赖，无 commit/push。
所有自有改动文件均不超过 500 物理行、每行不超过 500 字符；新增 Rust 无 unwrap/expect。

## 契约、配置与迁移

沿用 P5d 已支持的字段名 `priceAvailable`（等价于 brief 中示例的 `priced`），避免产生第二套标记。
Rust `provider/pi_usage.rs::Row` 已用 `Option<bool>` 读取 `priceAvailable`，本次无需越界修改该文件。
新增布尔字段不改变账本 version 1，也不改变事件协议或前端字段。

- `input`、`output`、`cacheRead`、`cacheWrite` 四项价格都必须有限且非负，任一大于零即为 true。
- 缺模型定义、全零定义或非法价格均为 false；判定完全不读取请求金额。
- 全零定义按本任务约定归为未标价，不将其解释为免费。
- 旧记录缺字段时读取为 `None`（未知），不会根据正金额或当前模型目录推断为已标价，也不回填历史。
- false/缺字段均计入 `unpricedRequests` 和 `hasUnknownCosts`；已有金额保留。非法字符串/数字标记由 Rust 返回 `PiUsageInvalid`。
- 价格读取使用产生记录的会话 runtime，避免 `config.apply` 替换全局 runtime 后误判尚未结束的请求。

沿用已有配置：内置模型使用 SDK 目录价格，自定义模型使用 `providers[].models[].cost`。
单价单位为 USD/百万 tokens；已有 ProviderSchema 要求四项非负数，写入处另检查有限性。
自定义模型 cost 必填，不新增隐式配置默认值；未查到定义的保守默认是 false。
独立 agentd 的 `usageDir` 默认 `""`，禁用写入；Rust 宿主默认指定 `Polaris/data/pi-usage`。

切换示例：通过现有 `config.apply` 提交完整 providers 列表，保留其它 provider/model 字段，仅替换目标模型的 cost：

```json
{"input": 1.5, "output": 6, "cacheRead": 0.15, "cacheWrite": 0}
```

以上产生 true；改为下面配置后，新建/重新打开的会话产生 false：

```json
{"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}
```

配置更新仍沿现有 session reopen 机制生效；运行中会话直到结束继续使用原 runtime。
Claude 历史估价表 `POLARIS_USAGE_PRICES` 不参与 Pi 标记判定。

## 实际验证

agentd 命令目录为 `agentd/`，Rust 命令目录为 `src-tauri/`，Vitest 命令目录为仓库根。
带日志的命令使用 `set -o pipefail` 和 `2>&1 | tee <日志>`，保留真实失败退出码。

| 实际命令 | 真实结果 | 日志 |
| --- | --- | --- |
| `npm run typecheck && npm test` | typecheck 0；test 1，26/28 文件通过，e2e/web 失败 | `/tmp/p5d2-agentd.log` |
| `npm run typecheck && node --import tsx --test test/provider-usage.test.ts test/provider-usage-pricing.test.ts` | 0，两个文件通过 | 工具输出 |
| `env -u NODE_TEST_CONTEXT node --import tsx test/provider-usage-pricing.test.ts` | 0，新增 4 个用例通过 | `/tmp/p5d2-agentd-pricing.log` |
| `env -u NODE_TEST_CONTEXT node --import tsx test/provider-usage.test.ts` | 0，2 个用例通过 | `/tmp/p5d2-agentd-usage.log` |
| `cargo check --offline` | 0，桌面壳零警告 | `/tmp/p5d2-desktop-check.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 0，实际 server 入口零警告；沿用 P5d 的 server 特性组合 | `/tmp/p5d2-server-check.log` |
| `cargo test -p polaris-kernel --lib --tests --offline` | 101；207 通过、6 失败、2 ignored；包含新增价格测试通过，lib 失败后未继续 integration；编译零警告 | `/tmp/p5d2-kernel-test.log` |
| `cargo test -p polaris-kernel --test backend_config --test catalog_protocol --test credential_lock_interop --test provider_switch --offline` | 0，4 个集成测试目标共 8 个用例通过，编译零警告 | `/tmp/p5d2-kernel-integration.log` |
| `npx vitest run` | 0，14 文件/61 用例通过 | `/tmp/p5d2-vitest.log` |
| `env -u NODE_TEST_CONTEXT node --import tsx test/web.test.ts` | 1，2 通过、1 失败；监听 127.0.0.1 返回 EPERM | `/tmp/p5d2-agentd-web.log` |
| `env -u NODE_TEST_CONTEXT node --import tsx test/e2e.test.ts` | 1，首次 ping 超时；子进程 exit=0、stderr 为空，根因未进一步确定 | `/tmp/p5d2-agentd-e2e.log` |

kernel 的 6 个失败名（没有跳过或修改测试来改变全量结果）：

- `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`
- `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`
- `provider::claude_login::tests::loopback_run_end_to_end_local`
- `provider::claude_login::tests::loopback_run_rejects_wrong_state`
- `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`
- `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`

前四项为 bind PermissionDenied/Operation not permitted；后两项为本地路由无法绑定 8765–8789 端口。
这些失败及 agentd 的两个失败与已有 P5d 报告一致；未把历史报告当作本次通过证据。
环境禁止权限提升，不能在本轮解除本地监听限制；未越界调整网络、daemon 启动或既有测试。

另实际运行了 `git status --short`、`git diff --stat`/定向 diff、`git diff --check`、Python 物理行/行长检查，以及 `pwd`、`rg`、`cat`、`sed`、`wc` 进行定位和审查。
`git diff --check` 与行数/行长检查通过；8 个自有改动文件最多 461 行，最长一行 369 字符。
首次查找的 `agentd/src/sessions/factory.ts`、`agentd/test/providers.test.ts` 不存在，已定位到现有文件；未据此修改范围。

未运行：真实提供商计费请求、桌面 UI 点击、Windows/macOS、打包/部署、ignored 的两项 Rust 测试。
未完成：指定全量测试全部通过的验收条件；本轮无其它已知模块实现缺口。
