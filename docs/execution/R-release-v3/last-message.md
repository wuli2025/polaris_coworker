已落实本模块改动，未提交、未推送。

详细文件、行为、命令及结果见 [执行报告](/home/mi/work/polaris-v3/docs/tasks/R-release-v3-report.md)。

- 通过：离线 cargo check、前端类型检查、Vitest 118 项、agentd 类型检查及新增凭据测试。
- 验收 **BLOCKED**：Rust 4 项受端口限制失败；agentd 3 个文件失败，其中 E2E 在原始基线同样复现。
- Windows/macOS 安装包、Docker 构建/history 和真实模型请求尚未验证。

最终模块结论留给 Claude 重跑与 Pi 审查。