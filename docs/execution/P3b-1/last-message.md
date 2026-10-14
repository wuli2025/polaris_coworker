**BLOCKED：P3b 尚未完成。**

已实现沙箱创建侧的最小挂载、配置与保护名单、安全基线检查、保留旧容器的迁移命令；已修复根 Vitest 收集 agentd 测试的问题，并更新功能树。

验证结果：

- 通过：两壳 `cargo check --offline`、沙箱 13 项测试、`vue-tsc`、根 Vitest 13 项测试。
- agentd：类型检查通过，测试 18 文件通过、2 失败。
- 内核：168 通过、10 失败、2 忽略；排除列明的基线和端口项后通过。

未完成：工具路由接线、专家传递、一键重建 UI。所需 `wiring.rs`、沙箱 UI/API、Feishu 请求兼容修改涉及允许清单外文件，澄清未收到回复，按模块约定未越界。**当前旧 UI 启动会报 `SANDBOX_PROJECT_REQUIRED`。**

完整改动文件、实际命令、失败名单和未运行测试见 [执行报告](/home/mi/work/polaris-pi/docs/pi-migration/p3b-result.md)。未提交、未推送。