
# Pi 独立审查报告

## 审查模式
**packet**（无工具）。依据 `R10-diff.patch`（含测试代码）、`P6c-round1-triage.md` 和 `R10-rereview-brief.md` 独立评审。

---

## 结论
**CHANGES_REQUIRED**

发现 1 个严重缺陷（restore.sh 中 curl 参数错误导致超时行为与配置不符）和 2 个中等问题（gpg-agent 非交互假设、clean() 对符号链接目录的鲁棒性），需修复后复验。

---

## 范围与缺失材料

| 项目 | 状态 |
|------|------|
| 加密/解密核心逻辑 | ✓ 已提供 backup_crypto.py (105行) |
| 备份流程 (backup_authority, snapshot.py pack) | ✓ 已提供 |
| 恢复流程 (restore.sh, snapshot.py unpack) | ✓ 已提供 |
| 配置校验 (config.py validate) | ✓ 已提供 |
| 测试覆盖 (test_backup_security.py) | ✓ 已提供 (14项用例) |
| 真实镜像实测证据 | ✓ brief 提供 |
| gpg-agent 可用性 / passphrase 准备方式 | ✗ 未提供实现或文档 |

---

## 问题

### 问题 1 — 严重：`RESTORE_VERIFY_TIMEOUT_SECS` 未实际生效

**文件**：`deploy/singapore/scripts/restore.sh` 行 379

**代码**：
```bash
SG_TIMEOUT=$(value RESTORE_VERIFY_TIMEOUT_SECS)
dc exec -T authority curl -fsS --retry "$SG_TIMEOUT" --retry-connrefused \
  --retry-delay 1 --retry-max-time "$SG_TIMEOUT" --max-time "$SG_TIMEOUT" \
  http://127.0.0.1:8080/api/account/pubkey \
  | python3 "$SG_ROOT/scripts/probe.py" fingerprint "$SG_EXPECTED"
```

**证据**：`--retry 30` 是 curl 的**重试次数**参数（最多尝试 30 次），而非时间；配置范围 1..300 秒，但实际会被解释为重试次数 1~300 次。这导致：

- `RESTORE_VERIFY_TIMEOUT_SECS=30` 实际表示：最多重试 30 次，每次最多等 `30` 秒（由 `--max-time 30` 控制），总计可能约 **900 秒** 而非 30 秒
- `RESTORE_VERIFY_TIMEOUT_SECS=300` 实际表示：最多重试 300 次，每次最多等 300 秒，严重偏离设计意图

**可复现影响**：用户设置 30 秒超时预期，但实际等待时间可能达数分钟；sigkill/sigterm 无法及时终止，restore.sh 可能在 `dc exec` 期间长时间阻塞。

**最小修复建议**：
```bash
# 修复：限制重试次数，max-time 使用单次请求超时
SG_TIMEOUT=$(value RESTORE_VERIFY_TIMEOUT_SECS)
SINGLE_TIMEOUT=$((SG_TIMEOUT / 3 > 10 ? 10 : SG_TIMEOUT / 3))  # 单次超时不超过 10s
dc exec -T authority curl -fsS --retry 3 --retry-connrefused \
  --retry-delay 1 --retry-max-time "$SG_TIMEOUT" --max-time "$SINGLE_TIMEOUT" \
  http://127.0.0.1:8080/api/account/pubkey \
  | python3 "$SG_ROOT/scripts/probe.py" fingerprint "$SG_EXPECTED"
```

---

### 问题 2 — 中：gpg 解密无超时保护，可能挂起

**文件**：`deploy/singapore/scripts/backup_crypto.py` 行 214

**代码**：
```python
elif suffix == '.gpg':
    command = [tool('gpg'), '--no-options', '--batch', '--no-tty', '--output', '-', '--decrypt']
...
subprocess.run(command, stdin=body, stdout=output, check=True)
```

**证据**：`--batch` 禁止交互式提示，但如果 gpg-agent 未预缓存 passphrase 且密钥有 passphrase，`gpg --decrypt` 会以静默方式等待（不输出到终端）而非立即失败，可能永久挂起。README 行 83 仅说明"私钥解锁须事先准备"，未提供实现保障。

**可复现影响**：用户未配置 gpg-agent 时，restore.sh 的 `finish_restore()` trap 无法在 gpg 挂起前执行，导致无法清理明文且服务被停止（`SG_RESTORE_PENDING=1`）。

**最小修复建议**：
```python
import signal
try:
    subprocess.run(command, stdin=body, stdout=output, check=True, timeout=300)
except subprocess.TimeoutExpired:
    raise ValueError('BLOCKED: gpg 解密超时（5分钟），请检查 gpg-agent 是否已缓存 passphrase')
```

---

### 问题 3 — 中：`clean()` 对符号链接目录的处理可导致部分残留

**文件**：`deploy/singapore/scripts/backup_crypto.py` 行 155-156

**代码**：
```python
for name in dirs:
    path = Path(root) / name
    if path.is_symlink():
        path.unlink()  # ← 对符号链接目录无效，会抛出异常
    else:
        path.rmdir()
```

**证据**：`path.unlink()` 用于删除文件符号链接，对目录符号链接无效（需用 `os.unlink()` 系统调用）；若 `path.is_symlink()` 返回 True 但符号链接为目录类型，`unlink()` 会抛出 `OSError`，被追加到 errors 列表，但**不会重试删除**。若此时该符号链接指向加密备份目录，可能留下指向敏感路径的悬空链接。

**可复现影响**：在配置 `TMPDIR` 指向含符号链接结构的目录时，清理可能失败并抛出异常。

**最小修复建议**：
```python
for name in dirs:
    path = Path(root) / name
    if path.is_symlink():
        try:
            os.unlink(path)  # 对文件和目录符号链接均有效
        except OSError:
            pass  # 忽略删除失败的符号链接
    else:
        path.rmdir()
```

---

## 已验证通过的部分

| 检查项 | 结论 | 依据 |
|--------|------|------|
| 明文清理在 backup 失败路径执行 | ✓ | `finish_backup()` trap (common.sh:258-267) 调用 `snapshot.py clean`；subshell 异常退出时 trap 仍生效 |
| 明文清理在 restore 失败路径执行 | ✓ | `finish_restore()` trap (restore.sh:340-353) 始终调用 clean |
| 加密命令失败不退回明文 | ✓ | `policy()` 检查在 `pack()` 入口执行；`test_encryption_failure_cleans_partial_and_restarts` 覆盖 |
| 解密失败不继续执行 | ✓ | decrypt 异常由 snapshot.py 捕获，unpack 返回前失败；`test_decryption_failure_cleans_plaintext_before_docker` 覆盖 |
| 恢复失败后服务停止 | ✓ | `finish_restore()` 检测 `SG_RESTORE_PENDING=1` 并停止 authority；test 覆盖 |
| 恢复后公钥与预期指纹比对 | ✓ | `probe.py fingerprint` 比对；SIGPIPE/非零退出触发 `set -e` |
| 缺少 EXPECTED_FINGERPRINT 默认拒绝 | ✓ | restore.sh:331-336；`test_restore_missing_pin_refuses_before_docker` 覆盖 |
| 私钥文件权限 0600 | ✓ | extract 后 chmod 600 (restore.sh:372)；Caddyfile 中 tar 添加时 mode=0o600 |
| ACK 仅允许 0/1 | ✓ | config.py:287-289；test_behavior.py:796 覆盖 |
| 加密模式仅允许 age/gpg/空 | ✓ | config.py:305-306；test_behavior.py:795 覆盖 |
| BACKUP_RECIPIENT 校验（无选项/空白） | ✓ | config.py:308-309 |
| BACKUP_IDENTITY 需绝对路径 | ✓ | config.py:310-311 |
| restore 需公钥后重启/保持停止状态 | ✓ | restore.sh:376-383；`test_restore_checks_previously_stopped_authority` 覆盖 |
| 临时目录 0700/0600 | ✓ | `mktemp -d` 默认 0700；`os.umask(0o077)` 保护新建文件 |
| BaseException 捕获（含 SystemExit） | ✓ | snapshot.py:473 |

---

## 建议验证步骤

| 优先级 | 步骤 | 预期结果 |
|--------|------|----------|
| 高 | 构造 `RESTORE_VERIFY_TIMEOUT_SECS=5`，用 `timeout 2s dc exec` 模拟慢响应 | 应在约 2 秒后超时触发 finish_restore，而非等待 5×5=25 秒 |
| 高 | gpg 模式下用无 passphrase 的测试密钥执行 restore | 应在合理时间内完成，不挂起 |
| 高 | gpg 模式下用有 passphrase 但未配置 gpg-agent 的密钥执行 restore | 应超时失败，不挂起 |
| 中 | 验证 `clean()` 对符号链接目录的处理 | 目录符号链接应被正确删除 |
| 低 | 压力测试：快速连续发送 SIGINT/SIGTERM | 临时目录应被清理，服务状态正确 |
