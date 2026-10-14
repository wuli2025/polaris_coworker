# 任务 P5c：前端适配 Pi 后端（环境诊断、提供商、用量、上下文、专家、沙箱、文案）

后端已全部就位，本任务只做前端，让界面如实反映 Pi 后端，不再宣称 Claude CLI 的能力或要求。先读：`docs/pi-migration/call-sites.md`"前端与误命中裁定"一节，以及下列各阶段留给前端的说明：`docs/pi-migration/p5a-runtime.md`"P5c 前端配合"、`docs/pi-migration/p4-result.md`"前端配合与未完成项"、`docs/pi-migration/p3b-result.md`、`docs/pi-migration/p5b-indirect-callers.md`。

## 要求
1. **环境诊断（`EnvDoctor.vue`、`App.vue` 启动提示）**：读取 `EnvReport.agentRuntime`；显示 Node、agentd 入口、ping、协议版本、Pi 版本、shell 就绪与实际来源、凭据"已配置/未配置"（绝不请求或显示明文）；按 backend 与 `legacyActions` 显示/隐藏旧 Claude 安装、更新、PATH 修复按钮；Pi 校验不调用模型，`deep=false` 如实说明"未做计费推理"；不再承诺后台安装 Claude。浏览器预览模式的 mock 标注为预览，不假报就绪。
2. **提供商（`ProviderDock.vue`、`stores/providers.ts`、`AddProviderModal.vue`）**：读取 `agent_backend_status`；Pi 后端下"联动/代理"开关隐藏或标注无效（后端已返回 Unsupported）；新增提供商支持模型清单与协议选项（可先在现有 JSON 编辑区使用 `settingsConfig.models`，给出示例）；订阅登录标注 Pi 凭据位置与"首次迁移需重新登录"，Codex 不再提示必须安装 CLI。
3. **用量（`UsageBoard.vue`）**：来源说明改为"Pi 权威记录 + Claude 历史"；未标价模型的成本显示"未知"而不是 0/免费。
4. **上下文文件（`ClaudeMdPanel.vue`、`Sidebar.vue`、`stores/app.ts`）**：改为兼容的上下文编辑器：项目有 `AGENTS.md` 时编辑它，否则编辑 `CLAUDE.md`，界面写明 Pi 的读取优先级（AGENTS.md 优先）；不自动改名用户文件。
5. **专家（`ExpertCenter.vue`、`ExpertTeam.vue`）**：Pi 后端下"应用/导出人格"改为请求级装载的说明与入口（P3b 已实现请求级传递），不再引导写 `CLAUDE.md`；专家团默认关闭的状态与开关可见。
6. **沙箱（`SandboxStatus.vue` 等）**：文案反映真实路由状态（只挂项目与只读知识库、路由前安全校验），不再出现"Claude 沙箱就绪"一类表述；服务端形态明确"暂不支持沙箱"。
7. **对话与其他文案（`ChatComposer.vue`、`stores/chat.ts`、`FeishuSettings.vue`、`InterconnectView.vue` 执行名单提示等）**：去掉"需要 Claude CLI"类警告，运行时说明改为 Pi；样式与事件消费不变。
8. **类型**：`src/tauri.ts` 与后端新增字段对齐（`agentRuntime`、`agent_backend_status` 等），浏览器 mock 同步。

## 规则
- 只改前端（`src/`、`tests/frontend/`）与 `docs/`；后端如发现缺字段，在结果中列出，不改后端。
- 既有超长文件（例如 `ChatComposer.vue` 2556+、`src/tauri.ts` 2631+、`EnvDoctor.vue`、`ProviderDock.vue`、`UsageBoard.vue` 等，以实际行数为准）：新增逻辑放新组件/新文件（≤500 行），旧文件只做引用、文案替换与必要的少量接线，每个旧文件**净增 ≤20 行**；结果中列出每个旧文件的净增行数。
- 文案不硬编码在多处：集中到一个文案/常量模块（如已有 i18n 或文案文件则沿用）。
- 不新增 npm 依赖；新文件 ≤500 行、每行 ≤500 字符。
- 测试：为新增的展示逻辑（例如诊断项映射、成本"未知"判定、上下文文件选择、联动开关可见性）写 vitest 用例。
- 必须通过：`npx vue-tsc --noEmit`；`npx vitest run`；`npx vite build`。不 commit。
- 结束时输出：每项完成情况、改动文件与行数（旧文件净增）、命令结果、发现的后端缺口。
