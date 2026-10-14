R1 **部分完成，P2-5 BLOCKED**。未提交、未推送、未新增依赖；保留原有用户改动。

| 条目 | 结果 |
|---|---|
| P1-2 | 登记 URL、base64、JSON 转义变体；两种凭据入口的日志、工具事件脱敏测试通过 |
| P2-1 | Route/线程退出清理 active；config.apply 不再误报 Busy 的测试通过 |
| P2-2 | 保留首错，后续原因仅日志；测试通过 |
| P2-3 | 已补 reasoning/usage 契约注释 |
| P2-4 | 背压错误贯穿调用方、lastError、日志；测试通过 |
| P2-5 | 未实现：必要入口 `pi/mod.rs:85` 不在允许清单，已请求范围例外，尚未获答复 |

实际验证：

- `cd agentd && npm run typecheck && npm test`：类型检查通过；11 个测试文件通过，原有 e2e 失败。修改前代码亦复现首次 ping 超时，根因未确定。
- 两项指定 `cargo check ... --offline`：均通过。
- `cargo test -p polaris-kernel --lib --tests --offline`：134 通过、10 失败、2 忽略。失败为 **4 个基线项 + 6 个沙箱 TCP 绑定项**。
- 后端定向测试：34 通过；`backend_config`：2 通过。
- `git diff --check`、500 行/500 字符限制检查通过。P2-5 测试未编写或运行。

改动文件与物理行数（Rust 文件均位于 `src-tauri/crates/polaris-kernel/src/chat/backend/pi/`）：

| 文件 | 行数 |
|---|---:|
| `agentd/src/models/credentials.ts` | 34 |
| `agentd/src/models/runtime.ts` | 44 |
| `agentd/test/credentials-review.test.ts` | 63 |
| `client.rs` | 72 |
| `process.rs` | 158 |
| `routes.rs` | 140 |
| `supervisor.rs` | 253 |
| `tests.rs` | 263 |
| `translate.rs` | 79 |
| `docs/feature-tree.md` | 217 |
| `docs/pi-migration/r1-review-fixes.md` | 74 |

完整命令、失败项名单、配置说明和限制见[执行报告](/home/mi/work/polaris-pi/docs/pi-migration/r1-review-fixes.md)。此报告不代表模块验收完成。