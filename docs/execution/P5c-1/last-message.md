P5c 执行状态：**BLOCKED，部分实现**。

- 1、2、6、7、8：已接入诊断、提供商、沙箱、Pi 文案、类型及预览 mock。
- 3：成本不再误显示免费；精确判断缺少后端价格完整性字段。
- 4：缺少 AGENTS.md 选择与读写接口，Pi 旧编辑入口暂禁用。
- 5：请求级专家入口已接入；缺少真实开关接口，显示默认关闭、当前状态未知。

实际验证：

- `npx vue-tsc --noEmit`：通过。
- `npx vitest run`：13 文件、53 测试通过。
- `npx vite build`：通过，有大 chunk 警告。
- `git diff --check`：通过。

完整[执行报告与逐文件行数、旧文件净增](/home/mi/work/polaris-pi/docs/pi-migration/p5c-frontend.md)已生成，[feature-tree](/home/mi/work/polaris-pi/docs/feature-tree.md)已更新。新文件及旧文件增量均符合限制。

未运行真实 OAuth、模型、Docker 或浏览器点击验收。未修改后端、未新增依赖、未提交或推送；最终验收由 Claude 决定。