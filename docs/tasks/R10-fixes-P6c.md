# 任务 R10：P6c 审查采纳项
先读 `docs/reviews/P6c-round1-triage.md`。修复 #3、#5 与"额外"项；驳回项不要改。
- #3：`scripts/restore.sh` 在重启 authority 后自动获取公钥并用 `probe.py fingerprint` 比对 `EXPECTED_FINGERPRINT`；未配置预期指纹时拒绝恢复（除非显式确认变量）；不符则停止 authority 并非零退出，输出明确指引。测试（fake docker）：一致通过、不一致停止并失败、未配置拒绝。
- #5：备份默认加密：`.env.example` 增加加密配置（例如 `BACKUP_ENCRYPT=age|gpg`、`BACKUP_RECIPIENT`），`snapshot.py`/`backup.sh` 生成加密包并安全删除明文；未配置时拒绝，除非显式设置确认变量（例如 `BACKUP_PLAINTEXT_ACK=1`）并醒目告警；`restore.sh` 支持解密。服务器侧工具不进仓库依赖，README 写明安装。测试：加密路径不留明文、未配置拒绝、确认变量放行并告警。
- 额外：删除 Caddyfile 中多余的 `header_up X-Forwarded-For`，按 `caddy fmt` 规范格式化模板（保持占位符）。
允许修改：`deploy/singapore/`、`docs/`。无新依赖；脚本 `set -euo pipefail`、幂等；≤500 行/≤500 字符。
必须通过：`PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s deploy/singapore/tests -p 'test_*.py'`；`bash -n` 全部脚本。不 commit。
