已接通 `ChatSendArgs → 图片/档位校验 → TurnRequest → Pi`，复用现有 `images.rs`，保留 prompt 路径兼容。前端两旧文件各净增 2 行，无新增 UI、依赖或提交。

实际验证：

- 两壳 `cargo check --offline`：通过。
- Rust backend 定向测试：58 通过。
- `cargo test -p polaris-kernel --lib --tests --offline`：158 通过、10 个许可例外失败、2 忽略；补跑集成测试 4 通过。
- `npx vue-tsc --noEmit`：通过。
- 前端附件测试：2 通过。
- `git diff --check`、行数与新增 Rust unwrap/expect 审计：通过。

Pi `read` 支持图片，可为仅有路径的输入提供按需兜底，但需要模型实际调用工具。

逐文件行为、完整命令、日志、配置与迁移见[执行报告](/home/mi/work/polaris-pi/docs/pi-migration/p25b-result.md)；已更新 [feature-tree](/home/mi/work/polaris-pi/docs/feature-tree.md)。

无实现阻塞。真实模型、完整 UI、Windows、agentd 测试及 cargo fmt 未运行。最终完成判定留给 Claude／Pi。