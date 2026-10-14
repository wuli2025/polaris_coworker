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
# 任务 P5a-2：P5a 收尾

1. **Windows shell（范围例外已批准）**：按 Pi `docs/settings.md` 的 `shellPath`：agentd 配置增加 `shellPath`（可空），`agentd/src/sessions/create.ts` 传给 SettingsManager；Rust `src-tauri/crates/polaris-kernel/src/chat/backend/pi/process.rs` 在 Windows 下把随包 MinGit 的 bash 路径（复用 `doctor/bundled.rs` 现有定位逻辑与 PATH 前插）注入 agentd 配置/环境；未找到随包 bash 时按 Pi 文档的查找顺序回落并在 doctor 中报告。测试：路径解析三种情形（随包存在、仅系统 Git Bash、都没有）。
2. **消除新增编译警告**：`cargo check -p polaris-app` 与服务端壳的警告数必须回到基线 7（当前 10）。`doctor/path.rs`、`probe.rs`、`verify.rs` 的未用导入删除；`doctor/install.rs` 的 `node_install_script`/`node_portable_script`/`WIN_NODE_PORTABLE_BODY`/`NODE_INSTALL_BODY` 未被使用：确认它们原先的调用方，若属于旧后端的 Claude 安装链则按 `agent.backend` 守卫接回原调用点，若已无意义则删除并说明。
3. **Node 下载源顺序**：`ghfast.top`、`ghproxy.net` 是 GitHub 代理，不能代理 nodejs.org（实测 HTTP 403）。下载源按"资源来源"分别给出镜像列表：nodejs.org 资源用官方源 + 自家 R2/`POLARIS_DEPS_BASE`（如有）+ 可配置的 Node 镜像（例如 `POLARIS_NODE_MIRROR`），不再拼接 GitHub 代理；测试 URL 生成。

允许修改：`agentd/src/config/`、`agentd/src/sessions/create.ts`、`agentd/config/`、`src-tauri/crates/polaris-kernel/src/chat/backend/pi/process.rs`、`src-tauri/crates/polaris-kernel/src/doctor/`、`scripts/fetch-runtimes.mjs`、`scripts/runtime/`、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect。
必须通过：agentd `npm run typecheck && npm test`；两壳 `cargo check --offline`（**警告数 = 7**）；`cargo test -p polaris-kernel --lib --tests --offline`（应 0 失败，沙箱 TCP 项除外并列出）；`npx vue-tsc --noEmit`；`npx vitest run`；`node --test scripts/runtime/*.test.mjs`。不 commit。

--- END TASK DATA ---
