已落地提供商原生注册、保存前校验、错误路径透传及私有 CA 支持。**状态：BLOCKED，未提交、未推送。**

- 三种离线 `cargo check`、`cargo build --offline` 均通过，零警告；类型检查、9 项 kernel 集成测试、HTTP 错误透传和内存 TLS 握手通过。
- 全量测试未通过：本地监听被沙箱拒绝；agentd 另有可在 HEAD 副本复现的 e2e 失败。
- 白名单外两处 mesh 客户端未获修改授权，尚未统一 CA；D4 和桌面 UI 实测未运行。

改动文件、行为、实际命令、结果及阻塞详见[执行报告](/home/mi/work/polaris-pi/docs/tasks/P7b2-execution-report.md)；已更新功能树。最终验收由 Claude 决定。