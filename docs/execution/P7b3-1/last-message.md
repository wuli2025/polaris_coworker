状态：**BLOCKED，实现已落地，但未完成验收。未 commit / push。**

已改动：
- agentd：使用现有 undici **8.10.2** 安装代理分发器，支持宿主显式配置、NO_PROXY、凭据脱敏和 doctor HEAD 探测；锁文件无新增包。
- provider：保存改为本地翻译与缓存校验；完整 SDK 校验放在切换事务中。
- mesh：两类客户端复用统一 CA，拆分超长文件并补充测试。

实际验证：
- 三种 `cargo check --offline`：全部通过，零警告。
- agentd typecheck：通过；`npm test`：29 个文件通过、3 个失败。
- kernel 集成测试：10/10 通过；HTTP 回归及 6 项 mesh 单测通过。
- 强制 Rust 联合测试：collab 178 通过、1 失败，随后停止。

阻塞原因：沙箱禁止 loopback 监听，CONNECT、HTTPS 和两项 codex_proxy e2e 无法验证通过；既有 agentd 子进程测试仍无输出。未运行真实账号请求、Windows 实机及 D4 演练。

完整改动文件、命令和真实结果见 [执行报告](docs/tasks/P7b3-execution-report.md)；已更新 [feature-tree](docs/feature-tree.md)。交由 Claude 重跑、Pi 审查。