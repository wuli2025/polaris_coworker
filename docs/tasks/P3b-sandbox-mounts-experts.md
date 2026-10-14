# 任务 P3b：沙箱挂载最小化、专家入口脱离写文件、测试配置隔离

## 1. 沙箱挂载最小化（安全，高优先级）
现状（`polaris-sandbox/src/commands.rs:205-232`）：`sandbox_start` 把整个 `~/Polaris` 读写挂到 `/workspace`。该目录含 `data/providers.json`（明文模型 Key）、`data/host.key`、`data/device.key`（互联设备身份）、`data/collab.db`（账号库）等。沙箱内任何代码都能读取密钥、冒充本机设备。
要求：
- 改为**按项目挂载**：只把目标项目/工作目录读写挂到 `/workspace`；知识库 `/kb` 只读；`data/`、密钥、账号库、`claude-home`、会话目录等**一律不挂**。挂载清单由配置给出（schema + 默认值），代码中维护一份"禁止挂载"路径名单作为兜底校验（即使配置写错也拒绝挂载这些路径）。
- 工作目录变化时：同一容器可复用则复用，否则按配置重建；旧的宽挂载容器检测出来后**提示用户迁移**（给出明确错误/提示文案与一键重建入口），不静默继续使用，不自动删除用户容器。
- P3 的工具路由（`chat/backend/docker_tools.rs`、`host_tools.rs`）改为使用新挂载策略；`SANDBOX_PATH_UNMOUNTED` 等错误保持。
- Windows 路径（`path_for_docker` 的盘符转换）沿用并补测试。
- 测试：生成的 `docker run` 参数中绝不出现 `data/`、`*.key`、`providers.json`、`collab.db`；配置误写这些路径时被拒绝；旧宽挂载容器被识别。

## 2. 专家/团队显式选择脱离写文件
现状（`docs/pi-migration/p3-result.md` 第 70-72 行）：`ChatComposer` 的显式专家/团队选择仍调用 `expert.apply` / `teamApply` 写项目 `CLAUDE.md`。
要求：Pi 后端下，显式选择改为随本轮请求传递专家 id（复用 P3 已有的 `expertPrompt` 装载路径），**不写项目文件**；旧后端行为不变。前端改动限 `ChatComposer` 中专家选择的调用处与对应 store/类型，新增逻辑放新文件；`ChatComposer.vue` 已有 2556 行（既有技术债），只允许净增 ≤15 行。专家团默认关闭不变。
测试：Pi 后端下选择专家后项目目录中 `CLAUDE.md`/`AGENTS.md` 未被修改，且专家提示进入了本轮请求。

## 3. 根 vitest 误收 agentd 测试
根目录 `npx vitest run` 把 `agentd/test/*.test.ts`（node:test）当作 vitest 测试导致 19 个文件失败。在 `vite.config.ts`（或新增 vitest 配置）中排除 `agentd/**`；前端测试照常运行。

## 规则
- 允许修改：`src-tauri/crates/polaris-sandbox/`、`src-tauri/crates/polaris-kernel/src/chat/backend/`、`src-tauri/src/expert*`（仅专家传递所需）、`src/components/chat/ChatComposer.vue`（净增 ≤15 行）与相关 store/类型/新文件、`vite.config.ts`、测试、`docs/`。
- 无新依赖；新写/改动文件 ≤500 行、≤500 字符（既有超长文件按上面的净增限制）；新 Rust 代码无 unwrap/expect；可变值进配置。
- 必须通过：agentd `npm run typecheck && npm test`；两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline` 与 `cargo test -p polaris-sandbox --offline`（基线与沙箱 TCP 项除外并列出）；`npx vue-tsc --noEmit`；`npx vitest run` 全部通过。不 commit。

## 来自 P3 审查的并入项（见 `docs/reviews/P3-round1-triage.md`）
- P3b-2：受保护路径名单从 `src-tauri/src/wiring.rs` 写死改为"配置 + 代码兜底名单"（兜底名单至少含 `~/Polaris/data`、`~/.polaris`、`~/.pi`、`~/.claude`、`~/.codex`、`~/.ssh`、`~/.aws`、`~/.gnupg`、`~/.kube`），配置只能追加不能移除兜底项。
- P3b-4：路由前校验容器挂载的 `Destination` 与读写属性：`/workspace` 必须是目标项目目录且可写，`/kb` 必须只读，不允许其他宿主路径挂载。
- P3b-5：容器创建时按配置设置内存/CPU 上限、`--cap-drop=ALL`（按需最小加回）、`--security-opt no-new-privileges`；路由前校验这些安全基线，不满足报 `SANDBOX_UNSAFE_CONTAINER` 并提示重建。
