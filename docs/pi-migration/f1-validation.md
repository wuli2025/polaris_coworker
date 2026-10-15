# F1 验证执行报告

状态：**BLOCKED（尚未满足三项目标测试均 15 轮零失败）**。
实现、配置和同目录排查见 [F1 时序加固](f1-timing.md)。最终是否完成由 Claude/Pi 决定。

## 实际执行

所有命令在受限沙箱中执行，未绕过限制、未请求提权、未 commit/push。
没有添加 skip/ignore，内核已有的两个 ignored 保持原样：
`doctor::install::tests::dump_install_scripts`、`integrations::nas::tests::real_machine_list`。

| cwd | 命令 | 真实结果 |
| --- | --- | --- |
| `agentd/` | `npm test`，连续 15 次 | 每次 exit 1；文件级 27 pass / 2 fail；失败始终为 e2e、web 两个文件 |
| `src-tauri/` | `cargo test -p polaris-kernel --lib`，连续 15 次 | 两个目标测试各 15/15 通过；每轮 exit 101；第 9/11 轮 217/7/2，其余 218/6/2（pass/fail/ignored） |
| `agentd/` | `npm run typecheck` | exit 0 |
| `agentd/` | `npm run build` | exit 0，0 警告 |
| `src-tauri/` | `cargo check -p polaris-kernel --offline` | exit 0，0 警告 |
| `src-tauri/` | `cargo check -p polaris-app --offline` | 桌面组合 exit 0，0 警告 |
| `src-tauri/` | `cargo check -p polaris-cli --bin polaris-server --offline` | server 组合 exit 0，0 警告 |
| `src-tauri/` | `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | server+collab-net 组合 exit 0，0 警告 |
| `agentd/` | `node --import tsx test/<name>.test.ts`，下列 9 文件逐个运行 | 全部 exit 0，共 35 个子测试通过、无取消 |
| 仓库根 | `git diff --check -- agentd/test src-tauri/crates/polaris-kernel/src/chat/backend docs/feature-tree.md` | exit 0 |

逐个运行的文件及通过数：concurrency 3、credential-lock 5、provider-credentials 3、
protocol 5、sandbox-status 6、sdk-contract 2、sessions 6、synchronization 2、timeouts 3。
Node 22.19 的 MockTimers 有 `ExperimentalWarning`，这是测试运行时提示；未屏蔽，
不把它混同为三种壳构建的编译警告。

`npm test` 在此环境只显示文件级汇总，故不能把 27 个通过文件写成 27 个通过子测试。
直接运行 9 个改动文件补充了可见的子测试断言证据，未用它替代全量复跑。

## 环境失败逐项说明

agentd：

- `main.ts subprocess speaks JSONL through a real SDK tool round and shuts down via EOF/SIGTERM`：
  等首个 ping 时子进程 close，code=0，stdout/stderr 均空。最小独立探针
  `spawnSync(process.execPath, ['-e', 'console.log("child-ready")'])` 返回 `error.code=EPERM`，
  async spawn 同样无输出即 close。环境中的 Node 管道行为异常可独立复现，
  但本沙箱无法完成真实 JSONL E2E 验证，**这不是通过，也不能只算 TCP 例外**。
- `local HTTP adapter contract: search credential, bounds, timeout and HTML text`：
  `listen EPERM: operation not permitted 127.0.0.1`。同文件无 socket 的 2 个测试直接运行通过。

内核 TCP 限制共 6 项：

1. `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`
2. `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`
3. `provider::claude_login::tests::loopback_run_end_to_end_local`
4. `provider::claude_login::tests::loopback_run_rejects_wrong_state`
5. `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`
6. `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`

前 4 项是 `bind`/`listen` 的 `PermissionDenied / Operation not permitted`；
后 2 项是路由器不能绑定 8765–8789（上层文案“都被占用”，此沙箱禁止监听）。
没有改这些范围外测试或按名称排除它们。

额外发现（不是 TCP 例外）：第 9、11 轮
`chat::backend::docker_tools::status_tests::preserves_failure_output_normal_exit_and_signal`
失败，退出码断言已通过，但 signal 得到 None：第 9 轮预期 Some(9)，第 11 轮预期 Some(15)。
其后用完整测试名 `cargo test -p polaris-kernel --lib
chat::backend::docker_tools::status_tests::preserves_failure_output_normal_exit_and_signal -- --exact`
单独复跑 exit 0，1/1 通过。
源码中 `sandbox_command.sh` 使用 EXIT marker 与 shell wait stderr 区分正常高退出码和信号；
返回 None 表明这一分类条件未成立。尚未取得足以确定是 EXIT marker 还是 wait 诊断造成差异的证据，
不能把它归因于本轮修复的 idle 窗口，也不能把单独复跑通过当成已修复。
没有改生产信号分类或削弱 signal 断言；该项作为新增未解决问题交给主控复核。

## 基线和开发过程

- 改动前 `npm test`：exit 1，26 个文件通过、e2e/web 失败。
- 改动前 `cargo test -p polaris-kernel --lib`：exit 101，217 pass / 7 fail / 2 ignored；
  除 6 项 TCP 外，实际复现 idle config-apply 测试的 `SESSION_NOT_FOUND`。
- 首次修改后 `cargo test -p polaris-kernel --lib chat::backend::`：exit 0，86 pass。
- Node 第一次全量开发检查：exit 1，除环境失败外还暴露虚拟时钟推进顺序错误。
  定向运行 timeouts 的中间版本出现 idle/max 错误类别不符、active pending 被取消；
  最终用真实 readiness poll、先派发 SDK progress 再推进时钟、tick(0) 等真实终态修复。
  最终该文件直接运行 3/3，通过后的全量 15 轮中未再失败。
- 曾在仓库根误运行 `node --import tsx test/timeouts.test.ts`，exit 1（路径/cwd 错误）；
  随即在 `agentd/` 重新执行。该次不计入最终 15 轮。
- `node --import tsx test/e2e.test.ts`：exit 1，提前关闭诊断；
  `node --import tsx test/web.test.ts`：exit 1，2 pass / 1 TCP fail。
- 构建、类型检查、最终直接测试日志和基线日志随逐轮日志保留；
  中间开发日志另在 `/tmp/polaris-f1/`，未将中间失败从执行历史描述中隐去。

## 未完成和不作出的声明

- 不能声明 agentd E2E 连续 15 轮零失败；需 Claude 在支持子进程 stdio 的执行环境重跑。
- 全量 TCP 项需要可监听 loopback 的环境，沙箱中不能核实真实联网结果。
- DockerTools 的 SIGKILL/SIGTERM 分类偶发失败尚未修复；详见第 9、11 轮日志及上面的复跑结果。
- 未运行 UI/移动端测试、真实 Docker、Windows/macOS 或真实外部模型；本次没有这些模块改动。
- 新辅助代码和改动文件均执行 500 物理行/500 字符检查；feature-tree 保持在 499 行。
- 本次没有生产代码就绪信号改动；Rust 新 hooks 仅 `cfg(test)`，发布构建不携带测试控制协议。

## 连续复跑证据

每轮日志记录原始完整命令输出，summary 记录真实退出码和耗时；运行中未串行化测试用例。
两个循环各自连续运行；早期轮次还与另一套测试和构建同时运行，保留了并发压力。
最终结果：两个内核目标各 15/15 通过；agentd E2E 仍为 0/15，受上述环境阻塞。

| 轮次 | npm test：日志、退出码、耗时 | kernel：日志、退出码、耗时 | kernel pass/fail/ignored | 两个内核目标 |
| --- | --- | --- | --- | --- |
| 01 | [日志](f1-validation/agentd-01.log)，1，14.97s | [日志](f1-validation/kernel-01.log)，101，41.93s | 218/6/2 | 均通过 |
| 02 | [日志](f1-validation/agentd-02.log)，1，11.67s | [日志](f1-validation/kernel-02.log)，101，34.97s | 218/6/2 | 均通过 |
| 03 | [日志](f1-validation/agentd-03.log)，1，11.26s | [日志](f1-validation/kernel-03.log)，101，41.09s | 218/6/2 | 均通过 |
| 04 | [日志](f1-validation/agentd-04.log)，1，14.46s | [日志](f1-validation/kernel-04.log)，101，32.16s | 218/6/2 | 均通过 |
| 05 | [日志](f1-validation/agentd-05.log)，1，14.55s | [日志](f1-validation/kernel-05.log)，101，32.24s | 218/6/2 | 均通过 |
| 06 | [日志](f1-validation/agentd-06.log)，1，18.58s | [日志](f1-validation/kernel-06.log)，101，31.93s | 218/6/2 | 均通过 |
| 07 | [日志](f1-validation/agentd-07.log)，1，15.09s | [日志](f1-validation/kernel-07.log)，101，31.09s | 218/6/2 | 均通过 |
| 08 | [日志](f1-validation/agentd-08.log)，1，12.68s | [日志](f1-validation/kernel-08.log)，101，30.97s | 218/6/2 | 均通过 |
| 09 | [日志](f1-validation/agentd-09.log)，1，13.68s | [日志](f1-validation/kernel-09.log)，101，30.94s | 217/7/2 | 均通过 |
| 10 | [日志](f1-validation/agentd-10.log)，1，13.01s | [日志](f1-validation/kernel-10.log)，101，30.92s | 218/6/2 | 均通过 |
| 11 | [日志](f1-validation/agentd-11.log)，1，15.56s | [日志](f1-validation/kernel-11.log)，101，30.81s | 217/7/2 | 均通过 |
| 12 | [日志](f1-validation/agentd-12.log)，1，14.85s | [日志](f1-validation/kernel-12.log)，101，30.45s | 218/6/2 | 均通过 |
| 13 | [日志](f1-validation/agentd-13.log)，1，16.64s | [日志](f1-validation/kernel-13.log)，101，30.46s | 218/6/2 | 均通过 |
| 14 | [日志](f1-validation/agentd-14.log)，1，13.85s | [日志](f1-validation/kernel-14.log)，101，30.46s | 218/6/2 | 均通过 |
| 15 | [日志](f1-validation/agentd-15.log)，1，14.40s | [日志](f1-validation/kernel-15.log)，101，30.45s | 218/6/2 | 均通过 |

其他证据：

- [agentd 汇总](f1-validation/agentd-summary.json)、[kernel 汇总](f1-validation/kernel-summary.json)。
- [构建命令及退出码](f1-validation/build-summary.json)、[桌面](f1-validation/desktop-check.log)、[server](f1-validation/server-check.log)、[server+collab-net](f1-validation/server-collab-check.log)、[kernel check](f1-validation/kernel-check.log)。
- [TypeScript 检查](f1-validation/typecheck-final.log)、[agentd build](f1-validation/agentd-build.log)、[9 个直接测试汇总](f1-validation/direct-summary.json)（对应 direct-*.log 均同目录）。
- [子进程最小探针](f1-validation/subprocess-sandbox.log)、[E2E 直接错误](f1-validation/e2e-development.log)、[web 直接错误](f1-validation/web-sandbox.log)、[信号单独复跑](f1-validation/docker-signal-recheck.log)。
- [agentd 基线](f1-validation/agentd-baseline.log)、[kernel 基线](f1-validation/kernel-baseline.log)、[后端定向 86 项](f1-validation/kernel-development.log)。
- [改动源码行数、最长行与 SHA-256](f1-validation/source-files.json)；仅涵盖本次 23 个代码文件及 feature-tree/实现报告，不把工作区其他用户改动算为 F1。
