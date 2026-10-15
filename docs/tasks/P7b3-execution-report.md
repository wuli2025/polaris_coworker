# P7b-3 执行报告

状态：**BLOCKED（验证未完成）**。实现已落地；本报告不是验收完成声明。
Claude 需重跑测试，Pi 独立审查，最终由 Claude 决定是否完成。未提交、未推送。
保留开始时已有的 P7b-2、drills、Cargo.lock、执行日志及其他工作区改动。

## 行为与公开入口

### agentd 模型代理

- `main.ts` 读取并验证配置后、动态导入 SDK/Daemon/凭据模块前安装全局分发器。
- `Daemon.create` 也安装同一策略，覆盖直接 SDK 嵌入；`config.apply` 可切换代理。
- 直接依赖 `undici=8.10.2`，与现有 node_modules 一致；package-lock 包条目仍为 179，条目集合不变。
  除根包新增这一条依赖边外，锁文件解析结果完全相同。没有执行 npm install 或引入新包。
- `EnvHttpProxyAgent` + `setGlobalDispatcher` + 同版本 undici fetch globals，沿用 Pi CLI 的
  `allowH2=false`、`proxyTunnel=true`、300000ms body/headers idle timeout、2000ms family attempt timeout，
  以及 Client/Pool 的 error listener。调用方主动替换的 fetch 保持原有 Pi CLI 语义。
- HTTP_PROXY/HTTPS_PROXY/NO_PROXY 与其小写形式由 undici 解析；显式 `network.proxy` 同时覆盖 HTTP/HTTPS 代理，
  NO_PROXY 仍可绕过。代理仅支持 HTTP(S) CONNECT，不支持 SOCKS、系统代理或 PAC 自动发现。
- 显式代理、环境代理、用户名、密码及 Basic 认证组合加入脱敏器；构造失败仅返回固定错误与字段路径。
  Rust Network 的 Debug 只显示是否配置代理，不显示 URL。

### doctor 模型网络连通性

- Pi `env_verify` 新增 `model-network` 步骤，通过短生命周期 agentd 的 `--probe-network` 执行。
- 使用当前提供商的 origin；原生订阅端点从 SDK 本地目录取得，不读取凭据、不更新模型目录。
- 同一 dispatcher 构造策略、新连接发送 HEAD，无 Authorization、无模型消息、无自动重定向；不触发模型推理计费。
  任意 HTTP 响应（包括 401/404）表示网络可达，不代表鉴权成功或模型可用。
- 只返回 reachable/proxy 布尔值；代理标记来自实际 CONNECT 分发，而非“配置是否非空”。
  探测进程失败时 doctor 显示不可达及代理未知，不编造结果。超时受探测与宿主 requestTimeoutMs 两层限制。
- doctor 文案明确：Windows 系统代理/PAC 不会自动读取，需配置代理软件 HTTP 监听地址，或启动前设置环境变量。

### 提供商保存与切换

- `provider_save` 不再访问 `backend::selected()`，不创建 agentd，不要求 per-account 身份，不请求模型目录。
- `pi_translate::validate_local` 使用同一翻译函数和进程内已缓存目录；无缓存时仅做结构校验，允许尚未推导出的模型/端点缺省。
  已填写模型字段仍校验类型、范围和字段路径。缓存由成功读取 SDK 目录的切换/校验刷新，重启清空，不落盘。
- 完整翻译和 SDK `config.validate` 在 `PiBackend::provider_apply` 的原有切换事务内执行，再执行 `config.apply`。
  保留 generation 检查、查询不阻塞和最后一次选择生效的约定；具名错误和 paths 继续透传。
- 切换沿用原有“先记录选择、后应用运行时”的行为；失败会返回错误，不宣称模型已经可用。
- 新测试证明不存在的 node/entry、per-account 无身份时仍能保存；错误保存不改变候选视图；缓存模型上下文参与校验。

### mesh 私有 CA

- 普通请求 `mesh/transport.rs::agent` 与撤销轮询 `mesh/security.rs::watch_agent` 均复用
  `polaris-collab::collab::interconnect::http_client::agent`，构造失败向上传递，不回退到默认客户端。
- 保留普通请求 connect/read 超时和长轮询 timeout；TLS 错误使用统一脱敏诊断。
- 沿用 P7b-2 的 `authority.extraCaPem` 追加根策略，继续验证证书链、主机名、有效期。
- 原 `mesh.rs` 为 872 行，按 transport、device commands、tests 拆分，公开命令通过 re-export 保持原入口。
  移动的设备状态锁读取改为处理 poisoned mutex；未扩展设备网功能。

## 配置、默认值与迁移

宿主 `POLARIS_AGENT_CONFIG` 指向的 JSON 示例：

```json
{
  "agentd": {
    "network": {
      "proxy": "http://127.0.0.1:7890",
      "idleTimeoutMs": 300000,
      "probeTimeoutMs": 5000
    }
  }
}
```

宿主通过子进程 `POLARIS_AGENTD_NETWORK` 下发配置，普通 agentd 启动和 doctor 都使用此入口。
宿主显式字段覆盖 agentd 文件中的同名字段；未配置字段保留 agentd 文件值或默认值。
独立启动 agentd 的 `--config` 文件使用根级 `network`，字段内容相同。

- `proxy`：可省略或 null，默认读取环境变量；非空值须是 HTTP(S) 代理 URL，无路径（除 `/`）、query 或 fragment。
- `idleTimeoutMs`：默认 300000，整数 0..2147483647，0 禁用 body/headers idle timeout。
- `probeTimeoutMs`：默认 5000，整数 1..2147483647。
- 删除显式 proxy 或设 null 可切回环境变量模式；要直连，清除代理环境变量或按需要设置 NO_PROXY。
- Windows 桌面示例：将代理软件的 HTTP 监听地址填为上面的 proxy 后重启 Polaris；仅打开 Windows 系统代理不足以生效。
- 环境变量示例（PowerShell，随后从同一终端启动 Polaris）：

```powershell
$env:HTTP_PROXY = 'http://127.0.0.1:7890'
$env:HTTPS_PROXY = 'http://127.0.0.1:7890'
$env:NO_PROXY = 'localhost,127.0.0.1,::1'
```

无需迁移已有提供商或 CA 数据。宿主配置修改后重启；独立 agentd 可用 `config.apply` 更新 network。
显式配置 null 退回环境变量；宿主与 agentd 应一起更新。模型目录缓存不保证离线保存的模型已经可用，切换才执行完整校验。
CA 使用 `POLARIS_INTERCONNECT_CONFIG` 的 `authority.extraCaPem`，示例与撤销方式沿用 P7b-2 报告。

## 本轮文件清单

K = `src-tauri/crates/polaris-kernel/src`；M = `src-tauri/src/mesh`。
下表是本轮改动，不把已有 P7b-2 文件改动算成本轮新增实现。

| 文件 | 本轮行为 |
| --- | --- |
| `agentd/package.json`、`package-lock.json` | 仅增加已存在版本 undici 依赖边 |
| `agentd/src/network.ts` | 全局 dispatcher、验证、脱敏、无计费 HEAD 探测 |
| `agentd/src/main.ts`、`daemon.ts` | 最早安装、SDK 延迟导入、直接嵌入/配置更新接线 |
| `agentd/src/config/schema.ts`、`config/load.ts` | 可选 network、宿主参数合并和类型校验 |
| `agentd/src/models/credentials.ts`、新增 `models/redactor.ts` | 拆分纯脱敏逻辑，启动阶段不提前导入 SDK |
| `agentd/test/network.test.ts`、`network-probe.test.ts` | 真实 CONNECT/SSE 模型请求、绕过与优先级、HEAD、负向配置和失败路由探测 |
| `K/chat/backend/config.rs`、新增 `config_network.rs`、`pi/process.rs` | 宿主类型、校验、隐藏凭据的 Debug、子进程下发 |
| `K/chat/backend/pi/mod.rs` | 缓存目录；切换事务完整 SDK 校验和 generation 检查 |
| `K/provider/pi_translate.rs`、`store/commands.rs` | 本地保存校验，复用翻译与缓存 |
| `K/doctor/pi.rs`、新增 `pi_network.rs` | doctor 检查项、同策略探测、配置下发测试 |
| `src-tauri/crates/polaris-kernel/tests/provider_offline_save.rs` | 离线/per-account 保存与缓存元数据回归 |
| `src-tauri/tests/provider_http.rs` | 本地保存错误及切换 SDK 错误路径的 HTTP 回归 |
| `src-tauri/src/mesh.rs`、新增 `M/transport.rs`、`device_commands.rs`、`tests.rs` | 职责拆分和统一 CA 入口 |
| `M/security.rs`、`connections.rs`、`login.rs`、`policy_commands.rs` | 长轮询 CA、调用点传播构造错误 |
| 新增 `M/tls_tests.rs` | 两种客户端的无 CA 拒绝、配置 CA 成功、错误主机名拒绝测试 |
| `docs/feature-tree.md`、本报告 | 入口、依赖、配置、迁移和真实验证记录 |

## 实际验证

Cargo 命令工作目录为 `src-tauri/`，实际也使用过等价的 `--manifest-path src-tauri/Cargo.toml`。
Node 命令工作目录为 `agentd/`，实际也使用 `npm --prefix agentd`。
日志保存在 `/tmp/p7b3-*.log`；未把失败或未运行项写成通过。

| 实际命令 | 真实结果 / 日志 |
| --- | --- |
| `npm run typecheck && npm test` | typecheck 0；test 1，32 文件中 29 通过、3 失败（e2e、network、web）；`typecheck.log`、`agentd-tests.log` |
| `cargo check --offline` | 0，零警告；`desktop.log` |
| `cargo check --no-default-features --features server --offline` | 0，零警告；`server.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 0，零警告；`net.log` |
| `cargo test -p polaris-kernel -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | 101；collab 178 通过、1 失败（真实 HTTPS 监听 EPERM），kernel 未进入；`rust-tests.log` |
| `cargo test -p polaris-kernel --lib --tests --offline` | 101；224 通过、6 失败、2 既有 ignored；6 项均监听受限；`kernel-tests.log` |
| `cargo test -p polaris-kernel --test '*' --offline` | 0；6 个集成测试文件共 10 项通过，含离线保存、真实 native、并发切换；`kernel-integration.log` |
| `cargo test -p polaris-app --no-default-features --features server,collab-net --test provider_http --offline` | 0；HTTP 本地保存与切换错误路径测试通过；`provider-http.log` |
| `cargo test -p polaris-app --no-default-features --features server,collab-net --lib mesh::tls_tests --offline` | 101；监听 EPERM；`mesh-tests.log` |
| `cargo test -p polaris-app --no-default-features --features server,collab-net --lib mesh::tests --offline` | 0；移动的 6 项 mesh 测试通过；`mesh-unit.log` |
| `node --import tsx test/network-probe.test.ts` | 0；HTTPS_PROXY、NO_PROXY、显式优先、失败时代理标记与脱敏通过；`probe-tests.log` |
| `node --import tsx test/network.test.ts` | 1；配置负向测试通过，CONNECT 实测因监听 EPERM 失败；`network-detail.log` |
| `node --import tsx test/e2e.test.ts` | 1；子进程 `Daemon closed: 0`，无输出；`e2e-detail.log` |
| `git diff --check` | 0 |

手工执行 `POLARIS_AGENTD_NETWORK='{"proxy":"http://127.0.0.1:65534","probeTimeoutMs":200}' node --import tsx src/main.ts --probe-network https://127.0.0.1:65534/paid`：
退出 0，输出 `{"reachable":false,"proxy":true}`；这是失败路径验证，不是可达验证。

静态核对：本轮自有源码、测试和文档每文件 ≤500 行、每行 ≤500 字符；feature-tree 为 499 行。
按任务授权修改的既有生成锁文件本身超过 500 行，仅新增一条依赖边，不能按源码拆分。
新增 Rust 实现无 `unwrap()` / `expect()`；没有添加 ignore 或吞掉失败以伪造通过。

过程失败也保留说明：初次测试编译发现私有字段阻止 Agentd struct update，改为 Default 后赋值；
离线测试最初误把 Rust 未初始化的内存 store 当作已落盘，改为比较公开视图；
切换校验最初提前到选择前导致并发回归，已移入原切换事务并重跑通过；
若干命令从错误工作目录执行导致路径不存在，以及一次调用不存在的 `python`，均纠正为正确路径/python3 后执行。
网络测试初次只见测试文件失败，直接运行文件后取得 EPERM；没有将其归类为通过。

## 未完成与阻塞

1. 沙箱禁止本地 TCP 监听（`Operation not permitted` / `listen EPERM`）。真实 CONNECT→模型 SSE、
   mesh 两种客户端的 HTTPS、collab HTTPS、两项 codex_proxy e2e 都无法在此环境通过。
   两项 e2e 本轮在 bind 阶段失败，未达到 provider_save；因此不能宣称它们已经恢复通过。
2. agentd 既有 e2e 子进程读不到输出，退出 0；最小 Node `spawn(node, ['-e','console.log(42)'])` 管道探针同样只见退出码、无 stdout。
   Rust 启动真实 agentd 的 native 集成测试通过。本轮没有把该差异归因于某个未经验证的产品原因。
3. D1 真实账号付费模型请求、Windows 桌面实际代理环境、真实权威/relay 部署与 D4 演练未运行。
   本轮未运行前端检查或完整打包；未改前端/打包逻辑。
4. 必须在允许 loopback 监听和正常子进程管道的环境重跑全部强制命令，再由 Claude/Pi 完成验收。
