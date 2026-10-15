本轮实现已落地，未 commit/push，保留现有用户改动；最终验收由 Claude/Pi 决定。

- 实现设备绑定、签名吊销同步、会话失效、离线新鲜度检查、通知及策略 B 批准流程。
- 修复 wiki 状态构造回归和无 `collab-net` 编译警告。
- 三种构建均通过、0 警告；collab 137 项通过。
- kernel/wiki/fable 排除 6 个沙箱 TCP 项后，338 项通过；`vue-tsc` 通过。
- 前端交互留给 P6b；真实跨主机联测未运行。

[协议、迁移与接口报告](/home/mi/work/polaris-pi/docs/interconnect/p6a2-policy-b.md) · [改动文件、实际命令及限制](/home/mi/work/polaris-pi/docs/interconnect/p6a2-validation.md)