R12 已实现，未提交或推送，保留了现有用户改动。

- 后端统一拒绝设备名中的控制字符及 `< > " ' 反引号`，覆盖浏览器、登记、改名、心跳、邀请、契约和遥测入口；保留中文、常见符号及原长度规则。
- 新增前端 `v-html` 扫描回归测试，仅允许指定静态头像。
- 拆分 `identity.rs`，更新功能树；17 个改动文件均满足行数和行长限制，新增 Rust 无 `unwrap/expect`。

实际验证：三种 `cargo check --offline` 均通过、0 警告；指定 Rust 测试 **165 通过**；`npx vitest run` **96 通过**；`git diff --check` 通过。

无实现阻塞。`rustfmt` 因未安装而未运行成功；真实网络测试未启用。完整文件清单、命令及过程记录见[执行报告](/home/mi/work/polaris-pi/docs/execution/R12/report.md)。最终完成状态由 Claude 重跑、Pi 审查后决定。