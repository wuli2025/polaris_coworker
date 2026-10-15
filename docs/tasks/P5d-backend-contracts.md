# 任务 P5d：补齐 P5c 所需的三个后端契约，并完成前端接线

背景见 `docs/pi-migration/p5c-frontend.md`"后端契约缺口"。本任务后端 + 前端接线一起做，完成后 P5c/P5d 一并送审。

## 1. 上下文文件选择与读写
- `polaris-kernel/src/claude_md.rs`（及调用方）新增与后端无关的上下文文件契约：按项目**实际工作目录**选择 `AGENTS.md`（存在则用）否则 `CLAUDE.md`；提供 metadata（选中的文件名、两者是否存在、路径相对项目根）、read、write（写入**同一个已选择文件**；两者都不存在时按配置决定创建哪个，默认 `AGENTS.md`）。
- 明确区分"不存在"与"读取失败"（具名错误），不得把任意 I/O 错误当成不存在。
- 路径限定在项目目录内（规范化、拒绝 `..`/符号链接逃逸），复用现有安全工具。
- 旧的 `project_claude_md_path`/`kb_claude_md_path` 保留给旧后端与现有调用方，行为不变。
- 前端：`ClaudeMdPanel.vue` 改用新契约（新逻辑放新文件，旧文件净增 ≤20 行），显示选中的文件与 Pi 的优先级说明；解除 P5c 的临时禁用。
- 测试：只有 CLAUDE.md、只有 AGENTS.md、两者都有、都没有、读取失败（权限）五种；写入不会切换文件；越界路径拒绝。

## 2. 用量价格完整性
- `provider/usage.rs`：聚合结果增加价格完整性信息：每个模型（或来源）是否有价格、未标价请求计数、`hasUnknownCosts`；保留已标价部分的总成本。
- 前端 `UsageBoard.vue`：已标价总成本照常显示，存在未标价部分时标注"另有 N 次请求未标价"，不把未知当 0。
- 测试：全标价、部分未标价、全未标价三种。

## 3. 专家团开关
- 公开专家团实际 `enabled` 状态读取与受校验的设置接口（Tauri 与服务端壳两条调用路径都要有），通过 agentd `config.apply` 下发，且持久化到宿主配置；设置后新会话生效、空闲会话按现有 reopen 机制生效。
- 前端 `ExpertCenter.vue`/`ExpertTeam.vue`：显示真实状态与开关（新逻辑放新文件，旧文件净增 ≤20 行）。
- 测试：读取默认关闭、开启后新会话的系统提示含专家段、关闭后不含；非法输入拒绝。

## 规则
- 允许修改：`polaris-kernel/src/claude_md.rs` 及其调用方、`polaris-kernel/src/provider/usage.rs` 及其聚合调用方、专家开关所需的 `src-tauri/src/expert*`/命令注册/`apihub` 分发、`polaris-kernel/src/chat/backend/`（仅下发专家配置）、`agentd/`（仅如需暴露专家配置读取）、对应前端组件/store/类型与新文件、测试、`docs/`。
- 无新依赖；新文件 ≤500 行、每行 ≤500 字符；既有超长文件按上述净增限制；新 Rust 代码无 unwrap/expect；编译警告保持 0。
- 必须通过：agentd `npm run typecheck && npm test`；两壳 `cargo check --offline`（0 警告）；`cargo test -p polaris-kernel --lib --tests --offline`（0 失败，沙箱 TCP 项除外并列出）；`npx vue-tsc --noEmit`；`npx vitest run`；`npx vite build`。不 commit。
