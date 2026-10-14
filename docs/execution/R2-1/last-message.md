R2 修复已落地，完整验证仍 **BLOCKED**，待 Claude 复核。

- `supervisor.rs` / `client.rs`：显式回复退出错误，区分通道关闭与真正超时。
- `options.ts`：完整预检后应用配置，失败恢复模型、思考档位和工具；测试确认 `toolProfile` 保持原值。
- 已补行为测试，并在 `docs/feature-tree.md` 记录 #2 限制；驳回项未改代码。

验证：两壳离线 check、agentd typecheck 通过；配置测试 5/5、协议测试 6/6 通过。Rust 排除指定 10 项例外后，160 个库测试和 4 个集成测试通过。`npm test` 为 13 文件通过、既有 e2e 失败：首个 ping 超时，子进程 exit=0，根因未确认。

完整文件清单、实际命令、例外及未运行项见 [执行报告](/home/mi/work/polaris-pi/docs/pi-migration/r2-review-fixes.md)。文件约束检查通过，无新依赖，未提交或推送。