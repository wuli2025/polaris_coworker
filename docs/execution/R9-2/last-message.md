R9 采纳项已落地，执行状态 **BLOCKED：联合必测尚未通过**。

- 删除冷静期方法、调用分支及专属测试；保留权限、撤销和接口兼容行为。
- 保留缓存恢复、BackendStatus 和文档修复；按职责拆分超长文件，更新 feature-tree。
- 三种 `cargo check --offline`：全部通过，零警告。
- collab：138 通过。
- kernel/wiki 联合测试：219 通过、6 项端口绑定失败、2 忽略；补跑 wiki 7 通过、kernel 集成测试 8 通过。

完整改动文件、命令、真实结果及未运行项目见[执行报告](docs/execution/R9/report.md)。需在允许本地监听的环境重跑联合命令，由 Claude 决定是否完成。

未 commit/push，未触碰 `deploy/`。