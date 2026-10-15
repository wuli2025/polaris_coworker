# P5c 前端 Pi 适配执行报告

后续进展：P5d 已接通上下文编辑、专家实际开关和用量完整性展示；本报告保留 P5c 当时的结果。
当前实现、剩余价格来源缺口及实测结果见 [P5d 执行报告](p5d-contracts.md)，不能以本报告中的历史禁用状态判断当前代码。

状态：**BLOCKED（部分实现）**。仅为执行报告；Claude 自行重跑、Pi 独立审查，最终由 Claude 判断模块完成。
本次仅修改 src/、tests/frontend/、docs/，未修改后端、未新增 npm 依赖、未 commit/push。

## 逐项结果

| 要求 | 执行结果 |
| --- | --- |
| 1 环境诊断 | 已接入 agentRuntime；展示 Node 版本/路径、agentd 入口存在性、ping、协议/Pi/agentd 版本、shellReady 与 env_verify 的实际 shell 来源、凭据配置布尔值。就绪须同时满足 report.ready/shellReady。Pi 校验传 deep=false，提示未做计费推理；旧安装/更新/PATH/Node 动作同时要求 backend=claude-cli 与 legacyActions。服务端也自动取快速校验。App 不再承诺后台安装 Claude。 |
| 2 提供商 | store 读取 agent_backend_status；Pi 隐藏并阻止联动/代理 setter 与代理查询。自定义表单支持五种协议、settingsConfig.models 示例与类型校验；Pi 不自动预填虚构模型。订阅提示共享凭据位置与首次迁移重登；沿用 P4 登录接口，不要求 Codex CLI。 |
| 3 用量 | 来源为「Pi 权威记录 + Claude 历史」；移除没有后端依据的来源筛选。总成本显示未知，正的已记录金额单列，0/负数/非有限值不当免费。供应商坞成本卡同步。**精确价格完整性判断受阻**，见下方契约缺口。 |
| 4 上下文 | 标题与说明改为兼容上下文文件，明确 AGENTS.md 优先、不自动改名。**自动选择/编辑 BLOCKED**：现有接口固定 CLAUDE.md；Pi 暂禁用旧写入面板，避免错写被遮蔽的文件；旧后端保留编辑器。Sidebar 的 Pi 人格入口转到请求级专家中心；保留 claude_md 路由键。 |
| 5 专家 | Pi 应用入口说明请求级装载；导出按钮改为选择专家并进入对话，复用 P3b expertSelection，不下载/写入 CLAUDE.md。默认关闭、当前状态未知及禁用开关可见，展示配置示例。**真实开关读写 BLOCKED**：后端无公开契约。 |
| 6 沙箱 | 文案说明项目 RW + KB RO、每次路由前校验、容器运行不代表本轮已执行。env_check.imageManaged 区分服务端，显示暂不支持并禁用操作；浏览器预览也禁用。Pi 隐藏未接通的 E2B 配置，命令示例改 pwd。 |
| 7 其他文案 | ChatComposer、chat store、飞书、互联执行名单、humanizeError 改为 Pi/当前 Agent 后端；保留样式规则与事件消费。Claude 品牌订阅、兼容分支安装能力及无关审查注释保留。 |
| 8 类型/mock | 新增完整 AgentBackendStatus 与 tauri.agentRuntime.status；VerifyStep 对齐 Pi key。现有 AgentRuntimeReport 已匹配后端，无需虚构字段。预览 Node/shell/凭据/握手都不就绪，登录发起显式拒绝，轮询不返回成功，旧代理 setter 拒绝。 |

## 公开接口、默认值与切换

- 集中文案：`src/features/runtime/copy.ts`；无新增 npm 依赖。
- 公开纯函数：`legacyDoctorActions`、`legacyProviderActions`、`runtimeRows`、`usageCost`、`recordedCost`、`validateProviderConfig`、`contextEditingAvailable`。
- `useBackendStatus` 查询失败时状态归 null，旧动作默认关闭；provider store 同样先清空旧状态再查询，失败不保留旧开关权限。
- `usageCost(cost, pricesComplete=false)` 默认返回未知；当前后端不提供完整性标志，调用方不传 true。已记录金额不等于总成本或最终账单。
- 后端选择沿用 `POLARIS_AGENT_BACKEND=pi`（默认）；兼容切换为 `POLARIS_AGENT_BACKEND=claude-cli` 并重启宿主。前端读取实际状态，不新增独立后端配置。
- 协议默认空串=Anthropic Messages；选 openai / responses / chatgpt / google 分别对应后端公开映射。JSON 输入区就是 settingsConfig，**不套第二层 settingsConfig 对象**。
- models 未设时保留后端 env → 内置目录回退；显式 models 必须非空数组，条目为非空字符串或带 id 的对象。
- 对象可选 name/string、reasoning/boolean、contextWindow/maxTokens/正安全整数、input/非空 text/image 数组、cost/四项有限非负价格；env 必须对象。没有假定零价表示免费。

示例（在表单选择 OpenAI Completions；填写真实端点、密钥、模型 ID）：

```json
{
  "env": {"ANTHROPIC_BASE_URL": "https://gateway.example/v1"},
  "models": ["model-a", "model-b"]
}
```

需要价格元数据时使用完整模型对象，例如：

```json
{
  "env": {"ANTHROPIC_BASE_URL": "https://gateway.example/v1"},
  "models": [{
    "id": "model-a", "reasoning": false, "input": ["text"],
    "contextWindow": 128000, "maxTokens": 8192,
    "cost": {"input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 1}
  }]
}
```

这些示例值仅作结构说明，不宣称真实模型或价格；不要把占位模型直接用于推理。
凭据仍由 P4 写入 `~/Polaris/data/providers.json`；未复制旧 CLI OAuth 文件，首次迁移重新授权。
专家后端默认 `experts.enabled=false`，显式配置 `{"experts":{"enabled":true,"prompt":""}}` 并重开会话；恢复关闭传 false。
前端没有写此配置的权限入口，禁用复选框不会伪装为已修改后端。

## 后端契约缺口（未越界修改）

1. **上下文选择与读写**：`polaris-kernel/src/claude_md.rs` 的 project_claude_md_path/kb_claude_md_path 固定拼接 CLAUDE.md；list/read/write 没有文件名、AGENTS.md 存在性、项目实际工作目录或选择结果。
   `artifact_read/write` 要求已存在文件，缺失/无法访问共用字符串错误，不能安全判断「AGENTS.md 不存在」并切换；不可把任意读取错误当不存在。
   需要后端公开按实际项目目录选择 AGENTS.md→CLAUDE.md 的 metadata/read/write 契约，并保持写入同一已选择文件。
   本轮没有编造新 IPC，也没有用本地文件插件绕过服务端访问边界。**未编写/未声称通过上下文文件实际选择测试**；当前测试仅验证阻止 Pi 误写旧固定接口。
2. **价格完整性**：`provider/usage.rs` 的 TokenBucket/DailyUsage 只有聚合数值 cost，没有来源/模型价格状态、未标价计数或 hasUnknownCosts；正的混合成本也可能缺项，不能从 cost>0 推断完整。
   需后端提供完整性或逐模型明细，才能保留已标价总成本并只把未标价模型标未知。当前 UI 保守显示未知，不把此降级算作精确模型成本适配完成。
3. **专家开关**：agentd 的 sessions/registry.ts 使用启动配置 experts.enabled，但 agent_backend_status 只返回 supervisor 字段，前端 IPC 未提供专家配置读取/修改。
   需要公开实际 enabled 状态与受校验 setter（或明确只提供部署配置入口）；当前只展示默认/未知与示例，不假报启用。

shell 实际来源已在 env_verify.steps[key=shell].detail 提供，因此没有为 shell 虚构新后端字段。
agentd 入口仅显示后端 entryExists；后端未返回入口完整路径，本轮未猜测该路径。

## 验证

执行目录：`/home/mi/work/polaris-pi`。本轮新增 21 项用例，包含 Vue SSR 输出、纯映射、真实 preview invoke、Pinia store 行为。
既有 P3b 请求级专家测试继续验证 expertSelection 进入 chat_send 且临时 CLAUDE.md/AGENTS.md 不变。

| 实际命令 | 真实结果 |
| --- | --- |
| `npx vue-tsc --noEmit` | 最终退出 0；首次失败为 ExpertTeam 的 emit 联合事件名不匹配重载，改成明确分支后通过 |
| `npx vitest run` | 最终退出 0，13 文件 / 53 用例通过；早期一轮为 12 文件 / 52 用例，补 preview 测试后增至 53 |
| `npx vite build` | 最终退出 0；存在大于 500 kB chunk 警告，不影响构建 |
| `git diff --check` | 退出 0 |
| Python 物理行数/行长检查 | 新文件均 ≤500 行、每行 ≤500 字符；旧前端文件净增均 ≤20；结果如下 |
| `git status --short`、`git diff --stat/--numstat`、`rg`/`sed`/`cat` | 用于核对范围、字段和改动；个别探索路径不存在后改用 rg 定位实际 crate 文件，无后端修改 |

未运行：浏览器交互点击验收、桌面真机/Windows/macOS、真实 OAuth、模型推理、真实 Docker、后端 Rust/agentd 全量测试。
没有把单测/SSR 当成上述实机或计费端到端验证。
构建和测试日志位于 `/tmp/p5c-{types,vitest,build}-final.log`；中间日志保留 `/tmp/p5c-{types,vitest,build}.log`。

## 文件与物理行数

下表是本次修改的既有前端文件；前后数按物理行计算（含空行/注释）。既有超过 500 行的文件只做接线与文案修改，未新增长文件。

| 文件 | 原行数 | 当前行数 | 净增 |
| --- | ---: | ---: | ---: |
| `src/App.vue` | 585 | 585 | +0 |
| `src/components/AddProviderModal.vue` | 844 | 839 | -5 |
| `src/components/ClaudeMdPanel.vue` | 783 | 792 | +9 |
| `src/components/EnvDoctor.vue` | 1143 | 1149 | +6 |
| `src/components/ExpertCenter.vue` | 233 | 236 | +3 |
| `src/components/ExpertTeam.vue` | 394 | 402 | +8 |
| `src/components/FeishuSettings.vue` | 687 | 687 | +0 |
| `src/components/ProviderDock.vue` | 1410 | 1404 | -6 |
| `src/components/Sidebar.vue` | 1349 | 1354 | +5 |
| `src/components/UsageBoard.vue` | 660 | 657 | -3 |
| `src/components/chat/ChatComposer.vue` | 2559 | 2560 | +1 |
| `src/features/interconnect/InterconnectView.vue` | 2756 | 2757 | +1 |
| `src/features/sandbox/components/SandboxStatus.vue` | 505 | 515 | +10 |
| `src/lib/humanizeError.ts` | 44 | 45 | +1 |
| `src/stores/app.ts` | 520 | 520 | +0 |
| `src/stores/chat.ts` | 483 | 483 | +0 |
| `src/stores/providers.ts` | 325 | 337 | +12 |
| `src/tauri.ts` | 2638 | 2632 | -6 |

新增文件：

| 文件 | 行数 |
| --- | ---: |
| `src/features/runtime/components/ContextRuntimeNotice.vue` | 12 |
| `src/features/runtime/components/ExpertRuntimeNotice.vue` | 18 |
| `src/features/runtime/components/ProviderModelHelp.vue` | 14 |
| `src/features/runtime/components/RuntimeDiagnostics.vue` | 26 |
| `src/features/runtime/contextPolicy.ts` | 4 |
| `src/features/runtime/copy.ts` | 47 |
| `src/features/runtime/presentation.ts` | 32 |
| `src/features/runtime/providerConfig.ts` | 42 |
| `src/features/runtime/useBackendStatus.ts` | 22 |
| `src/types/agentRuntime.ts` | 17 |
| `tests/frontend/runtimePresentation.test.ts` | 83 |
| `tests/frontend/runtimePreview.test.ts` | 21 |
| `tests/frontend/runtimeProviders.test.ts` | 41 |

文档：本报告与 docs/feature-tree.md 的 P5c 小节；feature-tree 保留并行执行者新增的 R7 小节，未覆盖其他任务内容。
新增模块均按文案、类型、纯映射/校验、状态查询与展示组件拆分；保留既有用户改动与原文件换行方式。
文档物理行数：本报告 134 行；docs/feature-tree.md 438 行，本轮 P5c 小节净增 16 行（另有并行 R7 的 8 行）。两者每行均不超过 500 字符。
