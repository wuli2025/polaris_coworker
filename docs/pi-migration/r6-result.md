# R6：P5a 审查修复执行报告

按 `docs/reviews/P5a-round1-triage.md` 仅处理「中 2（高）」「轻 1」及空环境变量文档约定。
无新依赖、无 commit/push；保留其他任务的现有及并行改动。本报告供 Claude 重跑与 Pi 独立审查，不代替最终验收。

## 改动文件与行为

| 文件 | 行为 |
| --- | --- |
| `src-tauri/crates/polaris-kernel/src/doctor/pi_shell.rs` | resolve 返回 ResolvedShell（path + ShellSource）；Windows PATH 遍历候选，排除 SystemRoot 整个目录树及 WindowsApps 目录中的 bash；无候选时 configure 返回 AgentShellMissing，阻止 Pi 再次自动回落 |
| `src-tauri/crates/polaris-kernel/src/doctor/pi.rs` | report/verify 保留同一次解析结果，shell detail 显示实际来源和路径；缺失时 shellReady/ready=false、shell 步骤 fail，并提示排除规则 |
| `src-tauri/crates/polaris-kernel/src/doctor/pi_shell_tests.rs` | 随包、两个 Program Files、PATH、显式配置及无候选；真实临时文件模拟 System32/SysWOW64/Sysnative/WindowsApps；被排除候选后继续选择 Git；大小写、斜杠、扩展路径前缀、点目录、目录边界 |
| `src-tauri/crates/polaris-kernel/src/doctor/pi_tests.rs` | 四种来源的 detail；其他组件齐全但 shell 缺失时整体不就绪、错误码与失败步骤 |
| `src-tauri/crates/polaris-kernel/src/doctor/path_tests.rs` | cfg(not(windows))：冒号分隔、完整目录匹配、空值、尾斜杠、含空格和分号的目录 |
| `docs/pi-migration/p5a2-closeout.md` | 配置章节明确空环境变量视同未设置，记录根目录默认值、来源与恢复自动选择示例 |
| `docs/feature-tree.md` | R6 入口、依赖、配置、迁移、验证索引 |
| `docs/pi-migration/r6-result.md`、`r6-validation/*.log` | 本执行报告及五份实际验证日志 |

来源枚举为 bundled / program_files / path / explicit；bundled/program_files 仅 Windows 与测试构建使用。
Windows 系统根读取非空 SystemRoot → windir → 默认 C:/Windows；测试直接注入根目录与候选，不修改全局环境。
目录过滤忽略大小写、统一斜杠并处理点目录，以目录组件判断边界，不误伤 WindowsAppsGit 等相似名称。
显式配置优先级及失效不回落的约定保持；Linux /bin/bash 优先级、中文/空格路径处理、下载源等驳回项未改代码。

## 配置与迁移

无新增用户配置或报告字段，无需迁移旧配置。沿用 shellPath（可省略、null 或 string，默认 null，其他类型返回 AgentdShellPathInvalid）。
非空 JSON shellPath 优先于 POLARIS_AGENTD_SHELL_PATH；该空环境变量视同未设置。
显式示例：`{"shellPath":"C:/Tools/Git/bin/bash.exe"}`。
恢复自动选择：`{"shellPath":null}` 并清空或移除 POLARIS_AGENTD_SHELL_PATH。
Unix 系统 shell 记为 path；shellBundled 保持原有路径归属语义，detail 的来源独立记录选择分支。

## 实际验证

以下 Cargo 命令均在 `src-tauri/` 执行，日志包含 stdout/stderr。

| 实际命令 | 退出码与真实结果 | 日志 |
| --- | --- | --- |
| `cargo test -p polaris-kernel --lib doctor:: --offline` | 0；39 通过、0 失败、1 忽略；0 警告 | [doctor](r6-validation/doctor.log) |
| `cargo check --offline` | 0；桌面壳通过，0 警告 | [desktop](r6-validation/desktop-check.log) |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 0；服务端壳通过，0 警告 | [server](r6-validation/server-check.log) |
| `cargo test -p polaris-kernel --lib --tests --offline` | 101；195 通过、6 个 TCP 沙箱失败、2 忽略；该次未继续集成测试 | [完整测试](r6-validation/kernel-tests.log) |
| 上述命令仅追加下方六个完整名称的 `--skip` | 0；单元 195 通过、0 失败、2 忽略、6 过滤；集成 8 通过、0 失败；0 警告 | [排除 TCP 后](r6-validation/kernel-filtered.log) |

排除重跑的完整命令：

```sh
cargo test -p polaris-kernel --lib --tests --offline -- \
  --skip integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp \
  --skip integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp \
  --skip provider::claude_login::tests::loopback_run_end_to_end_local \
  --skip provider::claude_login::tests::loopback_run_rejects_wrong_state \
  --skip provider::store::tests::local_route_tests::openai_provider_routes_via_local_router \
  --skip provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

前四项报 `PermissionDenied / Operation not permitted`；后两项报无法在 8765–8789 绑定本地端口。
忽略项保留：`doctor::install::tests::dump_install_scripts`、`integrations::nas::tests::real_machine_list`，未运行。
未跳过任何非 TCP 失败，也未修改相关测试。

其他实际检查：`git diff --check -- src-tauri/crates/polaris-kernel/src/doctor docs/feature-tree.md docs/pi-migration` 退出 0。
Python 文件扫描确认本次所有改动/新增文件 ≤500 物理行、每行 ≤500 字符；增量 Rust 代码扫描无 unwrap()/expect()。

## 未完成与限制

TCP 六项在本沙箱中未通过，按 brief 的明确例外列出；需在允许本地 TCP 绑定的环境复跑。
未运行 Windows 原生编译、安装包及真实 shell 启动；Windows 选择逻辑在 Linux 通过候选列表和系统根注入验证。
未运行本任务范围外的前端、agentd、Docker 测试。本次限定修复无其余已知实现阻塞；最终完成状态由 Claude 决定。
