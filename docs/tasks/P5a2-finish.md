# 任务 P5a-2：P5a 收尾

1. **Windows shell（范围例外已批准）**：按 Pi `docs/settings.md` 的 `shellPath`：agentd 配置增加 `shellPath`（可空），`agentd/src/sessions/create.ts` 传给 SettingsManager；Rust `src-tauri/crates/polaris-kernel/src/chat/backend/pi/process.rs` 在 Windows 下把随包 MinGit 的 bash 路径（复用 `doctor/bundled.rs` 现有定位逻辑与 PATH 前插）注入 agentd 配置/环境；未找到随包 bash 时按 Pi 文档的查找顺序回落并在 doctor 中报告。测试：路径解析三种情形（随包存在、仅系统 Git Bash、都没有）。
2. **消除新增编译警告**：`cargo check -p polaris-app` 与服务端壳的警告数必须回到基线 7（当前 10）。`doctor/path.rs`、`probe.rs`、`verify.rs` 的未用导入删除；`doctor/install.rs` 的 `node_install_script`/`node_portable_script`/`WIN_NODE_PORTABLE_BODY`/`NODE_INSTALL_BODY` 未被使用：确认它们原先的调用方，若属于旧后端的 Claude 安装链则按 `agent.backend` 守卫接回原调用点，若已无意义则删除并说明。
3. **Node 下载源顺序**：`ghfast.top`、`ghproxy.net` 是 GitHub 代理，不能代理 nodejs.org（实测 HTTP 403）。下载源按"资源来源"分别给出镜像列表：nodejs.org 资源用官方源 + 自家 R2/`POLARIS_DEPS_BASE`（如有）+ 可配置的 Node 镜像（例如 `POLARIS_NODE_MIRROR`），不再拼接 GitHub 代理；测试 URL 生成。

允许修改：`agentd/src/config/`、`agentd/src/sessions/create.ts`、`agentd/config/`、`src-tauri/crates/polaris-kernel/src/chat/backend/pi/process.rs`、`src-tauri/crates/polaris-kernel/src/doctor/`、`scripts/fetch-runtimes.mjs`、`scripts/runtime/`、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect。
必须通过：agentd `npm run typecheck && npm test`；两壳 `cargo check --offline`（**警告数 = 7**）；`cargo test -p polaris-kernel --lib --tests --offline`（应 0 失败，沙箱 TCP 项除外并列出）；`npx vue-tsc --noEmit`；`npx vitest run`；`node --test scripts/runtime/*.test.mjs`。不 commit。
