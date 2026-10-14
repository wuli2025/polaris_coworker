R7 改动与验证已执行，未改生产代码，未 commit/push，保留其他用户改动。

- [tests.rs](/home/mi/work/polaris-pi/src-tauri/crates/polaris-wiki/src/wiki/compile/tests.rs:188)：新增指定四种路径的回归测试，均断言返回 `WikiWriteOutsideRoot`。
- [feature-tree.md](/home/mi/work/polaris-pi/docs/feature-tree.md:416)：更新入口、依赖、配置、迁移与验证信息。
- [执行报告](/home/mi/work/polaris-pi/docs/pi-migration/r7-result.md)：记录命令、结果与限制。

在 `src-tauri/` 实际运行：

| 命令 | 结果 |
| --- | --- |
| `cargo test -p polaris-wiki --offline` | 7 通过，0 失败 |
| `cargo check -p polaris-app --offline` | 退出 0，0 警告 |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0，0 警告 |

`git diff --check`、文件行数及行宽检查通过。未运行 Windows/macOS 原生测试或真实模型端到端测试。无剩余实现项、无阻塞；最终验收由 Claude 决定。