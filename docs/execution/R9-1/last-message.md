R9 **部分落地，BLOCKED**。

- 已实现缓存损坏清理及心跳恢复测试；`BackendStatus::Default` 改为仅测试可用；补齐 relay 信任边界与 prompt 字节上限文档。
- 三种 offline check 均成功、0 警告；collab 138 项通过。
- kernel/wiki 联合测试失败：kernel 219 通过、6 项端口绑定失败、2 ignored。单独补跑 wiki 7 项、kernel 集成测试 8 项均通过。
- **#7 未实施**：仍有范围外 `src-tauri/src/mesh.rs` 调用方，并涉及既有超长文件；未收到扩展授权。

改动文件、实际命令、日志及未运行项见 [执行报告](/home/mi/work/polaris-pi/docs/execution/R9/report.md)。所有本次编辑文件满足行数限制；保留用户改动，未 commit/push。