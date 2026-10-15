实现已落地，**验收状态：BLOCKED**，未提交或推送。

- `usage-store.ts`、`registry.ts`：按会话 ModelRuntime 的模型单价写入 `priceAvailable`，不根据请求费用判断。
- Rust 沿用既有字段读取；旧记录缺字段仍按未知处理。新增测试覆盖价格状态、零费用和聚合计数。
- 已更新测试及 `docs/feature-tree.md`。全部 8 个改动文件符合 500 行/字符限制，新增 Rust 无 unwrap/expect。

实际验证：

| 命令 | 结果 |
|---|---|
| agentd `npm run typecheck && npm test` | 类型检查通过；26/28 文件通过 |
| 两壳 `cargo check --offline` 对应入口 | 均通过，零警告 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 207 通过、6 失败、2 ignored |
| Rust 集成测试补跑 | 8 项通过 |
| `npx vitest run` | 61 项通过 |
| agentd 用量定向测试 | 6 项通过 |

阻塞：本地监听权限/端口绑定失败，以及 agentd e2e 首次 ping 超时。未运行真实计费请求、桌面操作及跨平台验证。

完整改动文件、精确命令、日志和未完成项见[执行报告](/home/mi/work/polaris-pi/docs/pi-migration/p5d2-usage-pricing.md)。最终完成结论由 Claude 复核决定。