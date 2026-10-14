R1b 已实现，待 Claude 重跑、Pi 审查后决定完成。

- 改动授权的 7 个 Rust 文件及 4 个文档：默认空闲超时 240000ms、硬上限 0；旧字段具名拒绝；两类超时分别以错误码 + done 收尾。
- 两壳 `cargo check --offline` 均通过。
- `cargo test -p polaris-kernel --lib --tests --offline`：139 通过、10 失败、2 忽略。失败仅为既有 4 项基线及 6 项沙箱 TCP 项。
- 单独运行 `backend_config`：4 项通过。
- 范围、500 行/字符限制、新增代码无 unwrap/expect、`git diff --check` 均通过。无新依赖、无 commit/push，保留用户改动。

[执行报告](/home/mi/work/polaris-pi/docs/pi-migration/r1b-idle-timeout.md) 已列出全部文件、实际命令、失败项与迁移示例。无剩余实现阻塞；真实模型、Windows、桌面 UI 端到端及实际分钟级测试未运行。