# 审查 brief：P5c 前端适配 + P5d/P5d-2 后端契约（含测试）

两包：`P5cd-frontend.patch`（前端组件、store、类型、新文件与 vitest 测试）、`P5cd-backend.patch`（上下文文件契约、用量价格完整性、专家开关、agentd 用量价格标记及测试）。

## 验收条件
1. 环境诊断读 `agentRuntime`：Node/入口/ping/协议/Pi 版本/shell 来源；凭据只显示"已配置/未配置"，绝不请求或显示明文；按后端与 legacyActions 显隐旧 Claude 按钮；浏览器预览 mock 不假报就绪。
2. 提供商：Pi 后端下联动/代理开关标注无效；Codex 不再要求安装 CLI。
3. 用量：来源"Pi 权威 + Claude 历史"；已标价金额照常显示，未标价请求单独计数，未知不当 0；价格可用性依据模型定义而非请求费用；旧记录缺字段按未知。
4. 上下文：按项目实际工作目录 AGENTS.md 优先否则 CLAUDE.md；写入锁定已选文件；"不存在"与"读取失败"区分；路径限定项目内；旧接口行为不变。
5. 专家团：真实开关可读可写（Tauri 与服务端两条路径）、持久化、下发 agentd；默认关闭。
6. 既有超长前端文件净增 ≤20 行，新逻辑在新文件。

## Claude 实测（沙箱外）
`npx vue-tsc --noEmit` 通过；`npx vitest run` 61/61；`npx vite build` 通过（大 chunk 警告为基线既有）；agentd typecheck 通过、`npm test` 122/122；两壳 `cargo check` 0 警告；`cargo test -p polaris-kernel --lib --tests` 213 通过 0 失败；fable 105 / 0 失败。

## 请重点审查
凭据是否可能经任何前端路径显示或进入日志；上下文文件读写能否越出项目目录（符号链接、`..`、项目目录本身为链接）、两文件同时存在时的竞态；专家开关的服务端路径是否有鉴权（谁能改）；用量完整性在混合新旧记录时是否正确；前端是否仍有"需要 Claude CLI"之类过时表述或假就绪。
