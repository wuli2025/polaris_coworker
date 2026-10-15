# 任务 R10b：P6c 第 2 轮两项低修复
见 `docs/reviews/P6c-round1-triage.md` 第 2 轮。
- `deploy/singapore/scripts/restore.sh`：指纹校验的 curl 改为单次请求超时取较小值（例如 min(10, 配置值)），`--retry-max-time` 为配置值，使总耗时不超过配置值加一次请求超时；README/注释写明语义。
- `deploy/singapore/scripts/backup_crypto.py`：加解密子进程加超时（新配置项，默认 300 秒，进 `.env.example` 与校验），超时报具名错误并保证既有明文清理执行；测试（假命令模拟挂起）。
允许修改：`deploy/singapore/`、`docs/`。必须通过：`PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s deploy/singapore/tests -p 'test_*.py'`、`bash -n` 全部脚本。不 commit。
