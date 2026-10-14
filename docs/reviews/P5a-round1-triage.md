# P5a 第 1 轮 Pi 审查处置（Claude）

原始报告：`docs/reviews/P5a-round1/`（本次送审包含测试文件）。

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 严重 1 | Linux `/bin/bash` 存在即返回，不回落 | **驳回** | 优先使用 `/bin/bash` 是预期行为；"存在但版本不适配"是推测，Pi 的修法也不检查版本。 |
| 中 1 | `POLARIS_AGENTD_SHELL_PATH=""` 被当作未设置 | **驳回（约定）** | 空环境变量视同未设置是通行约定；在配置文档写明。 |
| 中 2 | doctor 的 shell 提示未反映实际来源 | **采纳并提为高（Windows）** | 关键风险：`doctor/pi_shell.rs:46` 的最终回落 `which_all("bash.exe").next()` 过滤了 WindowsApps 别名，但**未排除 `%SystemRoot%\System32\bash.exe`（WSL 启动器）**。无 Git Bash 且启用了 WSL 的机器上，Pi 的 bash 工具会静默在 WSL 中执行，路径与环境全错。修：回落时排除 `%SystemRoot%`（含 System32/SysWOW64/Sysnative）下的 bash，以及 WindowsApps 别名；都排除后无可用 shell 则明确报告；doctor 报告实际来源（bundled / program_files / path / explicit）。测试：System32 bash 被排除、各来源报告正确。 |
| 中 3 | 中文/空格路径未规范化 | **驳回** | `chat/backend/pi/process.rs:25-26` 以 `Command::new(&config.node).arg(&config.entry)` 直接传 `PathBuf`，未经字符串化，Rust 的 `Command` 对空格与非 ASCII 正确处理。 |
| 中 4 | Node 下载源注释与代码 | **无问题** | Pi 自认代码与注释一致，`sources.test.mjs` 断言不含 GitHub 代理。 |
| 轻 1 | Linux 下 `path_contains_dir` 无测试 | **采纳（低）** | 补 `#[cfg(not(windows))]` 用例。 |
