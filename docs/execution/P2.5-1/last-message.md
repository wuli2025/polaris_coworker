**BLOCKED：协议扩展已实现，完整验收尚未满足。**

- 已实现：`session.open` 新设置、空闲 `session.configure`、图片附件及限额、版本 ping、Rust 版本不符立即熔断。
- 已同步：Rust 类型、协议样例与哈希、逐轮模型和权限映射、`TurnRequest` 图片传递。
- SDK 核查：三个设置接口及 `prompt.images` 均真实存在；具体 d.ts 出处见报告。
- 入口阻塞：`ChatSendArgs` 缺少结构化图片和思考档位字段；未越界修改。

实际验证：

| 命令 | 结果 |
|---|---|
| agentd `npm run typecheck` | 通过 |
| agentd `npm test` | 12 文件通过，原有 E2E 首次 ping 超时 |
| 新增协议行为测试 | 6 项通过 |
| 两壳 `cargo check --offline` | 均通过 |
| kernel `--lib --tests --offline` | 149 通过、10 失败、2 忽略；失败为许可的 4 基线＋6 TCP 项 |
| backend 定向测试／配置集成测试 | 49／2 项通过 |

[完整执行报告、SDK 出处与日志](/home/mi/work/polaris-pi/docs/pi-migration/p25-result.md) · [34 个改动文件及行数](/home/mi/work/polaris-pi/docs/pi-migration/p25-files.md)

已更新功能树；文件行数、行长及范围检查通过。未运行真实模型、Windows 或 UI 图片端到端测试。未提交、未推送，保留现有用户改动。