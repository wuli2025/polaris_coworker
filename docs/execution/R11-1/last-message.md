已实现 #5、#6，未改驳回项，未 commit/push，保留原有用户文件。

- 邮箱：统一匿名响应，后台按账号状态发送验证码、提醒或不发信；保留频控，统一后续验码错误。
- 前端：本地清理并隐藏过期待批准项，补充 Vitest。
- 已更新功能树。完整改动文件、配置及命令记录见[执行报告](/home/mi/work/polaris-pi/docs/execution/R11/report.md)。

实际验证：

| 命令 | 结果 |
|---|---|
| `cargo check -p polaris-app --offline` | 退出 0，零警告 |
| `cargo check -p polaris-cli --bin polaris-server --offline` | 退出 0，零警告 |
| 上述 server check 加 `--features collab-net` | 退出 0，零警告 |
| 指定 collab 全量 `cargo test … --lib --tests --offline` | 160 passed，零警告 |
| `npx vue-tsc --noEmit` | 退出 0 |
| `npx vitest run` | 87 passed |
| 差异、文件尺寸、新增 Rust unwrap/expect 检查 | 全部通过 |

无实现阻塞。未运行真实 SMTP、网络联测；rustfmt 因工具链未安装而失败，已记录。以上仅为执行报告，最终由 Claude/Pi 核实验收。