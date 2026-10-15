# R10：P6c 审查采纳项执行报告

日期：2026-10-15。仅实现分诊记录中的 #3、#5 与额外 Caddy 清理；#1、#2、#4 驳回项未改。
代码和规定离线测试已落地；真实工具/容器验证仍有下述阻塞。此报告不代表最终验收，由 Claude 重跑、Pi 独立审查后决定。
未 commit/push，未部署；保留原有前端、Rust、功能树 P6b 行及 `docs/execution/R10-1/` 等用户/并行改动。

## 改动文件、入口与依赖

| 文件 | 行为 |
| --- | --- |
| `deploy/singapore/scripts/restore.sh` | 缺 pin 默认拒绝；先解密/验证，备份现状后恢复；重启后获取本机 pubkey，经原 `probe.py fingerprint` 核对；失败停 authority、非零退出并给出指引；原服务停止时临时启动核验后停回去 |
| `deploy/singapore/scripts/common.sh` | `backup_authority` 供 backup/upgrade/restore 共用；停机前检查加密策略；独立 subshell trap 清理复制/归并临时目录并恢复原运行状态，不覆盖 restore trap |
| `deploy/singapore/scripts/snapshot.py` | SQLite WAL 归并、既有白名单/哈希/种子检查；在私有目录打包，加密后原子发布；新增 policy/unpack/clean CLI；关闭 SQLite 连接后清理 |
| `deploy/singapore/scripts/backup_crypto.py`（新增） | 标准库调用服务器 age/gpg；固定 argv，不执行用户提供的 shell；独占临时输出；明文覆盖/fsync/删除，不跟随符号链接；缺工具或加密失败不降级 |
| `deploy/singapore/.env.example`、`scripts/config.py` | 新增加密、收件人、恢复私钥、两个 ACK 与恢复等待参数；默认值和类型/范围校验 |
| `deploy/singapore/templates/Caddyfile` | 删除多余 X-Forwarded-For；改为 tab 缩进和块间空行，保留占位符及 PROXY/TLS 配置；原生 fmt 校验未运行 |
| `deploy/singapore/tests/fake_docker.py` | 模拟本机 pubkey、失败复制部分文件及恢复时删除 WAL/SHM |
| `deploy/singapore/tests/test_behavior.py` | 新配置非法值拒绝；复用维护夹具；保持既有 WAL、升级、重复恢复等测试；恢复成功核对指纹输出，失败检查清理 |
| `deploy/singapore/tests/fake_crypto.py`（新增） | 仅测试用可逆传输替身，显式标注不是加密；记录 argv，模拟部分输出失败，不进入生产调用 |
| `deploy/singapore/tests/test_backup_security.py`（新增） | 10 项行为测试：缺配置拒绝、明文 ACK 告警、age/gpg 路径往返及无明文残留、加/解密失败清理、缺 pin 拒绝、不符停机、取 key 失败停机、无 pin ACK、原停止状态核验 |
| `deploy/singapore/README.md`、`VALIDATION.md` | 更新安装/配置/迁移/恢复/清理限制；原 P6c 历史执行记录仅追加 R10 入口 |
| `docs/feature-tree.md`、本报告 | 替换既有 P6c 行增加 R10 入口，功能树保持 500 行，保留并行 P6b 修改；列出验证与限制 |

`backup.sh` 本身沿用原入口，通过公开 `backup_authority` 调用新增行为。
无 Cargo/npm/Python 新依赖；静态测试继续用环境已有 PyYAML。
生产加密依赖运维在宿主安装 age 或 GnuPG，README 给出安装命令；不添加仓库包或镜像依赖。

## 配置、切换与迁移

所有业务配置继续由 `.env`/`SG_ENV_FILE` 读取，不接受同名 shell 环境变量覆盖。

| 参数 | 默认/类型 | 切换与用途 |
| --- | --- | --- |
| `BACKUP_ENCRYPT` | `age`；枚举 age/gpg/空 | 与非空 recipient 配对；输出 `.tar.age` 或 `.tar.gpg` |
| `BACKUP_RECIPIENT` | 空；无空白、不能以 `-` 开头的字符串 | age 公钥或 gpg 完整公钥指纹；具体密钥格式由对应工具校验 |
| `BACKUP_PLAINTEXT_ACK` | `0`；仅 0/1 | 缺加密配置时默认拒绝；1 表示明确确认备份目录是加密介质，允许明文且醒目告警；不能绕过已配置工具的失败 |
| `BACKUP_IDENTITY` | 空；非空须绝对路径 | age 解密私钥文件；gpg 使用运维用户密钥环 |
| `RESTORE_UNPINNED_ACK` | `0`；仅 0/1 | 缺 EXPECTED_FINGERPRINT 时默认拒绝；1 放行但告警且标明身份尚未核实，不绕过已配置 pin 的不匹配 |
| `RESTORE_VERIFY_TIMEOUT_SECS` | `30`；整数 1..300 | curl 连接重试时间与单次请求上限；总体耗时可能包含最后一次请求时间 |

切换示例：`.env` 中设置 `BACKUP_ENCRYPT=gpg`、`BACKUP_RECIPIENT=<已核实完整指纹>`、`BACKUP_PLAINTEXT_ACK=0`。
回到 age：设 `BACKUP_ENCRYPT=age`、可信 age 公钥 recipient；恢复时提供绝对 `BACKUP_IDENTITY`。
临时明文目录：`TMPDIR=/mnt/encrypted-tmp ./scripts/backup.sh`，restore 同理；私有子目录 0700，CLI 文件 0600。
更多完整示例和安装要求见 [README](../../../deploy/singapore/README.md)。

旧 `.env` 合并新增默认值；旧部署未补加密配置时，backup、upgrade 及有现存库的 restore 前置备份均拒绝执行。
历史 `.tar` 仍可恢复，归档内部 format=1 不变，无数据库迁移。加密恢复按文件后缀选工具，与新备份选择的模式独立。
恢复前必须取得可信 EXPECTED_FINGERPRINT；仅显式灾难恢复 ACK 可缺省，实际输出不代表已建立信任。

## 实际验证

| 实际命令/操作 | 结果 |
| --- | --- |
| `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s deploy/singapore/tests -p 'test_*.py'` | 最终退出 0；29 项通过，22.863s。包括 fake Docker、fake age/gpg、真实 SQLite 与原 probe 指纹计算 |
| `for file in deploy/singapore/scripts/*.sh; do bash -n "$file" || exit; done` | 退出 0，8 个 shell 脚本全部通过 |
| `git diff --check -- deploy/singapore docs/feature-tree.md` | 退出 0 |
| `shellcheck -x deploy/singapore/scripts/*.sh` | 退出 1；info 级 SC1091（动态 source 路径）、SC2317（trap 间接调用）、SC2016（刻意延后到容器 shell 展开） |
| `shellcheck -x -P SCRIPTDIR --severity=warning deploy/singapore/scripts/*.sh` | 退出 0；解析脚本目录 source 后无 warning/error；上述 info 未改作通过 |
| `test_file_limits` 及独立 Python 行数/字符断言 | 退出 0；29 个自有文本（含报告）均 ≤500 行、每行 ≤500 字符；功能树仍为 500 行 |
| `command -v caddy`、`command -v age` | 未找到；未运行真实 age 与 Caddy fmt/validate |
| `docker image inspect caddy:2.10.2-alpine --format '{{.Id}}'` | 退出 1，Docker socket permission denied；未启动/拉取容器 |
| 隔离临时 GNUPGHOME 下，Python 调用 `gpg --no-options --batch --pinentry-mode loopback --passphrase '' --quick-generate-key 'R10 Test <r10@example.invalid>' rsa2048 encr 1d` | 退出 2，gpg-agent 无法启动，No agent running；因此后续真实 encrypt/decrypt 未执行；finally 调用 gpgconf 清理 agent、删除临时目录 |

首轮 29 项中有 2 个子用例失败：替身未模拟删除 WAL/SHM，测试读到恢复前更新值；补齐替身并显式关闭测试连接后第二轮 29 项通过（22.483s）。
之后将 Python 内层临时目录纳入 shell 清理范围，再运行上表最终一轮通过。
输出中的 HTTPS/注册 PASS 为既有 mock 结果，没有访问真实权威或创建真实账号。

## 未完成验证与限制

- **BLOCKED：原生工具验证。** 没有 age/Caddy 可执行文件，Docker socket 拒绝访问，gpg-agent 无法启动。
  未运行真实 age/gpg 加解密往返、`caddy fmt --diff`、Caddy validate 或真实容器恢复演练；模板只做格式整理，不声称原生 formatter 已确认。
- 29 项测试验证脚本流程和数据恢复，不证明替身实现真实加密；目标服务器必须安装工具并用隔离数据重跑备份/解密/身份不符停机演练。
- 覆盖/fsync/unlink 无法保证 SSD/CoW/日志文件系统历史块被抹除；断电/SIGKILL 不能执行 trap。README 明确要求临时目录使用加密盘或 tmpfs，并预留足够容量。
- 原 P6c 成品 digest、真实域名/SMTP/服务器等上线输入仍未提供；没有上线、真实凭据、邮件、公网 TLS/UDP 或客户端联调验收。
- 未运行 Rust/前端全量测试；本次未修改这些模块。未修改驳回项、未扩展任务范围。
