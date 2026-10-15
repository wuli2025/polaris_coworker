已实现桌面登录编排展示、默认设备视图、安全通知/审批重试、高级区折叠及可配置 HTTP 频控。未 commit/push；保留其他任务改动，待 Claude/Pi 复核。

`InterconnectView.vue`：**2757 → 2209 行，减少 548 行**。

| 新组件 | 行数 |
|---|---:|
| LoginPanel.vue | 31 |
| LoginProgress.vue | 18 |
| DeviceList.vue | 28 |
| AdvancedPanel.vue | 84 |
| SecurityNotices.vue | 16 |
| ApprovalPanel.vue | 24 |

实际验证：

- `npx vue-tsc --noEmit`：通过。
- `npx vitest run`：77 项通过。
- `npx vite build`：通过，有大 chunk 提示。
- 三种要求的 `cargo check --offline`：全部通过，**0 警告**。
- 指定 collab Rust 测试：**142 项通过，0 警告**。
- `git diff --check`、文件行数/字符检查：通过。

仍缺接口：浏览器/手机等价登录编排 HTTP 入口；审批发起设备 ID 到名称的映射。前者仍为 **BLOCKED**，未声称全端完成。真实多设备、relay、NAS 挂盘联测未运行。

完整改动文件、配置示例、命令与真实结果见[执行报告](/home/mi/work/polaris-pi/docs/interconnect/p6b-frontend.md)；已更新 `docs/feature-tree.md`。