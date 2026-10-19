# R-release-v3：3.0.0 发布改动（默认 MiniMax + Pi 自带 + 去 Claude Code）

用户已决定（不要再提风险或反问）：
1. 三端安装包（Windows/macOS/Docker）自带 Pi agent，**不再依赖、不再分发 Claude Code**。
2. 默认模型改为自带的 **MiniMax**，API key **构建时打进安装包/镜像**。key 绝不能进 git、日志、前端 dist 的明文可检索位置之外的任何仓库文件。
3. 版本号改为 3.0.0。

## 1. 默认提供商 = MiniMax（key 构建时注入）
- 构建时读环境变量 `POLARIS_BUNDLED_MINIMAX_KEY`（Rust `option_env!` 或 build.rs；缺失时构建**不失败**，运行时回落到现状行为，便于本地开发）。
- 首次启动、`providers.json` 里没有用户配置的当前提供商时，自动写入内置提供商 `minimax`：baseUrl `https://api.minimaxi.com/anthropic`，api 类型 anthropic-messages，模型 `MiniMax-M3`，并设为 current。**已有用户配置（current_id 非空且不是内置默认）一律不覆盖。**
- 内置 key 不得出现在日志、`/api` 返回、导出配置里；列表接口只返回"内置"标记。用户自己填 key 后以用户为准。
- 默认值放配置/常量文件，不要散落硬编码（baseUrl、模型 ID、供应商 id 各一处）。
- 同时修已知缺陷：Codex 供应商默认模型 `gpt-5.3-codex-spark` 对 ChatGPT 账号不可用 → 默认改为 `gpt-6-astra`。模型请求失败时，错误提示应带上提供商返回的原因（脱敏），不能只显示"模型请求失败，请稍后重试"。
- 现有 Pi 的 provider 翻译（`provider/pi_translate.rs`）必须能把 `minimax` 翻成 Pi 可用的配置。

## 2. 去掉 Claude Code
- 默认且唯一发布的后端为 Pi。`BackendKind::ClaudeCli` 代码可保留但不得在任何默认路径、安装器、`fetch-runtimes`、Dockerfile、环境医生（doctor）里被安装、探测失败告警或提示用户安装。
- `scripts/fetch-runtimes.mjs` 中仅为 Claude Code 服务的内容（如 Git Bash/MinGit 仅供 Claude）：若确认只被 Claude CLI 使用则移除；若被别处使用则保留。判断依据写进报告。
- 前端/文案里"请安装 Claude Code / claude 命令"一类提示改为 Pi 自带说明或删除。
- `provider.current_id == "claude-official"` 的空值回落（`selection.rs`、`views.rs`）改为回落到内置 minimax。

## 3. 版本与发布配置
- 所有版本号 2.9.0 → 3.0.0：`package.json`、`src-tauri/tauri.conf.json`、各 `Cargo.toml`（含 lock 文件需同步，离线 `cargo check`）、`agentd/package.json` 如有版本字段。
- `.github/workflows/release.yml`、`mac-build.yml`、`docker-image.yml`：构建步骤增加 `POLARIS_BUNDLED_MINIMAX_KEY: ${{ secrets.MINIMAX_API_KEY }}` 环境变量；Docker 用 build-arg + 多阶段，**key 不得残留在最终镜像的 layer 历史里**（用 BuildKit secret 或只在编译阶段 ARG 且不进最终阶段；`docker history` 不能直接看到明文）。
- 同时修已知打包问题：release 流程里 `agentd` 需先 `npm ci`（目录内），否则 TypeScript 版本错误导致 `pack-agentd` 失败。
- 默认中继配置：本次**不改**（中继服务器尚未部署）。

## 约束
- 单文件 ≤500 行、单行 ≤500 字符（已超的历史文件不要再加逻辑，新逻辑放新文件）；不新增依赖；不改 `mobile/`；不 commit、不 push。
- 验证（在沙箱里能跑的都跑）：`cargo check -p polaris-app`、`cargo test -p polaris-kernel --lib provider`、`npx vue-tsc --noEmit`、`npx vitest run`、agentd `npm run typecheck && npm test`。跑不了的列出来，由 Claude 重跑。
- 新增测试：内置 provider 首次写入、已有配置不被覆盖、key 不出现在列表接口与日志。
- 报告写入 `docs/tasks/R-release-v3-report.md`，必须列出改动文件、每项验证结果、以及任何你没做的项和原因。
