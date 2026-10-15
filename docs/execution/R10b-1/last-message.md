R10b 两项修复已实现，未 commit/push，保留现有及并行改动。

- `restore.sh`：单次请求超时为 `min(10, 配置值)`，重试预算保持配置值。
- `backup_crypto.py`、`snapshot.py`、`config.py`、`.env.example`：新增加解密超时，默认 300 秒；超时报 `BackupCryptoTimeout`，沿用清理流程。
- 更新测试、README、feature-tree；完整 11 文件清单见[执行报告](/home/mi/work/polaris-pi/docs/execution/R10b/report.md)。

实际验证：

- 指定 unittest 命令：33 项全部通过。
- 部署包全部 8 个脚本 `bash -n`：通过。
- `git diff --check`、文件行数及行宽检查：通过。

本任务无遗留实现项或阻塞。未运行真实 Docker、age/gpg 密钥环及公网测试；最终完成状态由 Claude 复核决定。