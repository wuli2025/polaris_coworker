# 任务 P0：摸底与功能树（只读分析 + 写文档，不改业务代码）

你在 Polaris 仓库（Tauri 2 + Vue 3 + Rust）的 `refactor/pi-agent` 分支上工作。总体规划见 `docs/plans/2026-10-14-pi-refactor.md`，先读它。

## 目标
为"把 Agent 底座从 Claude Code CLI / Codex CLI 换成 Pi SDK"准备一份准确的改造清单和功能树。本任务**只新增文档**，不修改任何 `.rs/.ts/.vue` 文件。

## 产出
1. `docs/pi-migration/call-sites.md`：所有依赖 `claude` CLI、`codex` CLI、Claude/Codex 登录、`CLAUDE.md`、stream-json 格式的位置。每条写：文件:行号、做什么、属于哪个模块、迁移到 Pi 后的处理（替换/删除/保留为旧后端/无关）。区分真依赖与误命中（例如调用 ffmpeg 的 `Command::new`）。
2. `docs/pi-migration/agent-events.md`：Rust 现在发给前端的 Agent 事件清单（事件名、载荷字段、发出位置、前端消费位置）。这是 Pi 后端必须保持兼容的契约。
3. `docs/pi-migration/oversized-files.md`：自有代码中超过 500 行的文件清单（排除 node_modules、target、dist、生成文件），标注本次重构会触及哪些。
4. `docs/feature-tree.md`：功能树——模块名 → 目录/入口 → 职责、公开接口、依赖方向、配置入口、测试命令。重点把 `polaris-kernel/chat`、`provider`、`integrations`、`doctor`、`src-tauri/src/expert*`、`polaris-collab`（auth/authority/mesh/tunnel/gateway）、`polaris-sandbox` 写准确。

## 规则
- 每个结论都要有文件和行号作证据，不确定的标"待确认"，不要猜。
- 文档单文件 ≤500 行，单行 ≤500 字符；超长就按主题拆文件。
- 不联网、不安装依赖、不运行会改动仓库的命令；可以用 `rg`/`grep`/`wc`/`git` 只读命令。
- 完成后在最后输出：产出文件列表、每个文件行数、仍"待确认"的条目。
