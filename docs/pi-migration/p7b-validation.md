# P7b 实际验证记录

状态：**BLOCKED**。自定义 CA 未实现；必测 Rust 全量命令非零退出。以下是执行报告，不是 Claude/Pi 最终验收。

## 最终检查

Rust 命令工作目录为 `src-tauri/`；其它未注明命令在仓库根目录。

| 实际命令 | 真实结果 |
| --- | --- |
| `cargo check --offline` | 通过，0 warning；desktop 默认特性 |
| `cargo check --offline --no-default-features --features server` | 通过，0 warning |
| `cargo check --offline --no-default-features --features server,collab-net` | 通过，0 warning |
| `cargo test -p polaris-kernel -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | **失败，exit 101**；collab 176 passed；kernel 221 passed / 6 failed / 2 ignored。kernel lib 失败后 Cargo 未继续执行后续 integration targets |
| `cargo test -p polaris-kernel --offline --lib chat::` | 通过，103 passed / 127 filtered；包含最后新增的 done/lease 生命周期用例 |
| `npx vue-tsc --noEmit` | 通过，exit 0，无输出 |
| `npx vitest run` | 通过，24 files / 118 tests；其中 production packaging 回归实跑约 8 秒，不是 mock/skip |
| `node scripts/pack-agentd.mjs /tmp/p7b-agentd` | 通过，exit 0；契约自检、photon/quickjs-wasi 与 pi-tui linux-x64 加载成功；产物 123500408 bytes |
| `node scripts/runtime/contract-files.test.mjs` | 通过，2 tests；嵌套 re-export 帮助模块在移除源目录后仍可执行；当前 SDK helper 被收集 |
| `node --test scripts/runtime/contract-files.test.mjs` | 通过，exit 0（Node 汇总显示 1 test file） |
| `node --import tsx --test test/sessions.test.ts`（`agentd/`） | 通过，exit 0；包括新增 steer/followUp 顺序及空闲拒绝断言 |
| `node --check drills/d1.mjs`；`node --check drills/lib/live-input.mjs` | 通过，exit 0；仅语法验证，不等同真实演练 |
| `git diff --check` | 通过 |
| Python 读取 `git diff --name-only` 与新增文件，核对行数/长度/净增；扫描新增 Rust `unwrap/expect` | 通过；保留用户既有 `docs/execution/P7b-1/`，不计作本次自有文件；ChatComposer +4、chat store +1；其它自有文件 ≤500 行、行长 ≤500 |

额外用 Node fs 检查 `/tmp/p7b-agentd/.contract-tests` 和 `dist/test` 均不存在，输出 `production test directories absent`。完整生产打包回归 `scripts/runtime/pack-agentd.test.mjs` 也对两目录以及残留 staging 做断言，并清理自己的临时产物。

生产安装输出一条既有 `node-domexception@1.0.0` npm deprecated 提示；三个 Cargo check 日志没有 warning。

## Rust 全量失败明细

- `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`
- `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`
- `provider::claude_login::tests::loopback_run_end_to_end_local`
- `provider::claude_login::tests::loopback_run_rejects_wrong_state`
- `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`
- `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`

前四项记录 `PermissionDenied / Operation not permitted`（本地 listener/假上游绑定）；后两项报无法在 `8765–8789` 绑定路由端口。没有把这些项目删除、跳过或改成通过。当前执行环境不允许提权；需要 Claude 在允许本地端口绑定的环境重跑原必测命令。

必测全量执行后又新增了 1 项纯 registry 生命周期测试，所以最后 chat 定向测试为 103 项；没有因全量已知沙箱失败而将其称为全量通过。

## 已处理的验证问题

- 最初 pack helper 测试用捕获 stdio 的 `execFileSync`，本环境返回 EPERM；改用继承 stdio 后真实 Node 子进程通过。
- 新测试与既有 `drills/test/harness.test.mjs` 使用 node:test，最初完整 Vitest 报无 suite；按仓库已有 runtime 测试模式兼容两种 runner，最后全量通过。
- 前端 mock 测试 beforeEach 最初返回 mock 函数，被 Vitest 当作 teardown 执行；已改为无返回 block，最后通过。
- 曾在仓库根目录误运行未带 manifest 的 cargo 命令，以及在根目录误运行 agentd 的相对测试路径，均失败；修正工作目录后结果如表，未把误运行当通过。

## 未运行或未完成

- 自定义 `authority.extraCaPem`、统一 CA HTTP Agent、本地测试 CA HTTPS 服务测试及 D4 TLS 修改均未实现。理由和不新增依赖方案见 [P7b 产品缺口报告](p7b-product-gaps.md)。没有任何禁用证书校验路径。
- D1 的真实模型 steer/followUp 演练未运行；脚本已改为真实断言，需显式 staging 模型凭据和可运行产品 server 的演练环境。
- D4 Docker/权威/relay 真 TLS 演练未运行；不把 curl 或系统 CA 成功当作 ureq 成功。
- Tauri GUI 人工操作、真实多人 HTTP 控制演练未运行；当前验证为命令编译注册、kernel 归属/actor 行为测试及前端 transport 测试。
- Rust 后续 integration targets 未补跑；全量命令在 kernel lib 的六项失败处停止。
- 未执行 release build、commit 或 push。

本次原始输出暂存 `/tmp/p7b-{check-desktop,check-server,check-net,rust-tests,chat-tests,vue,vitest,pack,pack-tests,agentd-tests,payload}.log`。该路径为临时证据；本文件保留核心命令、计数和失败原因。
