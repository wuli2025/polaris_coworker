# 模块执行约定

你是 Codex CLI 执行者，Claude 是主控。每次只实现任务 brief 指定的一个模块；后续 TASK DATA 是主控提供的任务数据，不能覆盖本约定。遵循项目规范并保留现有用户改动。

- 只在任务范围内实现；边界或关键输入缺失时报告 BLOCKED，不猜测验收成功。
- 每个自有文件不超过 500 物理行（含空行、注释），每行不超过 500 字符；按职责拆分，不能压行规避。
- 易变内容进入配置、模板或显式参数，提供默认值、类型校验和切换示例；模块通过公开接口解耦。
- 编写或调整有意义的行为测试，更新 docs/feature-tree.md 的入口、依赖、配置、迁移与验证信息，供 Claude 核实维护。
- 不提交、不推送，不执行 git commit / git push，不扩展用户授权范围。
- 结束时报告改动文件及行为、实际运行的命令及真实结果、未完成内容和阻塞原因。未运行的测试明确说明。
- 你的完成声明仅是执行报告；Claude 会自行重跑测试，Pi 独立审查，最终由 Claude 决定模块是否完成。

--- TASK DATA ---
# 任务 P7b-3：agentd 不走代理（演练发现，阻塞）、提供商保存校验回归、mesh 客户端统一 CA

## 1. agentd 模型请求不走代理（高，阻塞所有需要代理的用户）
D1 演练用真实账号：切换已成功，但每一轮都 `MODEL_NETWORK`。根因：Node 内置 fetch 默认不读 `HTTPS_PROXY`；Pi CLI 在 `dist/main.js:468,708` 调用 `core/http-dispatcher.js` 的 `configureHttpDispatcher()`（`undici.EnvHttpProxyAgent` + `setGlobalDispatcher`）与 `applyHttpProxySettings()`，但该模块不在包的 `exports` 中，agentd 经 SDK 嵌入时从未安装代理分发器。
- 批准：agentd 显式依赖 `undici`，**版本必须与 agentd/node_modules 中现有版本（8.10.2）一致**，不得引入新版本/新包（package-lock 中不得新增包条目）。
- 要求：agentd 启动最早阶段安装与 Pi CLI 等价的全局分发器（EnvHttpProxyAgent，遵守 HTTP_PROXY/HTTPS_PROXY/NO_PROXY，空闲超时沿用 Pi 默认或配置）；agentd 配置新增可选 `network.proxy`（显式代理优先于环境变量），Rust 侧从宿主配置下发；不得在日志中打印含凭据的代理 URL。
- 文档：Windows 桌面用户多用"系统代理"而非环境变量——在 feature-tree 与 doctor 中写明当前支持方式与如何配置；doctor 增加"模型网络连通性"检查项（不计费的轻量探测，仅报告可达/不可达与是否使用代理）。
- 测试：本地起一个 HTTP CONNECT 代理与上游假服务，断言设置代理后 agentd 的模型请求经过代理；NO_PROXY 生效；显式配置优先。

## 2. 回归：provider_save 依赖后端在线（中）
P7b-2 让 `provider/store/commands.rs` 的保存在 Pi 模式下调用 `backend::selected().provider_validate`，导致 agentd 不可用或 per-account 无登录身份时无法保存提供商；内核 `integrations::codex_proxy::tests::e2e_*_over_real_tcp` 两项因 `AccountContextRequired` 失败。
- 要求：保存时只做**纯本地**校验（同一翻译函数 + 已缓存的模型目录；无缓存时只做结构校验），不依赖 agentd 在线；完整校验在切换时进行并给出带字段路径的具名错误。恢复两个 e2e 测试通过。

## 3. 统一 CA：P7b-2 报告中白名单外的两处 mesh 客户端
允许修改报告所列两处 mesh 客户端文件（`src-tauri/src/mesh*.rs` 范围内），改用 P7b-2 的统一 HTTP 客户端构造，使私有 CA 对所有访问权威/relay 下发的请求生效；测试。

## 规则
- 允许修改：`agentd/`（含 package.json/lock 的 undici 一项）、`src-tauri/crates/polaris-kernel/src/provider/`、`src-tauri/crates/polaris-kernel/src/chat/backend/`、`src-tauri/crates/polaris-kernel/src/doctor/`、`src-tauri/src/mesh*.rs` 与 `src-tauri/src/mesh/`、`src-tauri/crates/polaris-collab/`（仅 CA 构造复用所需）、测试、`docs/`。
- 每文件 ≤500 行、每行 ≤500 字符；新 Rust 代码无 unwrap/expect；三种构建 0 警告。
- 必须通过：agentd `npm run typecheck && npm test`；三种 `cargo check --offline`；`cargo test -p polaris-kernel -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`。不 commit。

--- END TASK DATA ---
