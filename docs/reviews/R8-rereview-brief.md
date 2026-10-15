# 复审 brief：P5c/P5d 第 1 轮修复（第 2 轮审查）
你上一轮的意见与主控处置见 `P5cd-round1-triage.md`（已附）；本轮审 `R8-all.patch`（**含测试**；其中 `BackendStatus` 默认构造来自 P6a-2 对 R8 回归的修复）。驳回项有代码证据，如仍认为错误请给文件与行号。
## Claude 实测（沙箱外）
三种构建 0 警告；vue-tsc 通过、vitest 65/65；内核单测除下述外全过；已知 `experts::recovery_tests::idle_daemon_reopen_restores_experts_before_releasing_requests` 在全量并行下偶发失败（单独 3/3 通过），已另派任务加固，不必复述。
## 修复要点
前端切换项目未保存确认；专家 prompt 上限 32768 字节与尾部空白规范化；agentd 不可达时专家同步不阻塞后端就绪、状态 `expertsSynced`/`expertSyncError`、恢复后重试；用量价格完整性边界测试。
## 请重点审查
"未同步"期间新会话的专家状态是否可能与用户设置不一致而无提示；恢复重试是否会与用户新设置竞态覆盖；`BackendStatus` 默认值是否可能被误用为真实状态。
