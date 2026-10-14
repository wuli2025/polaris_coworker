# P5b 文件与验证清单

执行范围/行为/配置切换见 [迁移报告](p5b-indirect-callers.md)。以下只列本次自有改动，
不归属并行 doctor/R6 改动，不修改已有 `docs/execution/` 文件。未执行 commit/push。

## 最终验证

Rust 命令工作目录为 `src-tauri/`，vue-tsc 工作目录为仓库根。

| 实际命令 | 真实结果 | 日志 |
| --- | --- | --- |
| `cargo check -p polaris-app --offline` | 退出 0；0 警告 | [desktop](p5b-validation/desktop-check.log) |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0；0 警告 | [server](p5b-validation/server-check.log) |
| `cargo test -p polaris-kernel -p polaris-wiki -p polaris-fable --lib --tests --offline` | 退出 101；fable 105 通过/7 忽略；kernel 195 通过/6 TCP 失败/2 忽略，Cargo 在 kernel 后停止 | [未过滤](p5b-validation/tests-unfiltered.log) |
| 上条命令排除下面 6 项（最终重跑） | 退出 0；fable 105、kernel 195 lib + 8 integration、wiki 6，通过共 314；9 忽略、6 排除 | [过滤后](p5b-validation/tests-filtered.log) |
| `cargo test -p polaris-app --no-default-features --features server,collab-net --lib exec:: --offline` | 退出 0；12 通过；0 警告 | [远程执行](p5b-validation/exec-tests.log) |
| `npx vue-tsc --noEmit` | 退出 0；无诊断输出 | [vue-tsc](p5b-validation/vue-tsc.log) |
| `git diff --check` | 退出 0 | 无输出 |

最终三 crate 测试有 1 条既有 test-only warning：`F/fable/scan.rs:1016 use super::*` 在 Linux 未使用。
该文件及其 Windows-only 测试模块与 HEAD 相同，未在授权范围内修改。两壳 check 均为零警告。

排除项与实际重跑命令：

```bash
cargo test -p polaris-kernel -p polaris-wiki -p polaris-fable --lib --tests --offline -- \
  --skip integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp \
  --skip integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp \
  --skip provider::claude_login::tests::loopback_run_end_to_end_local \
  --skip provider::claude_login::tests::loopback_run_rejects_wrong_state \
  --skip provider::store::tests::local_route_tests::openai_provider_routes_via_local_router \
  --skip provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

前四项为 TCP bind EPERM；后两项无法绑定 8765–8789。未删测试、未改生产逻辑规避。

## 中间命令与未运行项

- 初次在仓库根误跑 `cargo check -p polaris-wiki --offline`，退出 101，根目录没有 Cargo.toml。
- 转到 src-tauri 后同一 check 曾因并行 doctor 的 ResolvedShell/as_deref 错误失败；
  [日志](p5b-validation/initial-wiki-check.log)。后续并行修复到达，两壳最终验证通过。
- `cargo test -p polaris-wiki --lib --offline` 初次 5 通过，随后新增启动失败用例，最终 6 通过。
- 最后补测时曾漏写测试函数的闭括号，编译退出 101；已修复并完整重跑，
  [中间日志](p5b-validation/intermediate-test-error.log)。
- `cargo test -p polaris-app --no-default-features --features server --lib exec:: --offline`
  12 通过，但无 collab-net 时产生既有 mesh.rs 的 4 条警告；
  [日志](p5b-validation/exec-server-only-tests.log)。之后按实际 server 特性组合重跑，0 警告。
- `rustfmt --edition 2021 --config skip_children=true <本次 wiki/exec 配置/Pi 测试文件>`
  未执行格式化：当前 stable 工具链没有 rustfmt，未联网安装组件；代码已人工检查缩进和行宽。
- 一个整理配置的 Python 脚本曾因在 src-tauri 内重复使用 src-tauri/ 前缀而报 FileNotFoundError；
  没有发生部分写入，随后用绝对路径成功执行。
- 未运行：真实模型/Claude 编译、真实 Docker、Windows/macOS 真机、前端点击验收、全量 agentd 测试。
  本任务的执行前权限依赖已有 agentd Permissions；仅新增 Rust 调用和适配，不宣称真实端到端验收。

除 brief 允许排除的 TCP 项和上述未运行的实机验证外，没有剩余实现阻塞。
此结论仅为执行报告，最终是否完成由 Claude 决定，Pi 独立审查。

## 文件、职责与物理行数

- wiki：入口、配置、进度、后端生命周期、旧分支、提示模板与行为测试拆开。
- kernel：新增 wiki 必需的公共输入/工具选项和详细事件；无头名字兼容转发。
- fable：名称/注释替换；两个获准路径测试拆出，按平台构造路径或 cfg，生产逻辑不变。
- exec：允许名单移至 JSON；加载/校验与原执行测试拆出；迁移的测试去掉 unwrap，保持断言。
- 文档：feature-tree 入口/依赖/配置/迁移/验证，以及本报告和原始日志。

| 文件 | 物理行数 | 最大行字符数 | 500 行约定 |
| --- | ---: | ---: | --- |
| `docs/feature-tree.md` | 414 | 369 | 符合 ≤500 行/每行 ≤500 字符 |
| `docs/pi-migration/p5b-indirect-callers.md` | 85 | 103 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-fable/src/fable/echo.rs` | 1076 | 165 | 既有长文件，仅名称/注释或获准测试移动；其余职责技术债 |
| `src-tauri/crates/polaris-fable/src/fable/files/llm.rs` | 1037 | 123 | 既有长文件，仅名称/注释或获准测试移动；其余职责技术债 |
| `src-tauri/crates/polaris-fable/src/fable/files/profile.rs` | 558 | 264 | 既有长文件，仅名称/注释或获准测试移动；其余职责技术债 |
| `src-tauri/crates/polaris-fable/src/fable/files/tests.rs` | 890 | 161 | 既有长文件，仅名称/注释或获准测试移动；其余职责技术债 |
| `src-tauri/crates/polaris-fable/src/fable/inventory/path_tests.rs` | 10 | 82 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-fable/src/fable/inventory/tests.rs` | 872 | 119 | 既有长文件，仅名称/注释或获准测试移动；其余职责技术债 |
| `src-tauri/crates/polaris-fable/src/fable/ontology.rs` | 694 | 140 | 既有长文件，仅名称/注释或获准测试移动；其余职责技术债 |
| `src-tauri/crates/polaris-fable/src/fable/retrieve/ai.rs` | 291 | 100 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-fable/src/kb/access.rs` | 468 | 97 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-fable/src/kb/access_path_tests.rs` | 48 | 80 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-fable/src/kb/enrich.rs` | 685 | 148 | 既有长文件，仅名称/注释或获准测试移动；其余职责技术债 |
| `src-tauri/crates/polaris-fable/src/kb/mod.rs` | 66 | 88 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/bridge.rs` | 69 | 164 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/mod.rs` | 173 | 172 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/extension_tests.rs` | 189 | 129 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs` | 188 | 146 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/sessions.rs` | 67 | 138 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/translate.rs` | 121 | 131 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/wiki_tests.rs` | 50 | 129 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-kernel/src/headless.rs` | 87 | 142 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-wiki/src/wiki/compile.rs` | 79 | 113 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-wiki/src/wiki/compile/config.rs` | 36 | 114 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-wiki/src/wiki/compile/legacy.rs` | 263 | 95 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-wiki/src/wiki/compile/progress.rs` | 73 | 120 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-wiki/src/wiki/compile/prompt-pi.md` | 30 | 54 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-wiki/src/wiki/compile/prompt.rs` | 47 | 140 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-wiki/src/wiki/compile/runner.rs` | 111 | 114 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/crates/polaris-wiki/src/wiki/compile/tests.rs` | 215 | 129 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/src/exec.rs` | 372 | 101 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/src/exec/config.rs` | 51 | 114 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/src/exec/defaults.json` | 53 | 22 | 符合 ≤500 行/每行 ≤500 字符 |
| `src-tauri/src/exec/tests.rs` | 201 | 98 | 符合 ≤500 行/每行 ≤500 字符 |

本清单自身和验证日志均不超过 500 物理行，每行不超过 500 字符。
既有长 fable 文件只修改调用名/注释；inventory 的触及测试已拆出，其他旧测试保留为技术债。
不为满足机械行数而压缩代码或拆出范围外业务。

| 报告/日志 | 物理行数 |
| --- | ---: |
| `docs/pi-migration/p5b-files-and-validation.md` | 118 |
| `docs/pi-migration/p5b-validation/desktop-check.log` | 6 |
| `docs/pi-migration/p5b-validation/exec-server-only-tests.log` | 119 |
| `docs/pi-migration/p5b-validation/exec-tests.log` | 77 |
| `docs/pi-migration/p5b-validation/initial-wiki-check.log` | 26 |
| `docs/pi-migration/p5b-validation/intermediate-test-error.log` | 23 |
| `docs/pi-migration/p5b-validation/server-check.log` | 7 |
| `docs/pi-migration/p5b-validation/tests-filtered.log` | 377 |
| `docs/pi-migration/p5b-validation/tests-unfiltered.log` | 381 |
| `docs/pi-migration/p5b-validation/vue-tsc.log` | 0 |
