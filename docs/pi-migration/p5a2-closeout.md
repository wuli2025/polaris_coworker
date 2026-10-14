# P5a-2 收尾执行报告

状态：BLOCKED（实现已落地，严格验收条件未全部满足）。未提交、未推送，未新增依赖。
环境：2026-10-14，Linux x64，Node 22.19.0；仅安装 x86_64-unknown-linux-gnu Rust target。
开始时用户已有未跟踪目录 `docs/execution/P5a2-1/`，保持不变。
本报告是执行记录；Claude 独立重跑，Pi 独立审查，最终由 Claude 决定完成状态。

## 改动文件与行为

| 文件（K = src-tauri/crates/polaris-kernel/src） | 行为 |
| --- | --- |
| `agentd/src/config/schema.ts` | 可选 shellPath，string/null 类型校验，兼容旧完整配置 |
| `agentd/src/config/load.ts` | 显式非空 JSON shellPath 优先，其次 POLARIS_AGENTD_SHELL_PATH，再 null；测试可显式传入 env |
| `agentd/config/agent.default.json` | shellPath 默认 null |
| `agentd/src/sessions/create.ts` | shellPath 传入 SettingsManager.inMemory，空值交给 Pi 默认解析 |
| `agentd/test/shell-config.test.ts` | 默认/空值/环境/显式优先级/非法类型；真实 Pi 会话设置及 `~` 展开 |
| `K/doctor/pi_shell.rs` | 共用 shell 解析、agentd 配置读取、Windows 子进程环境设置；不改变宿主 PATH |
| `K/doctor/pi_shell_tests.rs` | 随包存在、只有系统 Git Bash、都无；ProgramFiles 优先级、PATH 回落；PATH 前插去重；显式失效及类型错误 |
| `K/doctor/bundled.rs` | 从既有 git_bash 提取 git_bash_at，复用 runtime/git/usr/bin/bash.exe 定位 |
| `K/doctor/mod.rs` | 注册内部 pi_shell 模块 |
| `K/chat/backend/pi/process.rs` | Windows 启动 agentd 前注入已解析 shell 和 child_path_prefix |
| `K/doctor/pi.rs` | ping 与实际 spawn 使用同一环境设置；report 共用解析；verify 区分随包/系统或显式/缺失 |
| `K/doctor/path.rs` | no_window 导入仅 Windows 编译 |
| `K/doctor/probe.rs` | 删除 Stdio、Duration、Instant、no_window 冗余导入；helper 保留自身所需导入 |
| `K/doctor/verify.rs` | path::* 仅 Windows 编译，保留 Windows 子模块使用 |
| `K/doctor/install.rs` | 两个 Node 脚本函数与正文常量改为 cfg(any(windows, test)) |
| `K/doctor/install/tests.rs` | 安装模板测试用条件数组元素消除 Linux unused_mut，保持测试覆盖 |
| `scripts/runtime/sources.mjs` | Node/GitHub 各自生成源列表；验证 HTTPS、字符串、无凭据/query/fragment |
| `scripts/runtime/node.mjs` | 调用 Node 源列表，额外校验版本参数，不使用 GitHub 代理 |
| `scripts/fetch-runtimes.mjs` | 接入 POLARIS_NODE_MIRROR、共用源模块；GitHub 清单独立 |
| `scripts/runtime/node.test.mjs` | 调整 Node 源顺序断言；同一组测试可由 node:test 与 Vitest 执行 |
| `scripts/runtime/sources.test.mjs` | 精确 URL 顺序、空值关闭、错误类型/协议/URL、GitHub 独立回落测试 |
| `docs/feature-tree.md`、本报告 | 入口、依赖、配置、迁移、验证与阻塞信息 |

所有本次修改/新建文件均不超过 500 物理行，每行不超过 500 字符；新增 Rust 无 unwrap()/expect()。

## shell 约定与旧安装链核查

依据本地锁定 Pi 1.0.4 的 `agentd/node_modules/@earendil-works/pi-coding-agent/docs/settings.md:139`、
`docs/windows.md` 与 `dist/utils/shell.js`：显式 shellPath → ProgramFiles Git Bash → ProgramFiles(x86) Git Bash → PATH bash.exe。
宿主在未显式配置时优先采用随包 MinGit，将路径作为 agentd 默认 shell 注入；缺包则使用 Pi 的系统查找顺序。
随包使用 `usr/bin/bash.exe`，沿用 bundled.rs 的 child_path_prefix（usr/bin、mingw64/bin、可选 uv/Python）。
显式路径不存在时不回落，不把可用的其他 bash 当成显式 shell 已就绪。
doctor 读取 `--config <path>` 或 POLARIS_AGENTD_CONFIG，与 agentd 启动路径一致；不读取凭据内容到报告。
未找到 shell 时 shellReady=false；无其他 runtime 错误时 error=AgentShellMissing；verify shell 步骤始终给出缺失提示。

旧 Node 代码不是孤儿：

- `env_install_node → node_install_script → NODE_INSTALL_BODY`，入口已有 `require_legacy()`。
- `autopilot::run → install_node_silently → node_portable_script → WIN_NODE_PORTABLE_BODY`，run 已在 Pi 分支诊断后返回。
- 两条生产调用仅在 Windows 编译，原来函数/常量跨平台无条件编译才产生 Linux dead_code；现在保留调用及守卫，补齐 cfg。
- 脚本仍供跨平台模板行为测试编译，未通过删除测试或 allow(dead_code) 隐藏问题。

## 配置与迁移

| 参数 | 默认与校验 | 示例 |
| --- | --- | --- |
| agentd shellPath | 可省略、null 或字符串；空串视为未配置 | `{"shellPath":"C:/Tools/Git/bin/bash.exe"}` |
| POLARIS_AGENTD_SHELL_PATH | 未设置；宿主 Windows 自动注入解析结果；JSON 非空值优先 | 独立 agentd 可设 `C:/MinGit/usr/bin/bash.exe` |
| POLARIS_RUNTIME_DIR | 沿用已有随包定位 | `C:/Polaris/runtime` |
| POLARIS_DEPS_BASE | `https://llmwiki.cloud/downloads/deps`；HTTPS 基址；空串禁用 | `https://downloads.example.com/deps` |
| POLARIS_NODE_MIRROR | 未设置；HTTPS Node dist 基址；空串禁用 | `https://npmmirror.com/mirrors/node` |

Node 下载顺序：官方 `https://nodejs.org/dist/v<version>/<asset>` → R2 平铺 `<asset>` → Node 镜像 `/v<version>/<asset>`。
GitHub 资源维持 R2 → ghfast.top → ghproxy.net → GitHub；两种列表不交叉。
SHA256 清单仍只取官方；官方清单不可用会失败，不会以镜像清单替代官方校验。
无需迁移旧配置；已有 shellPath 非空值优先；改回 null 恢复启动时默认选择。
下载切换：`POLARIS_NODE_MIRROR=https://npmmirror.com/mirrors/node node scripts/fetch-runtimes.mjs`。
旧 Claude Node 安装模板的 npmmirror 列表原本没有 GitHub 代理，本次未改其版本或下载安装行为。

## 实际验证

Cargo 命令在 `src-tauri/` 执行；npm/tsx 在 `agentd/`；其他命令在仓库根。
原始日志位于 `/tmp/p5a2-*.log`，未写入用户已有执行目录。

| 实际命令 | 结果 |
| --- | --- |
| 修改前 `cargo check -p polaris-app --offline` | 成功，9 条实际诊断警告，全部在 doctor；不是 brief 所述 10 条 |
| `npm run typecheck && npm test` | typecheck 成功；npm test 26 文件，24 通过、2 失败；退出 1 |
| `node --import tsx test/shell-config.test.ts` | 新增 2 个行为测试通过 |
| `node --import tsx test/web.test.ts` | 2 通过、1 失败：本地 HTTP 监听 EPERM |
| `node --import tsx test/e2e.test.ts` | 失败：等待 ping 超时，子进程 exit=0，stderr 空 |
| `cargo check -p polaris-app --offline` | 成功，0 条警告 |
| `cargo check -p polaris-cli --offline` | 成功，4 条警告，均来自既有 src/mesh.rs |
| `cargo check -p polaris-app --no-default-features --features server --offline` | 成功，同样 4 条 src/mesh.rs 警告 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 189 通过、6 个 TCP 沙箱失败、2 忽略；lib 失败后 Cargo 未运行集成测试 |
| 上条命令追加下列 6 个 `--skip` | 单元 189 通过、2 忽略、6 排除；集成 8 通过，零失败 |
| `npx vue-tsc --noEmit` | 成功 |
| `npx vitest run` | 10 文件、32 测试通过 |
| `node --test scripts/runtime/*.test.mjs` | 成功，2 文件通过；默认隔离报告只显示文件汇总 |
| `node --test --test-reporter=spec scripts/runtime/*.test.mjs` | 成功，同样 2 文件通过 |
| `node --test --experimental-test-isolation=none scripts/runtime/*.test.mjs` | 7 个行为测试通过，确认不是空测试套件 |
| `node scripts/runtime/node.test.mjs` / `node scripts/runtime/sources.test.mjs` | 分别 4 / 3 个测试通过 |
| `git diff --check`、Python 文件行数/行长检查 | 通过 |

排除项完整名称（命令在 Cargo `--` 后逐项传 `--skip <名称>`）：

1. `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`
2. `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`
3. `provider::claude_login::tests::loopback_run_end_to_end_local`
4. `provider::claude_login::tests::loopback_run_rejects_wrong_state`
5. `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`
6. `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`

前四项显式 PermissionDenied；后两项无法绑定 8765–8789。本次未修改或禁用这些测试。
两项 ignored 为既有安装脚本导出与 NAS 真机列表测试，不计为通过。
agentd 两个失败与 P5a 执行报告记录相同。本次还运行最小 Node spawn/stdin 回显复现：
父进程写入 `pipe-probe`，子进程退出 0、输出空字符串，未接收预期内容；因此不能以此次 E2E 失败推定 shell 接线回归。
该复现也不能将 E2E 计为通过，需在管道正常的环境独立重跑。

## 未完成与阻塞原因

- agentd `npm test` 未全绿：HTTP TCP 沙箱限制，以及 Node 父子管道异常；不为通过而跳过或弱化断言。
- brief 要求两壳警告数严格各为 7，本机实际为 0 / 4，不能宣称满足等式；本次要求清理的 doctor 警告已全部消除。
- 服务端剩余四条来自白名单外 `src-tauri/src/mesh.rs`：473/1386 不可达代码，329 未用 c，1348 未用 access；未扩大范围修改。
- 未运行 Windows/macOS 真机 shell、安装包验证或交叉编译；路径解析与环境单元测试在 Linux 执行。
- 未运行真实 runtime 下载/解压、Docker build/smoke；这次仅验证 URL 生成、校验逻辑及现有测试，不声称网络源可达或安装成功。

需 Claude 核对警告基线的命令/平台口径，并在支持本地 TCP 与 Node 子进程管道的环境重跑全量 agentd 测试。
