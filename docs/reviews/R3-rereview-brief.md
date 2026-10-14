# 复审 brief：P3 第 1 轮修复（第 2 轮审查）

你上一轮的意见与主控处置见 `P3-round1-triage.md`（已附），本轮只审修复 diff `R3-diff.patch`（测试文件已排除）。驳回项附有代码证据，如仍认为驳回错误，请给出具体文件与行号。

## Claude 实测（沙箱外）
agentd typecheck 通过、`npm test` 100/100（含子进程端到端、本地 HTTP 测试）；Rust 两壳 `cargo check` 通过（7 警告同基线）；`cargo test -p polaris-kernel --lib --tests` 174 通过，4 失败均为基线 Windows 路径测试；`vue-tsc` 通过；前端 vitest 13/13；改动文件 ≤500 行。
主控复现脚本（工作区内 `link → 外部目录`，写 `link/../escaped.txt`）修复前判"目录内"并写到外部，修复后判"非目录内"。

## 修复要点
C1 沙箱 bash 真实退出码；P3a-1 路径判定（含 `..` 一律非目录内、只 realpath 已存在前缀）；P3a-2 快速探测与缓存；P3b-8 授权过期关闭事件与倒计时；P3b-9 告警；协议 v4。

## 请重点审查
路径判定还有无其他绕过（大小写不敏感文件系统、Windows 盘符/UNC/短文件名、硬链接、路径中的空字节、尾随分隔符）；探测缓存是否会在沙箱掉线后错误放行；退出码在超时/信号下的语义。
