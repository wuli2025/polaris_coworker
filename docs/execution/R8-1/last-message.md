R8 **部分实现，BLOCKED**，未 commit/push。

已修改：

- `ContextFileEditor.vue`、新增 `useContextFileEditor.ts` 和 Vitest 测试：切换项目前确认，取消保留选择、编辑内容及保存目标。
- `experts.rs`、`experts_tests.rs`：prompt 默认上限 32768 字节，支持环境配置、具名错误及尾部空白规范化。
- `usage_costs.rs`：仅补价格完整性边界测试。
- 更新 `docs/feature-tree.md` 和[执行报告](/home/mi/work/polaris-pi/docs/pi-migration/r8-review-fixes.md)。

实际验证：

- `npx vue-tsc --noEmit`：通过。
- `npx vitest run`：65 测试通过。
- 桌面 `cargo check --offline`：零警告通过。
- 服务壳：普通组合通过但有 4 条范围外警告；`collab-net` 组合因范围外 `mesh/orchestration.rs` 编译错误失败。
- 内核指定全量测试：213 通过、6 个网络相关失败、2 忽略；补跑 8 个集成测试通过。
- 自有文件均符合 500 行/500 字符限制，新 Rust 代码无 unwrap/expect。

后端 **#4 未实现、恢复测试未运行**：需要修改允许范围外的 `pi/mod.rs`、`pi/supervisor.rs` 行为接线，已请求授权但尚未收到。完整命令、失败名称和阻塞证据均在执行报告中；不声明验收完成。