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
# 任务 P5a：随包运行时（Node + agentd）、环境诊断（doctor）、Docker 镜像

先读：`docs/pi-migration/call-sites.md`"供应商、认证、用量与安装"中 doctor 各行、"间接 Agent 调用与宿主入口"中 `Dockerfile`/`docker-compose.yml`/`.env*`/`lib.rs`/`server.rs` 行；`scripts/fetch-runtimes.mjs`（现有随包运行时机制）、`src-tauri/tauri.conf.json` 的 `bundle.resources`、`polaris-kernel/src/doctor/`（含 `bundled.rs`）、`chat/backend/config.rs`（agentd 的 node/entry/args 配置）。

## 1. 随包分发 Node 与 agentd（桌面版）
- `fetch-runtimes.mjs` 增加 Node（版本与 `agentd/package.json` 的 engines 一致，按平台下载官方便携包并校验 SHA256；下载源顺序沿用脚本现有规矩，幂等 `.stamp.json`）。
- 新增 agentd 打包步骤：`agentd` 构建产物 + **仅生产依赖**（锁定版本，`--ignore-scripts`）放进 `src-tauri/runtime/agentd/`；报告打包体积，并评估可裁剪项（不得破坏 `sdk-contract.test.ts` 所测能力）。原生/WASM 依赖（如 photon、quickjs-wasi）按平台核实可用。
- `tauri.conf.json` 把 runtime 目录纳入 `bundle.resources`；`chat/backend/config.rs` 的默认 node/entry 在未显式配置时解析为随包路径（开发态回落到仓库内 `agentd/`），解析逻辑可测、不写死绝对路径。
- Windows：Pi 的 bash 工具需要 bash——复用已内置的 MinGit bash（`doctor/bundled.rs` 现有 PATH 前插逻辑），通过 agentd 配置/Pi 设置把 shell 路径指过去；核查 Pi 文档中 Windows shell 的配置方式并引用出处。
- 测试：路径解析（随包/开发态/显式配置三种）；脚本的版本与校验和逻辑（离线单测，不真下载）。

## 2. doctor（环境诊断）
- 默认检测对象由 Claude CLI 改为 Pi 后端运行时：Node 可执行、agentd 入口存在、`ping` 握手（协议版本、Pi 版本）、模型凭据就绪（来自 P4 共享存储，只报"已配置/未配置"，不显示明文）。
- `autopilot` 不得在 Pi 后端下自动安装 Claude；Claude 安装/更新/校验保留为旧后端的显式动作（`agent.backend=claude-cli` 时才展示与执行）。
- 拆分本次触及的超长文件（`install.rs` 1118、`verify.rs` 1270、`probe.rs` 706、`path.rs`）到 ≤500 行；纯搬移与行为修改分开说明。
- 修复基线的 4 个 Windows 路径测试，使其跨平台（按平台构造期望值或用 `cfg`），基线失败清零。
- 前端类型（`doctor/types.rs` 对应的 TS）同步；UI 文案留给 P5c，但类型必须能编译。
- 测试：Pi 后端就绪/各类不就绪的报告；Pi 后端下 autopilot 不触发 Claude 安装。

## 3. Docker / 服务端
- `Dockerfile`：默认镜像安装 Node + agentd（生产依赖），不再强制安装/验证 Claude；旧后端所需步骤按构建参数可选。
- `docker-compose.yml`、`.env.example`、`.env.server.example`：增加 agentd 相关配置项与凭据卷说明；旧 Claude 凭据卷保留兼容，**不删除用户数据**。
- `server.rs` 健康报告由"Claude 配置"改为"Agent 运行时健康"（复用 `agent_backend_status`）。
- `scripts/docker-smoke.ps1` 的验收字段同步。

## 规则
- 允许修改：`scripts/fetch-runtimes.mjs`（及其测试/新增脚本）、`src-tauri/tauri.conf.json`、`polaris-kernel/src/doctor/`、`polaris-kernel/src/chat/backend/config.rs`、`src-tauri/src/lib.rs`/`server.rs`/`apihub/` 中 doctor 与健康相关装配、`src/tauri.ts` 中 doctor 类型（既有超长，净增 ≤15 行，新类型放新文件）、`Dockerfile`、`docker-compose*.yml`、`.env*.example`、`scripts/docker-smoke.ps1`、`agentd/`（仅构建/打包脚本与 package.json scripts，不改依赖版本）、测试、`docs/`。
- 依赖：不新增 crate / npm 依赖。Node 便携包属运行时下载物，按脚本现有方式处理。
- 每文件 ≤500 行、每行 ≤500 字符；新 Rust 代码无 unwrap/expect；版本、下载源、路径进配置。
- 必须通过：agentd `npm run typecheck && npm test`；两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`（**基线 4 项应已修复**；沙箱 TCP 项除外并列出）；`npx vue-tsc --noEmit`；`npx vitest run`。不 commit。
- 结束时输出：随包体积与组成、Windows shell 配置依据、各部分完成情况、改动文件与行数、命令结果、需要 P5c 前端配合的文案/类型点。

--- END TASK DATA ---
