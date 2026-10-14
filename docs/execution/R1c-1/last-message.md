R1c 已实现，待 Claude 复核；未 commit/push。

- Pi：Route 丢弃时补发 `AgentdSupervisorExited` error→done；State 丢弃同步更新故障状态。
- legacy：修复一次性 stdout 和常驻 reader/job 异常退出漏收尾，增加具名错误与状态记录。
- 新测试覆盖 panic、提前退出、通道关闭、满队列及终态不重复。

实际验证：

- 两壳离线 `cargo check`：均通过。
- backend 测试：46 项通过。
- 完整内核测试：146 通过、10 项既有例外失败、2 忽略。
- 排除指定例外后：146 项单测及 4 项集成测试通过。
- 文件尺寸、授权范围、新代码无 unwrap/expect、`git diff --check`：通过。

[执行报告](/home/mi/work/polaris-pi/docs/pi-migration/r1c-drop-completion.md)列出全部 13 个改动文件、完整命令和十项例外；feature-tree 已更新。

无剩余实现阻塞。未运行真实模型、桌面 UI 端到端或 Windows 测试；无排除全量测试仍受既有基线及沙箱 TCP 限制。