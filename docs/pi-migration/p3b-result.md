# P3b 第 2 次执行报告：BLOCKED（待 Claude / Pi 核实）

2026-10-14。本轮在已批准范围继续实现，保留已有创建策略、schema、测试和 Vitest 排除项。
未 commit/push，未增加依赖，未操作真实 Docker 容器。下方保留第 1 次报告作为历史记录；本节为当前结论。

## 本轮改动文件与行为

| 文件（仓库相对路径） | 行为 |
| --- | --- |
| `src-tauri/crates/polaris-kernel/src/chat/backend/host_tools.rs` | 新增 SandboxPolicy、ExpertLoader 公开注入端口；专家加载失败传播为本轮错误 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/docker_tools.rs` | 每次调用在 docker exec 前将完整 inspect 与目标 cwd 交给策略校验；通过后映射 /workspace；保留 PATH_UNMOUNTED，校验耗时计入执行预算 |
| `src-tauri/src/wiring.rs` | 桌面注入 sandbox::Config::load().validate_container，共用创建侧保护名单、挂载和安全基线；两壳注入只读 expert_export/team_export |
| `src-tauri/crates/polaris-kernel/src/chat/backend/{expert_selection.rs,bridge.rs,mod.rs}` | 解析带 kind 的专家/团队 id；复用 expertPrompt 请求路径；空 id、未知类型和找不到模板均失败 |
| `src-tauri/crates/polaris-kernel/src/chat/types.rs` | 可选 expertSelection 字段，旧请求默认 None |
| `src-tauri/crates/polaris-kernel/src/integrations/feishu.rs` | 仅给结构体字面量补 expert_selection: None，净增 1 行 |
| `src/lib/expertSelection.ts`、`src/components/chat/ChatComposer.vue` | Pi 显式选择不调用写文件 API；旧后端继续 apply/teamApply；主发送和 sendOptions 均携带本轮选择，Composer 净增 3 行 |
| `src/stores/{chat.ts,chatTypes.ts}`、`src/components/chat/shared.ts` | 请求选项和 store 转发 expertSelection；Bubble 的公开导出保持兼容 |
| `src/stores/{chatArtifacts.ts,chatDeltas.ts}` | 为满足行数约定按职责拆出既有产物逻辑和 token 缓冲；chat.ts 从 667 降至 483 行 |
| `src/features/sandbox/{api.ts,project.ts,components/SandboxProject.vue,components/SandboxStatus.vue}` | 目录输入/选择、确认并启动、默认当前项目绑定目录、成功后更新工作目录；识别迁移错误并提供保留旧容器的一键重建；Status 净减 12 行 |
| `src-tauri/crates/polaris-sandbox/src/commands.rs` | 停止状态的旧容器也做状态检查；缺少必需挂载给出明确迁移提示 |
| `tests/frontend/expertSelection.test.ts`、`src/features/sandbox/project.test.ts`、`src/stores/{chatDeltas,chatArtifacts}.test.ts` | 临时项目文件不变、store 请求携带 id、legacy apply、基本模式清除选择、项目启动/重建参数、缓冲和产物回归行为 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/extension_tests.rs`、expert_selection/docker_tools 内联测试 | 注入端口强制拒绝、专家类型校验、专家与团队提示进入本轮 session.open，临时 CLAUDE.md/AGENTS.md 内容不变 |
| `docs/feature-tree.md`、本报告 | 入口、依赖、配置、迁移、验证与当前边界 |

前次已有文件 `polaris-sandbox/{sandbox.schema.json,src/config.rs,src/policy.rs,src/lifecycle.rs,src/policy_tests.rs,src/lib.rs,src/templates/Dockerfile.sandbox}` 和 `vite.config.ts` 保留复用。
本轮未改 `docs/execution/P3b-1/`、`docs/execution/P3b-2/` 的已有内容。

## 配置和用户迁移

沙箱默认配置、追加保护名单及切换示例见下方历史报告的“配置与公开接口”段，本轮沿用。
桌面路由每次读取 `POLARIS_SANDBOX_CONFIG`；配置错误不会回退为宽挂载。kernel 不依赖 sandbox crate，由宿主注入策略。
`/workspace` 必须等于本轮目标目录且 RW，`/kb` 必须等于宿主 KB 根且 RO；其他挂载、宽挂载和资源/权限基线不合规均拒绝。
UI 默认填当前会话所属项目已绑定的工作目录；未绑定时要求用户选择或输入目录，不猜测后台回落 cwd。
点击“确认目录并启动”携带 projectDir；旧宽挂载/安全基线错误显示迁移说明及“保留旧容器并一键重建”。
rebuild:true 仍为停止、改名保留旧容器、创建新容器；不自动删除旧容器。
工作目录变化按 workdirChange=reject/rebuild 处理，只在显式启动时重建，工具路由自身不会删除或创建容器。
UI 管理固定容器 polaris-sandbox；agentd.sandboxContainer 如自定义，须自行创建并管理同策略容器。

本轮请求字段例：`"expertSelection":{"kind":"expert","id":"qa-expert"}`；团队使用 kind="team" 与 roster 返回的团队 id。
缺省不带该字段，旧调用方无需迁移；UI 选基础模式后不再携带先前 id。
agentd 的专家功能默认仍关闭。显式启用示例：配置 `{"experts":{"enabled":true,"prompt":""}}` 并重开会话。
关闭时仍传 expertPrompt，但 agentd 不追加模型系统提示；本轮未更改默认开关，也未修改 agentd 生产代码。
为满足 store 行数限制搬出的 delta 缓冲默认 40ms；可用 `createDeltaBuffer(ensureArr, 20)` 显式切换，拒绝负数和非有限值。

## 实际运行命令与真实结果

Rust cwd=`src-tauri`；agentd cwd=`agentd`；前端 cwd=仓库根。日志位于 `/tmp/p3b-r2-*.log`。

| 命令 | 结果 |
| --- | --- |
| `npm run typecheck && npm test`（agentd） | typecheck 成功；npm test 18 文件通过、2 失败；退出 1 |
| `cargo check -p polaris-app --offline` | 通过，退出 0；6 项既有 kernel warning |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 通过，退出 0 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 170 通过、10 失败、2 忽略；退出 101；lib 失败后未运行 integration tests |
| 上述 kernel 命令加下方 10 个完整名称的 `--skip` | 170 lib + 4 integration 通过，2 忽略、10 排除；退出 0 |
| `cargo test -p polaris-sandbox --offline` | 13 通过，doc-tests 0；退出 0 |
| `npx vue-tsc --noEmit` | 最终通过，退出 0 |
| `npx vitest run` | 最终 8 文件、23 测试通过，退出 0；未收集 agentd |
| `node --import tsx test/web.test.ts` | 2 通过、1 失败；127.0.0.1 listen EPERM；退出 1 |
| `node --import tsx test/e2e.test.ts` | 0 通过、1 失败；等待 ping 超时，exit=0、stderr 为空；退出 1，根因未确认 |
| `git diff --check`、修改文件行数/行长和新增 Rust unwrap/expect 扫描 | 最终通过；例外文件增量见上表 |

过程中的失败也保留说明：桌面和服务端 cargo check 各曾误在仓库根启动，因找不到 Cargo.toml 退出 101，随后在 src-tauri 重跑；一次脚本用 python 启动失败（未安装），改 python3 执行。
vue-tsc 曾发现 selectedExpert 命名冲突，已使用导入别名修复；新增真实文件测试最初放 src 下缺 node 类型，移至 tests/frontend 后前端检查通过，Vitest 实际运行该测试。
没有新增类型依赖，没有把测试假定为通过。没有运行本轮 rustfmt（前次已确认 toolchain 未安装）。

内核排除项与前次相同：

```text
doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix
doctor::path::tests::prime_and_prepend_behaviour
doctor::verify::tests::install_root_groups_npm_entries_together
doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts
integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp
integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp
provider::claude_login::claude_oauth_tests::loopback_run_end_to_end_local
provider::claude_login::claude_oauth_tests::loopback_run_rejects_wrong_state
provider::store::local_route_tests::openai_provider_routes_via_local_router
provider::store::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

前四项为既有 baseline，后六项为本环境端口限制。实际过滤命令为 `cargo test -p polaris-kernel --lib --tests --offline --` 后逐项追加 `--skip <完整名称>`。

## 未完成与阻塞

1. 服务端 Cargo feature 不依赖 polaris-sandbox，而 sandbox 当前强依赖 Tauri；“不增加 crate 依赖、不跨 crate 引源码”的范围限制下不能直接复用其策略。当前仅 desktop 装配 SandboxPolicy；server 不安装 executor，沙箱调用显式返回 SANDBOX_UNAVAILABLE。未保留旧的部分校验。已请求确认是否接受服务端禁用，或批准共享策略依赖调整；尚未收到答复。不能把双壳编译成功当成服务端沙箱功能完成。
2. agentd 的两项失败仍未解决：web 为本环境 TCP 权限限制；e2e 原因未确认。未修改范围外的 agentd 生产代码或隐藏失败测试。
3. 未运行真实 Docker/Windows 实机、浏览器按钮点击和真实模型验证。单测用假 Docker 命令、临时文件及 Pi 协议 fixture；不声称容器实机安全验收通过。

因此报告仍为 **BLOCKED**。本报告仅记录执行结果，Claude 自行复跑、Pi 独立审查，由 Claude 决定模块是否完成。

---

# 第 1 次执行历史报告（下列待授权状态已被本轮更新）

2026-10-14。没有 commit/push，没有新增依赖，没有操作真实 Docker 容器。
保留任务开始时的用户文件；未修改 `docs/execution/P3b-1/` 等既有未跟踪内容。
本报告不代表模块完成或最终验收。

## 已落地文件与行为

| 文件（相对仓库根） | 行为 |
| --- | --- |
| `src-tauri/crates/polaris-sandbox/src/config.rs` | 类型化配置、默认值、未知字段/非法类型/非法限额校验；保护路径配置只能追加 |
| `src-tauri/crates/polaris-sandbox/sandbox.schema.json` | 配置 JSON Schema，默认值与运行时一致性测试 |
| `src-tauri/crates/polaris-sandbox/src/policy.rs` | 公开 resolve_mounts/run_args/validate_container/validate_security；项目 RW + KB RO，禁止额外挂载，校验资源/权限；Windows 路径转换 |
| `src-tauri/crates/polaris-sandbox/src/lifecycle.rs` | 安全容器复用；配置控制工作目录变化；显式迁移停止并改名保留旧容器；从不自动删除旧容器 |
| `src-tauri/crates/polaris-sandbox/src/commands.rs` | start 使用目标项目；status/诊断 exec 检查当前挂载与安全基线；缺少项目路径报具名错误 |
| `src-tauri/crates/polaris-sandbox/src/lib.rs` | 导出上述公开模块，注册策略测试 |
| `src-tauri/crates/polaris-sandbox/src/policy_tests.rs` | 挂载参数、敏感路径/父路径/符号链接、访问模式、旧容器、安全基线、Windows 路径、配置与生命周期决策测试 |
| `src-tauri/crates/polaris-sandbox/src/templates/Dockerfile.sandbox` | 更新挂载和 cap-drop 注释，不改变镜像构建步骤 |
| `vite.config.ts` | 使用 vitest/config；保留 configDefaults.exclude 并追加 agentd/** |
| `docs/feature-tree.md`、本报告 | 更新入口、依赖、配置、迁移、验证和未接入边界 |

所有本次改动文件均小于 500 物理行、每行小于 500 字符。
ChatComposer 没有改动，净增 0 行。新 Rust 代码没有 `.unwrap()` / `.expect()`。
commands.rs 原有镜像构建函数中的 unwrap 没有新增或改动。

## 配置与公开接口

`POLARIS_SANDBOX_CONFIG` 指向 JSON 文件；未设置时使用以下默认值。
设置后文件缺失、JSON 错误、未知字段、类型错误均返回错误，不静默采用默认值。

```json
{
  "mounts": [
    {"source":"${project}","destination":"/workspace","readOnly":false},
    {"source":"${kb}","destination":"/kb","readOnly":true}
  ],
  "protectedPaths": [],
  "memoryBytes": 4294967296,
  "cpus": 2.0,
  "capAdd": [],
  "workdirChange": "reject"
}
```

- `${project}` 来自显式 projectDir；`${kb}` 来自宿主注入的 KbLocator，不回落到 Polaris 根。
- 也可配置绝对 source，但实际 canonical 路径必须与本轮目标项目/KB 分别一致。
- 两个目录必须存在且互不嵌套；不创建用户项目，也不扩大挂载到其父目录。
- `/workspace` 必须可写，`/kb` 必须只读；只允许这两项 bind mount。
- 保护名单至少包括 `~/Polaris/data`、`~/Polaris/claude-home`、`~/Polaris/sessions`、`~/Polaris/conversations`、`~/.polaris`、`~/.pi`、`~/.claude`、`~/.codex`、`~/.ssh`、`~/.aws`、`~/.gnupg`、`~/.kube`，以及宿主控制目录。
- 敏感名称 data、claude-home、sessions、conversations、上述隐藏目录、providers.json、collab.db、*.key 不允许作为挂载源或出现在挂载树中。扫描会拒绝含这些名称的普通业务项目；这是保守限制，不做自动搬迁。
- 扫描不跟随目录内符号链接；挂载源本身解析 canonical 路径。此检查不是对并发恶意替换文件/硬链接的完整 OS 防护承诺。
- `protectedPaths` 必须为绝对路径，只能追加；空数组不会移除代码兜底项。
- memoryBytes 为 6 MiB..i64::MAX，cpus 为 0.01..1024；容器实际限额须为正且不超过配置。
- 必须 drop ALL、no-new-privileges；capAdd 仅接受 CHOWN/FOWNER/DAC_OVERRIDE/NET_BIND_SERVICE。未配置时不加回能力。
- 资源与安全基线不符返回 SANDBOX_UNSAFE_CONTAINER；挂载不符返回 SANDBOX_UNSAFE_MOUNT；只有工作目录变化返回 SANDBOX_PATH_UNMOUNTED。

切换示例：将上述配置保存到 `/absolute/path/sandbox.json`，修改 `memoryBytes` 为 2147483648、`cpus` 为 1.0、`workdirChange` 为 `rebuild`，并设置：

```sh
export POLARIS_SANDBOX_CONFIG=/absolute/path/sandbox.json
```

追加保护路径示例：`"protectedPaths":["/absolute/private-model-credentials"]`。
工作目录变化的策略只在显式调用 sandbox_start 时执行；未接入工具路由自动重建。

## 迁移命令和当前 UI 限制

```ts
await invoke("sandbox_start", { projectDir: "/absolute/path/to/project" });
// 明确迁移：停止旧容器，改名保留，然后创建只挂载目标项目的新容器。
await invoke("sandbox_start", {
  projectDir: "/absolute/path/to/project",
  rebuild: true,
});
```

旧宽挂载不会被静默复用，workdirChange=rebuild 也不能绕过旧宽挂载/安全基线错误。
错误提示包含上述 rebuild:true 入口。迁移保留名称为 `polaris-sandbox-previous-<时间戳>`。
创建失败时返回错误并给出保留名称，不删除备份，也不自动重启不安全旧容器。
原有 sandbox_stop 是用户显式停止/删除当前容器命令，未改变其既有语义。

**沙箱 UI 尚未接线**：既有 start() 不传 projectDir，因此当前点击启动会报 SANDBOX_PROJECT_REQUIRED。
还没有「一键重建」按钮，不能把上述命令入口算作 UI 验收通过。

## 未完成与范围阻塞

模块约定要求边界缺失时报告 BLOCKED；已发出三项澄清，当前未收到回复，未擅自修改这些文件：

1. `src-tauri/src/wiring.rs`：P3b-2 明确要求改接线，但该文件不在允许修改清单。需要在宿主通过公开接口将 sandbox 策略注入 kernel，避免增加 crate 依赖或跨 crate 源码引用；当前 `docker_tools.rs` / `host_tools.rs` 仍保持 P3 实现。
2. `src/features/sandbox/api.ts` 与 `components/SandboxStatus.vue`：项目选择和一键重建所需入口未列入清单，UI 未接线。
3. `src-tauri/crates/polaris-kernel/src/integrations/feishu.rs`：专家 id 的可选请求字段需要给此处 ChatSendArgs 字面量补兼容值；该文件不在允许清单且既有超过 500 行，需要一行兼容修改的范围与行数例外。专家迁移没有实施，Pi 下显式选择仍存在写项目文件行为。

因此，创建侧安全校验已具备，但 **P3b-2 接线、P3b-4/P3b-5 路由前强制校验、专家请求传递、一键重建 UI 均未完成**。
没有运行或声称通过“选择专家不修改 CLAUDE.md/AGENTS.md 且提示进入本轮请求”的测试。

## 实际运行命令与结果

Rust 命令 cwd=src-tauri；agentd 命令 cwd=agentd；其余 cwd=仓库根。
日志为 `/tmp/p3b-*.log`，最终结果如下。

| 命令 | 真实结果 |
| --- | --- |
| `npm run typecheck && npm test`（agentd） | typecheck 通过；npm test 18 文件通过、2 失败，退出 1 |
| `cargo check -p polaris-app --offline` | 通过，退出 0；6 个既有 kernel 警告 |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 通过，退出 0 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 168 通过、10 失败、2 忽略，退出 101；lib 失败后未继续 integration tests |
| 同一 kernel 命令加下面的十项 `--skip` | 168 lib + 4 integration 测试通过、2 忽略、10 排除，退出 0 |
| `cargo test -p polaris-sandbox --offline` | 13 通过，doc-tests 0，退出 0 |
| `npx vue-tsc --noEmit` | 通过，退出 0 |
| `npx vitest run` | 4 文件、13 测试全部通过，退出 0；未收集 agentd |
| `node --import tsx test/web.test.ts` | 2 通过、1 失败；本地监听 EPERM，退出 1 |
| `node --import tsx test/e2e.test.ts` | 0 通过、1 失败；ping 等待超时，子进程 exit=0、stderr 空，退出 1；没有推断其根因已解决 |
| `git diff --check`；逐文件行数/行长扫描 | 通过 |
| `rustfmt --edition 2021 <本次 Rust 文件>` | 未能运行：当前 stable toolchain 未安装 rustfmt；没有联网安装 |

中途曾在仓库根误运行 cargo test，因没有 Cargo.toml 退出 101，已在 src-tauri 重跑。
Schema 默认值测试曾因 JSON 数字 2 与 Rust 序列化 2.0 的比较失败；修正 schema 后 13 项通过。
一次临时编辑脚本 cwd 错误未找到文件，随后在正确目录执行；没有修改其他路径。

内核全量失败名单（前四项是 [既有基线](baseline.md)，后六项是当前环境端口限制）：

```text
doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix
doctor::path::tests::prime_and_prepend_behaviour
doctor::verify::tests::install_root_groups_npm_entries_together
doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts
integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp
integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp
provider::claude_login::claude_oauth_tests::loopback_run_end_to_end_local
provider::claude_login::claude_oauth_tests::loopback_run_rejects_wrong_state
provider::store::local_route_tests::openai_provider_routes_via_local_router
provider::store::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

过滤复跑命令为 `cargo test -p polaris-kernel --lib --tests --offline --`，后接上述每个完整名称的 `--skip <名称>`。
不修改 baseline.md，也不把过滤后的成功当成原全量命令全部成功。

未运行真实 Docker、Windows 实机、浏览器按钮点击、真实模型测试。
范围澄清和 agentd 两项失败未解决前，P3b 保持 **BLOCKED**。
