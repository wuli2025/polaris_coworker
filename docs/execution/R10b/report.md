# R10b：P6c 第 2 轮两项超时修复执行报告

范围依据：`docs/reviews/P6c-round1-triage.md` 第 2 轮的两项采纳问题。
仅修改 `deploy/singapore/` 与 `docs/`；未 commit、未 push，保留范围外用户及并行改动。
本报告供 Claude 重跑、Pi 独立审查，最终由 Claude 决定模块是否完成。

## 改动文件与行为

| 文件 | 行为 |
| --- | --- |
| `deploy/singapore/scripts/restore.sh` | curl 单次请求上限改为 `min(10, RESTORE_VERIFY_TIMEOUT_SECS)`，重试预算仍取配置值；保持指纹失败停机与清理路径。 |
| `deploy/singapore/scripts/backup_crypto.py` | 公共 `encrypt` 接口显式接收超时秒数，`decrypt` 从配置读取；共用 `run_crypto`，子进程超时后终止、回收并抛出具名 `BackupCryptoTimeout`。 |
| `deploy/singapore/scripts/snapshot.py` | 将已校验超时传给加密入口；继续使用既有 workspace 与部分归档清理。 |
| `deploy/singapore/scripts/config.py` | 新配置按正整数 `1..86400` 校验。 |
| `deploy/singapore/.env.example` | 新增 `BACKUP_CRYPTO_TIMEOUT_SECS=300`，补充指纹请求超时语义。 |
| `deploy/singapore/README.md` | 说明两种超时的范围、具名错误、清理行为、默认值、类型校验与切换示例。 |
| `deploy/singapore/tests/fake_crypto.py` | 假 age/gpg 写出部分内容后挂起，记录 PID，最长挂起 10 秒以限制回归时测试等待。 |
| `deploy/singapore/tests/test_backup_security.py` | age/gpg 各覆盖加密/解密超时、子进程消失、明文清理、部分密文删除、原服务状态及恢复前拒绝；覆盖 1/5/10/30/300 秒指纹预算参数。 |
| `deploy/singapore/tests/test_behavior.py` | 覆盖旧配置缺字段时默认 300、合法边界与切换值、空值/非整数/非正值/越界值拒绝。 |
| `docs/feature-tree.md` | 原部署入口更新配置、依赖、迁移与本报告链接，保持 500 行。 |
| `docs/execution/R10b/report.md` | 本次执行证据与限制。 |

## 配置、依赖与迁移

- 无新增生产依赖；沿用 Python 标准库 `subprocess.run(timeout=...)`、现有 age/gpg 与 curl。
- `BACKUP_CRYPTO_TIMEOUT_SECS` 默认 300 秒，整数范围 `1..86400`，每次加密/解密调用独立计时。`.env` 设为 `600` 可延长，删除该项恢复默认；旧配置自动合并默认，归档与密钥格式不变。
- `RESTORE_VERIFY_TIMEOUT_SECS` 沿用默认 30、整数范围 `1..300`。curl 的重试预算不截断最后一次请求，请求等待至多配置值加 `min(10, 配置值)`，默认 40 秒；设为 `60` 时上限为 70 秒。Docker 调度、恢复、清理不属于此请求预算。
- 超时报错包含工具名、加密或解密操作与配置秒数；加密失败删除部分归档、清理明文并恢复原服务状态，解密失败由 restore EXIT trap 清理且尚未执行 Docker 操作。

## 实际验证

在仓库根目录执行：

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s deploy/singapore/tests -p 'test_*.py'
rg --files deploy/singapore -g '*.sh' -0 | xargs -0 -n 1 bash -n
git diff --check
```

- unittest：退出 0，33 项通过，35.693 秒；含新增挂起行为用例与既有回归。
- bash：部署包全部 8 个 shell 脚本语法检查退出 0。
- diff 空白检查：退出 0。
- 文件限制：单独用 Python 检查本次 11 个文件，物理行数与单行字符数均不超过 500。

## 未运行与阻塞

本任务两项实现及指定检查无阻塞，无遗留实现项。
未执行真实 age/gpg 密钥环、真实 Docker 容器及公网部署测试；挂起测试使用假命令，curl 测试核对传给容器的实际参数，未实测真实网络的墙钟上限。
现有缺少审核镜像 digest、域名/SMTP/服务器条件的上线阻塞保持原状；本次未部署、不宣称上线验收通过。
