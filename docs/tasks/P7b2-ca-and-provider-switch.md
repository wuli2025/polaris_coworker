# 任务 P7b-2：自定义 CA、订阅提供商切换失败（演练发现）

## 1. 切换到订阅提供商失败（D1 演练复现，阻塞演练）
复现（服务端 HTTP `/api/invoke`）：`provider_save {input:{id:"codex", name:"Drill subscription", settingsConfig:{models:["gpt-6-astra"]}}}` → `provider_switch {id:"codex"}` 返回 500：`INVALID_CONFIG: 配置字段校验失败。`。凭据为 Pi 的 `openai-codex` OAuth。`gpt-6-astra` 是用户 Codex CLI 实际使用、且 Pi 订阅可用的模型，但可能不在 Pi 1.0.4 内置目录中。
要求：
- 先把 agentd 返回的字段路径（`WireError.paths`，agentd `config/validate.ts:15` 已提供）一路传到调用方错误信息，用它定位哪些字段非法。
- 修复根因：对订阅/原生提供商（codex→openai-codex、claude-official→anthropic）使用 SDK 原生 provider 与鉴权，不应要求自定义 baseUrl 等；目录里没有的模型 ID 应能以安全默认值（上下文窗口、最大输出、成本标记为未知）注册，或给出明确的具名错误——不能是笼统的配置校验失败。成本未知时与 P5d 的"价格可用性"语义一致（不当作 0）。
- `provider_save` 时就做与切换相同的翻译与校验，保存不可能成功切换的配置时立即返回具名错误（含字段路径）。
- 测试：codex + 目录外模型 ID 可保存并切换；非法配置在保存时即报错且含路径；错误信息传到 HTTP/Tauri 调用方。

## 2. 自定义 CA（私有部署）——依赖已批准
- 批准：`polaris-collab` 显式声明 `webpki-roots`，**版本必须与 Cargo.lock 中 ureq 2.12.1 所用版本一致**（不得引入新版本或新包，`cargo build --offline` 必须成功，Cargo.lock 中不得出现新包条目）。
- 要求同 `docs/tasks/P7b-product-gaps.md` 第 2 节：`authority.extraCaPem` 可选配置；以 webpki 默认根为基础**追加**私有 CA 构造 rustls ClientConfig，交给 ureq；无配置时行为与现在完全一致；访问权威/relay 下发的所有 HTTP 客户端统一使用该构造；不出现任何跳过校验路径；证书错误明确诊断。
- 测试：本地用测试 CA 签发证书的 HTTPS 服务（测试内生成证书，可用已有依赖），配置 CA 后可连、未配置时拒绝。

## 规则
- 允许修改：`src-tauri/crates/polaris-kernel/src/provider/`、`src-tauri/crates/polaris-kernel/src/chat/backend/`、`agentd/src/`（仅当原生 provider 注册需要）、`src-tauri/crates/polaris-collab/`（含 Cargo.toml 的上述一项依赖）、`src-tauri/Cargo.lock`（只允许新增依赖边，不允许新增包条目）、`drills/`、测试、`docs/`。
- 每文件 ≤500 行、每行 ≤500 字符；新 Rust 代码无 unwrap/expect；三种构建 0 警告。
- 必须通过：三种 `cargo check --offline`；`cargo test -p polaris-kernel -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`；agentd `npm run typecheck && npm test`；`npx vue-tsc --noEmit`。不 commit。
