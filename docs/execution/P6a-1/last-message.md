**BLOCKED：部分实现，未声明 P6a 完成。未提交、未推送。**

已实现：
- 随包配置、覆盖优先级、占位本地模式、完整 SHA-256 指纹校验。
- 登录/登记/报到下发 relay，自动应用及“待重连生效”状态。
- 登录编排、失败阶段与重试；新增 `account_login_password`。

**策略 B 尚未实现**：当前断言不绑定设备，且离线会话无法保证云端吊销立即生效。待主控明确设备绑定与离线访问取舍；冷静期暂未取消，通知、审批、超时及 N 小时配置未接线。

高风险清单已写入代码：删除远程盘/挂载、修改权限/角色、吊销其他设备、改密码/邮箱、删除账号。

验证结果：
- 两壳 `cargo check --offline` 对应检查：通过，零警告。
- collab：124 passed；其中两项网络测试被既有环境门控跳过。
- kernel：216 passed、6 项端口绑定失败、2 ignored；补跑集成测试 8 passed。
- `npx vue-tsc --noEmit`：通过。
- rustfmt：工具链未安装，未运行成功。

[执行报告](/home/mi/work/polaris-pi/docs/interconnect/p6a-backend.md) 包含配置项、迁移、高风险依据及 P6b 接口；[验证与文件清单](/home/mi/work/polaris-pi/docs/interconnect/p6a-validation.md) 列出全部 **26 个自有文件及行数**、完整命令和失败名称。已更新 `docs/feature-tree.md`，保留并行用户改动。