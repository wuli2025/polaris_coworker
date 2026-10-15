R8 指定修复已落地，**全量验证仍 BLOCKED，未宣称验收完成**。

- 保留前端切换确认、prompt 上限与费用边界测试。
- 新增 `experts_sync.rs` 和恢复测试，接入 `pi/mod.rs`、`supervisor.rs`；状态类型增加 `expertsSynced`、`expertSyncError`。
- 已更新 `docs/feature-tree.md`；全部 16 个自有文件满足行数、行长限制，新增 Rust 无 `unwrap/expect`。

实际验证：

| 命令 | 结果 |
|---|---|
| `cargo check --offline` | 桌面壳通过，0 警告 |
| 服务壳 check，含 `collab-net` | 通过，0 警告 |
| 服务壳 check，不含 `collab-net` | 通过，4 条范围外 mesh 警告 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 218 通过、6 失败、2 ignored |
| 补跑内核集成测试 | 8 通过 |
| `npx vue-tsc --noEmit` | 通过 |
| `npx vitest run` | 65 通过 |

6 个失败均涉及受限环境下的本地端口绑定。浏览器 E2E、真实 agentd 端到端未运行。

完整文件清单、命令、日志及阻塞见[第二次执行报告](/home/mi/work/polaris-pi/docs/pi-migration/r8-review-fixes-run2.md)。未修改 P6a 文件，无新依赖，未 commit/push；最终由 Claude 重跑、Pi 审查后决定。