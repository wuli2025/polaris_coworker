# P7b-R1：P7b 系列独立审查意见修复

审查记录：`docs/reviews/P7b-diff-r1`、`P7b23-diff-r1`、`P7b23-splits-r1`。Claude 已核实以下三项属实，请逐项修复并补测试。

## 1. 插话/排队入口未按后端能力控制（中）
`src/components/chat/LiveInputActions.vue` 与 `ChatComposer.vue` 只看 `sending`。`AgentBackend::live_input` 默认返回 `ChatLiveInputUnsupported`，只有 Pi 后端支持。

- 由后端声明能力（如 `liveInput: bool`，放在现有后端状态/能力查询返回中），前端据此隐藏入口或禁用并提示原因。不要在前端按后端名称硬编码。
- `ChatComposer.vue` 已超过 500 行（既有技术债）。本次**不要再往里加逻辑**：能力判断放在 `LiveInputActions.vue` 或 `liveInput.ts` 中。若必须改 ChatComposer，只允许改一行 props 传递。
- 测试：能力为 false 时不渲染/禁用；为 true 时可用（vitest）。Rust 侧测试：Pi 报告支持，ClaudeCli 报告不支持。

## 2. agentd 覆盖调用方预装的 fetch（P2）
`agentd/src/network.ts` 在模块加载时记录 `originalFetch`；`configureNetwork` 首次调用时只要 fetch 未变就执行 `undici.install()`。
嵌入方若在导入前安装了自定义 fetch，会被当作"原始"而覆盖。

- 改为：`configureNetwork` 只负责 `setGlobalDispatcher`；替换 fetch globals 只能由进程入口 `main.ts` 显式选择（例如调用参数 `{ installFetch: true }` 或单独的导出函数）。`Daemon.create` / `config.apply` 不再安装 fetch。
- 必须保证独立进程（main.ts）下，模型请求仍经过代理。现有 network 测试需继续通过。
- 新测试：先安装自定义 fetch，再导入并创建 daemon、再 `config.apply` 改代理，自定义 fetch 仍被保留。

## 3. 认领停用账号仍成功（中，既有缺陷，安全相关）
`src-tauri/crates/polaris-collab/src/collab/authority/membership.rs` 中，邮箱匹配分支和"唯一无 uid owner"认领分支都没有读取 `disabled`。已有 uid 分支则会拒绝停用账号。

- 在同一事务内读取目标行的 `disabled`。停用时返回现有的停用错误文案（与 uid 分支一致），且不修改任何身份字段。
- 回归测试：停用账号经邮箱认领被拒且绑定不变；停用的无 uid owner 认领被拒。
- 顺带核查同文件及调用链（会话签发）中是否还有其他未检查 `disabled` 的路径。只列入报告，不扩大改动。

## 约束
- 单文件 ≤500 行、单行 ≤500 字符；不新增依赖；不改 `mobile/`；不 commit。
- 验证：`cargo check` 三种形态 0 警告；`cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests` 中相关测试；agentd `npm run typecheck && npm test`；`npx vitest run`、`npx vue-tsc --noEmit`。沙箱内跑不了的列出，由 Claude 重跑。
- 报告写入 `docs/tasks/P7b-R1-execution-report.md`。
