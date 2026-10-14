**BLOCKED，入口尚未接通。**

已新增 `backend/images.rs` 及行为测试，调整 `backend/config.rs`、`backend/mod.rs`，更新 `docs/feature-tree.md`：实现路径边界、附件限额、MIME 校验和 base64 编码，目前尚未接入发送链路。

阻塞原因来自任务范围约定：

- 必须同步范围外 `headless.rs`、`integrations/feishu.rs` 的结构体初始化。
- 指定前端文件已有 665、2631 行，需要允许保留既有超长结构。
- 上述有限例外尚未收到确认。

实际验证：

- 两壳 `cargo check --offline`：通过。
- `npx vue-tsc --noEmit`：通过。
- 新增附件测试：5 通过；集成测试：4 通过。
- 指定 Rust 全量测试：154 通过、10 项既有基线/TCP 例外失败、2 忽略。
- TurnRequest 接线与端到端测试未实现、未运行。

Pi 内置 `read` 支持图片，可对仅含路径的 prompt 按需兜底。未提交、未推送。

完整文件清单、命令、日志及阻塞说明见 [执行报告](docs/pi-migration/p25b-result.md)。