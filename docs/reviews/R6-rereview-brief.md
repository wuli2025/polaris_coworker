# 复审 brief：P5a 第 1 轮修复（第 2 轮审查）
你上一轮的意见与主控处置见 `P5a-round1-triage.md`（已附）；本轮审 `R6-diff.patch`（**含测试**）。驳回项有代码证据，如仍认为错误请给文件与行号。
## Claude 实测（沙箱外）
两壳 `cargo check` 0 警告；`cargo test -p polaris-kernel --lib --tests` 连续 2 次 201 通过 0 失败；改动文件 ≤500 行。
## 修复要点
Windows bash 回落排除 `%SystemRoot%`/`%windir%` 下（System32、SysWOW64、Sysnative，含 WSL 启动器 bash.exe）与 WindowsApps 别名；`resolve` 返回来源枚举；doctor 显示实际来源，全部排除时不就绪；补 Linux `path_contains_dir` 测试；文档写明空环境变量视同未设置。
## 请重点审查
排除判定能否被绕过（短路径名 `C:\WINDOWS~1`、大小写、`\\?\` 前缀、环境变量缺失/被改写、符号链接/目录联接指向 System32）；来源报告是否与实际使用的路径一致。
