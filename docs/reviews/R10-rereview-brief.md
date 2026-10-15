# 复审 brief：P6c 第 1 轮修复（第 2 轮审查）
你上一轮意见与主控处置见 `P6c-round1-triage.md`（已附，其中 #1/#2 经真实镜像实测驳回）；本轮审 `R10-diff.patch`（含测试）。
## Claude 原生实测
29 项单测通过；真实 `age` 1.1.1 加密→解密往返一致、密文不含明文；按部署包 `config.prepare` 渲染后 `caddy:2.10.2-alpine caddy validate` Valid、`haproxy:3.2.26-alpine haproxy -c` 退出 0；全部脚本 `bash -n` 与 `shellcheck -S warning` 无告警。
## 修复要点
恢复后自动比对 `EXPECTED_FINGERPRINT`，不符停止 authority 并失败，未配置默认拒绝；备份默认 age/gpg 加密并清理明文，例外需显式确认；Caddy 清理与格式化。
## 请重点审查
明文清理是否在所有失败路径执行（加密命令失败、磁盘满、中断）；恢复失败后服务处于什么状态（是否可能留下错误身份在运行）；身份私钥文件路径与权限；gpg 模式下的非交互与信任模型。
