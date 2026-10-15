# 任务 P6c：新加坡服务器部署包（账号权威 + iroh-relay），只产出不部署

用户决定：权威与 iroh-relay 部署在用户的新加坡服务器；Cloudflare 只做 DNS/TLS（可选橙云挡在权威 HTTP 前）。**iroh-relay 不能放 Cloudflare Workers，也不能走橙云代理**（需要长连接与 QUIC/UDP）。本任务只产出部署包、脚本与文档，不连接任何服务器。

## 产出（`deploy/singapore/`）
1. `docker-compose.yml`：
   - `authority`：Polaris 服务端壳（`polaris-server`，`collab-net` 特性），以账号权威模式运行（`POLARIS_ACCOUNT_AUTHORITY=1`），签名私钥与数据库在持久卷；只监听容器内网，经反向代理暴露 HTTPS。
   - `iroh-relay`：官方 iroh-relay（版本与仓库 `Cargo.lock` 中 iroh 版本兼容，写明依据；镜像或二进制固定版本与校验和），开放其所需端口（HTTPS/TCP 与 QUIC/UDP，按 iroh-relay 官方文档列出），自带或经反向代理获取证书——按官方推荐方式，并说明为何选该方式。
   - 反向代理（如 Caddy，自动 ACME 证书）：`id.<域名>` → authority，`relay.<域名>` → iroh-relay（如官方要求直连则不经代理，写明）。
   - 健康检查、`restart: unless-stopped`、日志轮转、资源上限。
2. `.env.example`：域名、邮件（SMTP，注册/找回密码需要）、relay 列表、频控、备份路径等全部可配置项，带注释；不含任何真实密钥。
3. `scripts/`：
   - `init-keys.sh`：首次生成权威签名密钥（若服务端已有生成逻辑则调用之），**输出公钥指纹**，供填入客户端随包配置 `authority.publicKeyFingerprints`。
   - `backup.sh` / `restore.sh`：权威数据库与签名密钥的备份恢复（密钥备份需提示加密保存）。
   - `upgrade.sh`：拉新镜像、先权威后成员的升级顺序说明（见 `docs/interconnect/p6a2-policy-b.md` 部署顺序）。
   - `smoke.sh`：部署后自检——权威 HTTPS 可达、公钥指纹与预期一致、注册/登录 API 可用（用测试账号，可选）、relay 端口可达（TCP 与 UDP）。
4. `README.md`（中文）：服务器最低配置、防火墙端口、DNS 记录（Cloudflare 上 `id` 可橙云、`relay` 必须灰云）、首次部署步骤、把指纹与域名写回客户端随包配置的步骤、备份/恢复/升级/回滚、故障排查。
5. 客户端随包配置模板：说明部署后要改的字段（`authority.url`、`publicKeyFingerprints`、relay 列表默认值）以及由谁、在哪个文件改。

## 规则
- 不新增代码依赖；镜像与二进制固定版本；脚本 `set -euo pipefail`、可重复执行（幂等）；不在仓库中写入任何密钥、域名真实值、IP。
- 用 `docker compose config` 校验 compose 语法（若本地无 Docker，用其他方式静态校验并说明）；脚本用 `bash -n` 与 shellcheck（若可用）检查。
- 每文件 ≤500 行、每行 ≤500 字符。不 commit。
- 结束时输出：部署拓扑图（文字）、端口清单、iroh-relay 版本选择依据、待用户提供的信息清单。
