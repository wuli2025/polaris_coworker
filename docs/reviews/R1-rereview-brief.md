# 复审 brief：P1/P2 第 1 轮修复（第 2 轮审查）

上一轮你的意见与主控处置见 `P1-P2-round1-triage.md`（已附）。本轮只审查修复本身（附 diff `R1-diff.patch`），判断：每条"采纳"项是否真正修好、修复是否引入新问题。驳回项附有代码证据，如你仍认为驳回错误，请给出具体行号证据。

## Claude 实测（沙箱外）
- agentd：`npm run typecheck` 通过；`npm test` 46/46（含子进程端到端）。
- Rust：两壳 `cargo check` 通过、7 警告与基线相同；`cargo test -p polaris-kernel --lib --tests` 145 通过，4 失败均为基线已知 Windows 路径测试；`backend_config` 集成测试 4 通过。
- 行数：改动文件全部 ≤500 行、≤500 字符。

## 修复要点
- P1-2：Redactor 同时登记 URL 编码、base64、JSON 转义变体。
- P2-1：Route drop 与 supervisor 线程退出时清理 active。
- P2-2：已有终态时保留首个错误原因。
- P2-4：写入背压独立错误码。
- P2-5：Rust 侧固定总超时改为空闲超时（默认 240s）+ 可选硬上限（默认 0），旧字段具名报错。
