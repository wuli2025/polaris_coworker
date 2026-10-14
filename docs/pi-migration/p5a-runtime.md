# P5a 随包运行时、doctor、Docker 执行报告

状态：BLOCKED（部分实现，不能作为模块验收通过声明）。未提交、未推送。
执行环境：Linux x64 / Node 22.19.0 / 2026-10-14。Claude 需独立重跑，Pi 独立审查。

## 入口与依赖

- 桌面 `tauri.conf.json.beforeBuildCommand` → `scripts/fetch-runtimes.mjs` → `scripts/pack-agentd.mjs`。
- Node 默认版本取 `agentd/package.json.engines.node` 下限 `22.19.0`；支持 Windows/macOS/Linux 的 x64/arm64。
- 官方便携包使用 R2 → 既有公共代理 → nodejs.org 顺序；SHA256 只信 Node 官方 HTTPS `SHASUMS256.txt`。
- 下载在解压前校验魔数、尺寸、精确文件名对应的 SHA256；不匹配换源，官方清单不可用则失败。
- `.stamp.json` 按平台、架构和组件版本判断是否复用；旧版已存在的二进制不会掩盖升级。
- agentd 生产包执行 `npm ci --omit=dev --ignore-scripts --no-audit --no-fund`，不改锁文件或依赖版本。
- 打包包含 `dist/src`、`config`、包元数据与生产 `node_modules`，不带 agentd 测试、源码或根开发依赖。
- 打包目录重跑编译后的 `sdk-contract.test`，实际调用 photon、QuickJS，并加载当前平台 pi-tui 原生模块。
- `bundle.resources` 纳入 `runtime/**/*`；Tauri setup 注入资源目录，支持 Linux 资源布局。
- Rust `config::runtime_paths` 为公开、可测解析函数：显式配置优先，其次随包文件，debug 开发态回落仓库 `agentd/dist/src/main.js`。
- 未构建开发产物时 doctor 报入口缺失；需先 `cd agentd && npm run build`。release 不回落源码入口。
- `doctor::pi` 诊断 Node 最低版本/可执行性、agentd 入口、受超时和行长限制的真实 JSONL ping、协议/Pi 版本、P4 共享凭据布尔状态。
- 不发送模型请求、不输出凭据内容。凭据布尔值说明有 API key 或 OAuth access+refresh，不承诺上游仍接受该凭据。
- Pi autopilot 只诊断；Claude 安装、更新、更新检查、PATH 修复受 `agent.backend=claude-cli` 守卫。
- 旧 Node 安装器版本较低，仅旧后端可调用；Pi 依靠随包 Node 或显式配置。
- `/api/status.agent_runtime` 复用 `agent_backend_status`；公开 `/api/health` 保持常量时间存活探针。
- backend status 是运行中 supervisor 的快照，尚未启动的 backend 可以为 unhealthy；doctor 的独立 ping 不会伪造 supervisor 健康状态。

## 配置、类型和切换示例

| 配置 | 默认/类型 | 切换示例 |
| --- | --- | --- |
| `POLARIS_NODE_VERSION` | engines 下限，严格 x.y.z，不低于下限 | `POLARIS_NODE_VERSION=22.19.0 node scripts/fetch-runtimes.mjs` |
| `POLARIS_DEPS_BASE` | `https://llmwiki.cloud/downloads/deps`，HTTPS URL | 指向已同步同名官方包的 R2/CDN |
| `POLARIS_RUNTIME_DIR` | Tauri 资源目录的 runtime，可由部署显式覆盖 | `/opt/polaris/runtime` |
| `POLARIS_AGENT_CONFIG` | 可选 JSON 文件，既有严格 serde 校验 | `{"agent":{"backend":"pi"},"agentd":{"node":"node","entry":"/opt/agentd/dist/src/main.js"}}` |
| `POLARIS_AGENTD_NODE` / `ENTRY` | 覆盖默认解析路径，字符串 | `/usr/local/bin/node` / `/app/runtime/agentd/dist/src/main.js` |
| `POLARIS_AGENTD_ARGS` / `NODE_ARGS` | JSON 字符串数组，默认 `[]` | `POLARIS_AGENTD_ARGS='["--config","/app/agentd.json"]'` |
| `POLARIS_AGENTD_ISOLATION` | 桌面 shared；server per-account | 可信单用户部署显式 `shared`；不改变 P2 的 OS 隔离拒绝规则 |
| Docker `NODE_VERSION` | `22.19.0`，打包脚本验证 engines | `--build-arg NODE_VERSION=22.19.0` |
| Docker `INSTALL_CLAUDE` | `false`，仅接受 true/false | `--build-arg INSTALL_CLAUDE=true`，运行时设 `POLARIS_AGENT_BACKEND=claude-cli` |

Compose 以 `POLARIS_NODE_VERSION`、`POLARIS_INSTALL_CLAUDE` 传构建参数。
agentd 凭据复用 `~/Polaris/data/providers.json`（polaris-data 卷），会话在 `~/.polaris/agentd`（polaris-pi 卷）。
旧 polaris-claude 卷原样保留；无用户数据迁移或删除。切回旧后端需选择包含 Claude 的镜像。
Docker agent 构建层和运行层补 `libxcb1`，满足 pi-tui Linux 原生模块依赖。

## Windows shell：有依据，但接线被范围阻塞

Pi [Windows 文档](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/windows.md)
定义查找顺序：`shellPath` → Program Files Git Bash → PATH `bash.exe`。
[Settings 文档](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/settings.md) 的 `shellPath` 是自定义 shell 可执行文件路径。
本地锁定的 `@earendil-works/pi-coding-agent@1.0.4/docs/windows.md:21`、`docs/settings.md:139` 与该接口一致。

agentd 当前 `src/sessions/create.ts` 使用 `SettingsManager.inMemory`，因此写用户 settings.json 不会接通该 daemon。
真正实现需要：agentd config 增加 shellPath，createSession 传给 SettingsManager；Rust `chat/backend/pi/process.rs` 注入 bundled PATH 和 shell 路径。
这些生产源码不在任务的 agentd「仅构建脚本」白名单内，Rust process.rs 也不在白名单。
已请求仅扩展这两处接线范围，当前没有收到授权；没有修改这些文件或用启动钩子绕过边界。
当前 Pi doctor 不把未接入 daemon 的 MinGit 当作 shell 已就绪，防止打包 Windows 错报 ready。
MinGit 下载与既有 `bundled::child_path_prefix` 保留，并修复 stamp 完整性检查遗漏 bash.exe 的情况。

## 体积、组成与裁剪评估

Linux x64 agentd 生产包：123,498,400 字节（约 117.78 MiB，未压缩）。

| 组成 | 字节 |
| --- | ---: |
| dist/src | 219,787 |
| config | 2,240 |
| node_modules（121 个安装包） | 123,195,390 |
| package.json / lock / stamp | 80,983 |

以上不包含 Node、uv、Python、MinGit，不能当作完整桌面安装包体积。
Node 官方包尚未下载：当前代理 `127.0.0.1:7897` 无法连接。完整跨平台体积为 BLOCKED。
photon 包 2,265,687 字节；quickjs-wasi 包 1,524,119 字节。
Linux x64 已验证 photon 构造/释放图像、QuickJS `1+2=3`、pi-tui 原生模块加载。
pi-tui 自带 win32/darwin/linux 各 x64/arm64 六份预编译模块；其他平台仅核实文件存在，未宣称实跑通过。
所有 WASM 与原生模块均保留；QuickJS extensions 的 `.so` 也保留。

可后续评估裁剪：source maps 共 30,920,637 字节；`.d.ts` 共 15,054,718 字节；SDK examples（含 Doom WASM）、文档及非当前平台原生模块。
本次不裁剪生产依赖内部文件，避免破坏扩展加载、运行时资源定位与 SDK 契约；不能仅凭扩展名认定可删。
仅省去 agentd 自身 dist/test 和开发依赖，打包前后 SDK 契约测试均通过。

## 纯搬移与行为修改

纯搬移：install 内联 shell/PowerShell 模板 → install/*.sh/*.ps1，流式执行 → install/stream.rs，测试 → install/tests.rs；
verify → helpers/steps/conflicts/tests；probe 基础探测 → probe_helpers.rs；path 测试 → path_tests.rs；server 运维采集 → apihub/server_status.rs。
模板正文保持逐字等价，`include_str!` 保留原有字符串内容。
行为修改另计：Pi 分流与旧后端守卫、PATH 分隔符按平台处理、Windows 测试路径用跨平台分隔符、运行时默认路径、打包与 Docker 配置、健康字段。
原 install 1118 / verify 1270 / probe 706 行已拆到每文件 ≤500；path 也拆出测试。
`src/tauri.ts` 为任务显式例外，净增 5 行；新契约单独放 `src/types/doctor.ts`。

## 验证结果与限制

命令运行目录：默认仓库根；Cargo 用 `--manifest-path src-tauri/Cargo.toml`（等价于在 src-tauri 下运行）；agentd 命令在 agentd 下运行。

| 实际命令 | 真实结果 |
| --- | --- |
| `npm run typecheck && npm test`（agentd） | typecheck 成功；25 测试文件中 23 通过、2 失败 |
| `node --import tsx --test --experimental-test-isolation=none test/e2e.test.ts test/web.test.ts` | 2 用例通过、2 失败；HTTP `listen EPERM 127.0.0.1`；E2E 等 ping 超时，子进程 exit=0 |
| `npm_config_cache=/tmp/polaris-npm-cache npm_config_offline=true node scripts/pack-agentd.mjs` | 成功；生产树 SDK 契约通过，photon/QuickJS/原生模块实跑通过；再次运行 stamp 复用 |
| Python Popen 系统管道发送 ping（开发 dist 与打包 dist 各一次） | 两者成功：agentd 0.1.0、Pi 1.0.4、protocolVersion 6；正常退出 0 |
| `cargo check --offline` | desktop 成功，有既有/搬移后的 warnings |
| `cargo check -p polaris-cli --offline` | server 成功，有 warnings |
| `cargo test -p polaris-kernel --lib --tests --offline` | 186 通过、6 失败、2 忽略；lib 失败使 Cargo 未继续集成测试 |
| 上述命令追加下列 6 个精确 `--skip` | lib 186 通过、2 忽略；集成测试 8 通过；零失败 |
| `cargo test -p polaris-kernel --offline --lib doctor::` | 32 通过、1 忽略；Pi 就绪/缺失/协议错/超时/旧动作守卫/不自动安装测试通过 |
| `npx vue-tsc --noEmit` | 成功 |
| `npx vitest run` | 9 文件、29 测试通过，含离线 Node 版本/校验和/平台/stamp 测试 |
| `git diff --check` | 成功 |
| `docker compose config --quiet` | 失败：当前 WSL 未启用 Docker Desktop integration |
| `curl --max-time 10 -fsSL https://nodejs.org/dist/v22.19.0/SHASUMS256.txt` | 失败：代理 127.0.0.1:7897 不可连接 |

6 个已观察到的 TCP 沙箱失败，排除项完整名称：

- `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`
- `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`
- `provider::claude_login::tests::loopback_run_end_to_end_local`
- `provider::claude_login::tests::loopback_run_rejects_wrong_state`
- `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`
- `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`

基线四项 Windows 路径失败：claude_dir_from_path、install_root_groups_npm_entries_together、single_npm_install_reports_no_conflict、two_installs_with_different_versions_report_both_conflicts 均通过。
另修复 path_contains_dir 在 Unix 仍按分号切 PATH 的真实问题；Windows 专属大小写测试加 cfg(windows)。

agentd E2E 限制已用最小 Node spawn→stdin 示例重现：子 Node 收不到父 Node 写入的数据；Python/Rust 系统管道成功。
这仍是失败，未把它计为通过，也未修改测试来规避。需 Claude 在正常管道环境独立重跑。
未运行：真实 Node 便携包下载/解压；Windows/macOS 安装包与 shell 实跑；Docker build、运行镜像与 PowerShell docker-smoke（当前无 pwsh）。
初次离线 npm 打包因空缓存 ENOTCACHED 失败；复制已有 npm 缓存至可写 /tmp 后成功，没有下载或更新依赖。
初次 Cargo 从仓库根未指定 manifest 失败；已在正确 manifest 下完成上述检查。
Vitest 初次把 node:test 文件当空套件失败；已改为现有 Vitest API，29 项全绿。

## P5c 前端配合

- 读取 `EnvReport.agentRuntime`；依据 backend 和 legacyActions 隐藏/显示旧 Claude 安装、更新、PATH 修复与旧 Node 安装按钮。
- 显示 Node/入口/ping/协议/Pi 版本及 credentialsConfigured「已配置/未配置」，不请求或展示凭据明文。
- Pi 校验不会调用模型；`deep=false` 如实表示未执行计费的端到端推理，不沿用 Claude 冒烟文案。
- 使用顶层 ready 加 shellReady；Pi autopilot 不再承诺后台安装 Claude。
- 保留旧字段作为类型兼容，Pi 中 claude.required=false；健康运维读取 `agent_runtime`，停止读取已移除的 `claude_config`。
- 浏览器 mock 添加 agentRuntime，明确标记 BrowserPreview，不假报就绪。
