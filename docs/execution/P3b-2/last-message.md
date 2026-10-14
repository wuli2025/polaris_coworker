**BLOCKED，未声明模块完成。**

已补齐桌面沙箱路由校验、项目选择与保留旧容器的一键重建、Pi 请求级专家传递；保留创建侧实现和 Vitest 隔离。ChatComposer 净增 3 行，无提交、推送或新依赖。

实际验证：

- 两壳 `cargo check --offline`、`vue-tsc`：通过。
- 沙箱测试：13 通过。
- 根 Vitest：8 文件、23 测试通过。
- 内核全量：170 通过、10 失败、2 忽略；排除既有基线及端口项后，170 单测和 4 集成测试通过。
- agentd：typecheck 通过；测试 18 文件通过、2 失败，分别为 web EPERM、e2e ping 超时。

剩余阻塞：服务端缺少可复用的沙箱策略依赖，当前调用明确失败；依赖范围调整尚未确认。未运行真实 Docker、Windows 实机、浏览器点击及真实模型验证。

逐文件改动、完整命令、失败名单与限制已写入[执行报告](/home/mi/work/polaris-pi/docs/pi-migration/p3b-result.md)，并更新[功能树](/home/mi/work/polaris-pi/docs/feature-tree.md)。最终由 Claude／Pi 核实。