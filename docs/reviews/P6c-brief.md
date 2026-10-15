# 审查 brief：P6c 新加坡部署包（账号权威 + iroh-relay），只产出不部署
附 `deploy/singapore/` 全部文件（含测试）。
## 验收条件
权威（polaris-server，collab-net，权威模式）+ iroh-relay（版本与仓库 Cargo.lock 一致）+ 反向代理与证书；`id` 可橙云、`relay` 必须灰云（Workers/橙云代理无法承载 QUIC/UDP 与长连接）；健康检查、自动重启、日志轮转、资源上限；镜像与二进制固定版本；`.env.example` 无真实密钥；`init-keys.sh` 输出公钥指纹；备份/恢复（密钥需加密保存提示）、升级（先权威后成员）、自检脚本；脚本 `set -euo pipefail` 且幂等；中文 README 覆盖端口、DNS、首次部署、回写客户端配置、备份恢复升级回滚、排障。
## Claude 实测
`python3 -m unittest discover -s deploy/singapore/tests` 19 项通过；所有脚本 `bash -n` 通过、`shellcheck -S warning` 无告警；`docker compose config`：未填镜像 digest 时按设计拒绝，用占位值填充后通过。未运行真实镜像构建、证书签发、公网 smoke。
## 请重点审查
密钥与数据库卷权限、备份文件是否可能明文泄露签名私钥；HAProxy TLS 透传与 Caddy 的分流是否会让 relay 或权威意外暴露明文 HTTP；防火墙/端口最小化；升级与恢复脚本是否可能覆盖现有密钥（不可逆）；smoke 脚本是否会在生产留下测试数据；幂等性。
