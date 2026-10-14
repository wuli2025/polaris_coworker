已落实 R6 限定修复，未 commit/push，保留其他用户改动。

- `pi_shell.rs`：排除 Windows 系统目录和 WindowsApps 候选；返回来源枚举；无可用 shell 时阻止启动。
- `pi.rs`：shell detail 显示实际来源和路径，缺失时报告不就绪。
- `pi_shell_tests.rs`、`pi_tests.rs`、`path_tests.rs`：补齐指定行为测试。
- 更新配置文档及 `docs/feature-tree.md`，明确“空环境变量视同未设置”。驳回项未改代码。

实际验证：

| 命令 | 结果 |
|---|---|
| `cargo check --offline` | 通过，0 警告 |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 通过，0 警告 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 195 通过、6 个 TCP 沙箱失败、2 忽略 |
| 仅排除上述六项重跑 | 单元 195＋集成 8 通过，0 失败 |

文件行数、行长、新增代码无 `unwrap/expect`、diff 检查均通过。Windows 原生编译和真实 shell 启动未运行。

完整改动清单、六项失败名称、命令和日志见 [R6 执行报告](/home/mi/work/polaris-pi/docs/pi-migration/r6-result.md)。最终验收由 Claude/Pi 决定。