# P2 实测记录

2026-10-14，当前沙箱，未使用网络、没有修改 baseline.md。
Cargo 命令在 `src-tauri/` 执行；前端类型命令在仓库根执行。

| 命令 | 退出码 | 原文摘要 / 完整日志 |
| --- | --- | --- |
| `cargo check --offline -p polaris-app` | 0 | ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.97s``；[原日志](p2-validation/app-check.log) |
| `cargo check --offline -p polaris-cli --bin polaris-server --features collab-net` | 0 | ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.32s``；[原日志](p2-validation/server-check.log) |
| `cargo test --offline -p polaris-kernel --lib --tests` | 101 | `test result: FAILED. 128 passed; 10 failed; 2 ignored; 0 measured; 0 filtered out; finished in 1.30s`；[原日志](p2-validation/kernel-tests.log) |
| `cargo test --offline -p polaris-kernel --lib chat::backend` | 0 | `test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 112 filtered out; finished in 1.02s`；[原日志](p2-validation/backend-tests.log) |
| `cargo test --offline -p polaris-kernel --test backend_config` | 0 | `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s`；[原日志](p2-validation/config-tests.log) |
| `npx vue-tsc --noEmit` | 0 | 无 stdout/stderr；[空日志](p2-validation/vue-tsc.log) |

完整内核命令在 lib 测试失败后停止，因此 backend_config 集成测试另跑，未假称该命令整体通过。
28 项后端测试包括迁移后的原管线/池测试，以及新增 Pi/legacy headless/config 测试，不是全部新写的测试。
桌面/服务端编译均为 6 个既有 doctor 警告；测试构建为 3 个既有 doctor 警告。

## 四个原基线失败

```text
doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix
doctor::path::tests::prime_and_prepend_behaviour
doctor::verify::tests::install_root_groups_npm_entries_together
doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts
```

## 六个额外失败（不计入基线豁免）

```text
integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp
integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp
provider::claude_login::claude_oauth_tests::loopback_run_end_to_end_local
provider::claude_login::claude_oauth_tests::loopback_run_rejects_wrong_state
provider::store::local_route_tests::openai_provider_routes_via_local_router
provider::store::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

这六项所在源文件没有修改。前四项原文错误：

```text
Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

后两项原文错误：

```text
无法在 8765–8789 间绑定本地端口(都被占用)
```

独立用 Python 创建普通 TCP socket，也在 `socket.socket()` 直接得到：

```text
PermissionError: [Errno 1] Operation not permitted
```

证据指向当前沙箱禁用 socket 创建，不能宣称满足“仅允许四项失败”的验收条件。
没有跳过、忽略或改写这些测试来制造绿色结果。

## 其他检查与限制

- `git diff --check`：通过，无输出。
- 改动范围、每文件 ≤500 行、每行 ≤500 字符：脚本检查通过，逐项见 [文件清单](p2-files.md)。
- 生产新增 backend 代码未使用 `.unwrap()` / `.expect()`；测试可使用。
- `rustfmt` 尝试失败，原文：`error: 'rustfmt' is not installed for the toolchain 'stable-x86_64-unknown-linux-gnu'.`
- 只启用已存在的 Tokio `process`、windows-sys Job/Security 特性；无新增 crate，无 Cargo.lock 改动。
- 未运行真实模型调用、Windows 实测、完整账号隔离测试；不将假 daemon 测试计作这些验证。
