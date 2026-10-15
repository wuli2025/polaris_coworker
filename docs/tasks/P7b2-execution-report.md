# P7b-2 执行报告

状态：BLOCKED，不能宣称模块验收完成。实现与局部验证已落地；未提交、未推送。
保留任务开始时已有的未跟踪目录 `docs/execution/P7b2-1/`，没有改动其中内容。

## 提供商修复

- `WireError` 的 Display 保留 `code`、`message` 和 `paths`，supervisor 不再丢弃字段路径。
- 旧 schema 临时副本实测：OpenAI 目录的 `gpt-6-astra` 元数据返回 `INVALID_CONFIG`，路径 `/providers/0/models/0/cost/tiers`（`/tmp/p7-original-schema-paths-final.log`）。原翻译带入 SDK 的 tiers，旧 ProviderSchema 拒绝该字段；切换会翻译 OpenAI 等其他预设，因此也能阻塞 Codex。现在支持 tiers 和负数未知价格。
- 本机 Pi 1.0.4 目录实际包含 `gpt-6-astra`。测试同时使用 `polaris-out-of-catalog-test`，避免把目录内模型误当作目录外覆盖。
- `codex` → `openai-codex`，`claude-official` → `anthropic`；翻译不再要求或覆盖原生端点/API/authHeader。SDK 注册保留原生鉴权和已知模型的兼容选项。
- 字符串模型先匹配 SDK 目录，保留已知模型元数据。目录外模型使用可配置默认值：128000 上下文、8192 最大输出、text 输入、reasoning=false。
- 未知价格沿用 P5d：SDK 数值成本占位为全零，但持久化使用 `priceAvailable=false`，不能解释为免费；负数目录价格也属于不可用。没有实际调用付费模型。
- `provider_save` 构造候选视图后，通过 `AgentBackend::provider_validate` → `config.validate` 执行同一翻译和 SDK 注册。校验失败不更新 store、选择或凭据。模型类型/范围错误带编辑器字段路径，agentd 错误带 JSON Pointer。
- `models.catalog` 始终返回未被配置覆盖的 SDK 目录，`models.list` 返回活动模型。避免一次自定义注册污染后续默认值。

## 配置和迁移

提供商默认参数仍由 `K/provider/pi-default.json` 定义，可通过 `POLARIS_PROVIDER_CONFIG` 指向 JSON 覆盖文件；模型参数支持单项覆盖并校验正整数范围、上下文/输出关系、输入类型与成本结构。

HTTP `/api/invoke` 或同名 Tauri 命令的示例：

```json
{"cmd":"provider_save","args":{"input":{"id":"codex","name":"Drill subscription","settingsConfig":{"models":["gpt-6-astra"]}}}}
```

```json
{"cmd":"provider_switch","args":{"id":"codex"}}
```

旧 provider 数据无需转换。宿主与 agentd 应成对更新：新增 `config.validate` 命令；旧 agentd 不支持此命令，会显式拒绝保存。协议 v6 增量扩展，Rust 样例和 schema 指纹同步更新。

私有 CA 配置位于 `POLARIS_INTERCONNECT_CONFIG` 指向的 JSON（默认 `~/Polaris/data/interconnect.json`）：

```json
{
  "authority": {
    "url": "https://authority.internal",
    "publicKeyFingerprints": ["替换为权威公钥完整64位SHA256十六进制指纹"],
    "extraCaPem": "/etc/polaris/private-ca.pem"
  }
}
```

`extraCaPem` 默认 null，也可直接提供 `-----BEGIN CERTIFICATE-----\n...` PEM 内容。类型错误、空字符串、文件无法读取、没有证书、无效证书均拒绝。移除该字段或设为 null 即恢复原有信任行为。相对文件路径按进程工作目录解析，部署推荐绝对路径。

- ureq 使用默认 webpki 根加额外证书构造 rustls ClientConfig，不配置时仍使用原 ureq builder 的 TLS 默认值。保留各调用点既有 timeout/redirect 设置。
- 所有新路径继续校验证书链、主机名、有效期，不存在跳过验证分支。
- 权威公钥获取、账号登录/转发、浏览器互联、吊销轮询、风险批准、云网关注册使用统一构造。证书错误具名为 `InterconnectTlsCertificateError`，提示检查 `authority.extraCaPem`、主机名和有效期。
- iroh 主机/客户端/网关 endpoint 的 relay TLS 使用同一 PEM 解析入口与追加根策略。已有 endpoint 保持构造时配置，改 CA 后重建 endpoint 或重启。
- 仅新增 `polaris-collab` → `webpki-roots 0.26.11` 依赖边，与 ureq 2.12.1 相同；Cargo.lock 不新增 package 条目。
- D4 member 配置注入临时 CA，新增 `tls` 步骤通过实际生产 ureq 转发验证；其余跨主机准入/真实设备入网演练仍按原边界报告 blocked。

## 文件清单

路径简写沿用 `docs/feature-tree.md`：K=kernel/src，C=collab/src。

| 文件 | 改动 |
| --- | --- |
| `K/provider/pi_translate.rs`、新增 `pi_model.rs` | 原生翻译、目录匹配、具名字段校验 |
| `K/provider/store/commands.rs`、`store/tests.rs` | 保存前 SDK 校验；旧 CLI store 测试明确验证原有模式 |
| `K/chat/backend/mod.rs`、`pi/mod.rs` | 公开保存校验接口、Pi `config.validate` 适配 |
| `K/chat/backend/pi/protocol.rs`、`supervisor.rs` | 新命令和完整错误路径透传 |
| `agentd/src/config/schema.ts`、`models/runtime.ts` | 原生注册、tiers/未知价格、稳定内置目录 |
| `agentd/src/protocol/schema.ts`、`daemon.ts` | 无配置写入的校验命令 |
| `agentd/test/native-provider.test.ts`、`catalog.test.ts` | 原生 OAuth、目录外模型、价格不可用、候选验证不变更运行时 |
| `src-tauri/crates/polaris-kernel/tests/native_provider.rs` | 真实 Rust→agentd 保存/切换、无效保存不覆盖、WireError 路径 |
| `src-tauri/crates/polaris-kernel/tests/fixtures/{fake-agentd.mjs,protocol-samples.jsonl,protocol-schema.sha256}` | 新命令、具名错误注入与契约样例 |
| `src-tauri/tests/provider_http.rs` | 实际 axum invoke 路由与共享 Tauri 命令返回值比较；使用协议 fixture 注入错误 |
| `src-tauri/crates/polaris-collab/Cargo.toml`、`src-tauri/Cargo.lock` | 已批准且锁定版本的依赖边 |
| 新增 `C/collab/interconnect/http_client.rs`、`http_client/tests.rs` | 统一 CA 构造、诊断、测试生成证书、真实 HTTPS 和内存 TLS 握手 |
| `C/collab/interconnect/{mod.rs,defaults.json,schema.json}` | 可选配置、类型校验、默认值 |
| `C/collab/authority.rs`、`authority/trust.rs` | 权威请求使用 CA 构造 |
| 新增 `C/collab/authority/{membership.rs,tests.rs,host_tests.rs}` | 将原有 1137 行 authority 文件按职责拆分，原成员逻辑和测试保留 |
| `C/collab/interconnect/{browser/authority.rs,sync.rs,gate.rs}`、`C/collab/commands.rs` | 浏览器/轮询/批准/注册客户端接线 |
| `C/collab/{tunnel/endpoint.rs,gateway/endpoint.rs}` | relay TLS 追加 CA |
| `drills/{d4.mjs,lib/d4-topology.mjs,README.md}` | 私有 CA 配置、真实转发 tls 步骤、演练边界说明 |
| `docs/feature-tree.md`、本报告 | 入口、依赖、配置、迁移和真实验证结果 |

## 验证与阻塞

以下 Cargo 命令工作目录为 `src-tauri/`；前端命令工作目录为仓库根。部分等价调用使用 `--manifest-path src-tauri/Cargo.toml`。日志均为本轮实际结果，没有将未执行项记为通过。

| 实际命令 | 真实结果 / 日志 |
| --- | --- |
| `cargo check --offline` | 0，零警告；`/tmp/p7-final-desktop.log` |
| `cargo check --no-default-features --features server --offline` | 0，零警告；`/tmp/p7-final-server.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 0，零警告；`/tmp/p7-final-net.log` |
| `cargo build --offline` | 0，零警告；`/tmp/p7-build.log` |
| `cargo test -p polaris-kernel -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | 101；collab 178 通过、1 失败（真实 HTTPS 测试 bind 被拒），kernel 尚未执行；`/tmp/p7-final-combined-tests.log` |
| `cargo test -p polaris-kernel --lib --tests --offline` | 101；222 通过、6 失败、2 既有 ignored；均为 TCP/OAuth loopback/本地路由端口相关；`/tmp/p7-final-kernel-tests.log` |
| `cargo test -p polaris-kernel --test '*' --offline` | 0；5 个集成测试文件共 9 测试通过，含真实 native 保存/切换及并发切换；`/tmp/p7-final-kernel-integration.log` |
| `cargo test -p polaris-app --no-default-features --features server,collab-net --test provider_http --offline` | 0；1 通过；实际 HTTP 路由与共享 Tauri 命令错误相同；`/tmp/p7-http-test.log` |
| `cargo test -p polaris-collab --features collab-host,collab-net --lib ca_chain_and_hostname --offline` | 0；1 通过；实际 rustls 内存握手，默认拒绝、追加 CA 成功、错误主机名拒绝；`/tmp/p7-ca-memory.log` |
| `npm run typecheck && npm test`（agentd/，等价 `npm --prefix agentd ...`） | typecheck 0；test 1，30 文件中 28 通过、2 失败（e2e/web）；`/tmp/p7-typecheck-final.log`、`/tmp/p7-agent-tests-final.log` |
| `node --import ./agentd/node_modules/tsx/dist/loader.mjs --test agentd/test/native-provider.test.ts` | 0；原生提供商与配置校验测试通过；`/tmp/p7-native-final.log` |
| `node --import ./agentd/node_modules/tsx/dist/loader.mjs agentd/test/catalog.test.ts` | 0；2 通过；`/tmp/p7-catalog-final.log` |
| `npx vue-tsc --noEmit` | 0；`/tmp/p7-vue.log` |
| `node --test drills/test/*.test.mjs` | 0；harness 测试文件通过；`/tmp/p7-drills.log` |
| `git diff --check` | 0；无空白错误 |

额外静态核对：42 个本轮改动文件（最终清单以 git status 为准）；自有源码/文档/配置均 ≤500 物理行、每行 ≤500 字符。Cargo.lock 为既有生成文件，package 条目保持 997→997，diff 仅新增一条依赖边。

过程中的失败未隐藏：初次校验中修复了 authority 拆分后的尾部 doc 注释和未使用 import、原生测试误把已在目录内的 gpt-6-astra 当目录外、catalog 测试旧的污染目录断言、旧 CLI store 测试未显式选择旧保存契约。曾从错误目录调用 npm/cargo，分别因缺脚本/manifest 失败，随后在正确目录重跑。初次基线探针错误地找 Anthropic tiers，未找到；更正到 OpenAI 后获得上面的真实错误路径。

1. 任务修改白名单不含 `src-tauri/src/mesh.rs` 和 `src-tauri/src/mesh/security.rs`。二者仍直接创建默认 ureq 客户端访问权威；已提出范围确认，在获得授权之前不修改。因此“所有客户端统一使用”不能标记完成。
2. 执行沙箱禁止本地 TCP 监听。新增真实 HTTPS 测试报 `Operation not permitted`；kernel 6 项 TCP/OAuth loopback/本地路由测试、agentd web HTTP 测试也失败。保留失败，未新增 ignore 或成功断言掩盖。
3. agentd 既有 e2e 子进程测试在 ping 前报告 `Daemon closed: 0`。已在临时副本恢复 HEAD 的四个 agentd 源文件重跑，得到同样失败（`/tmp/p7-e2e-baseline.log`）；不归因于此次修复，也未越界修改运行入口。
4. D4 Docker/TLS 演练、真实订阅模型请求、桌面 WebView/Tauri invoke 端到端未运行。HTTP 路由测试和共享命令返回值测试不等同桌面 UI 实测。
