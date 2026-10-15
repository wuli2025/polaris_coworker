# 新加坡账号权威与 iroh-relay 部署包（P6c）

本目录只提供部署材料，不连接服务器。执行报告见 [VALIDATION.md](VALIDATION.md)。
**当前部署就绪状态为 BLOCKED**：尚无经审核的 authority/relay 成品镜像 digest，真实域名、SMTP 与服务器条件也未提供；不能把离线测试通过当成已上线或最终验收。

## 拓扑与边界

```text
客户端 / 成员主机
  ├─ id.<域名>:443 → [可选 Cloudflare 橙云] → 新加坡 TCP 443
  │                                              ↓ HAProxy SNI + PROXY v2
  │                                            Caddy（ACME / TLS 终止）
  │                                              ↓ HTTP / 来源 IP 频控
  │                                            HAProxy 内部 :8081
  │                                              ↓
  │                                            authority :8080 → 持久库 + 签名私钥
  ├─ relay.<域名>:443 → Cloudflare 灰云 DNS → HAProxy SNI 字节透传
  │                                              ↓（保留 TLS / ALPN）
  │                                            iroh-relay :8443（自管 ACME）
  └─ relay.<域名>:7824/UDP ─────────────────────→ iroh-relay QUIC 地址发现
公网 TCP 80 → HAProxy → id: Caddy HTTP-01 / relay: captive portal HTTP
```

同一公网 IP 的 443 不能让两个容器同时发布，所以增加 HAProxy 四层 SNI 分流。
relay 的 TLS 在官方 relay 内终止；没有 Caddy HTTP 转发、Cloudflare Workers 或橙云。
UDP 直接发布到 relay，不经过 HAProxy。两个域名均用正常的无端口 HTTPS URL。
HAProxy 只向 Caddy 发送 PROXY v2，绝不向不支持它的 relay 注入该头。

authority 运行 `polaris-server`（`collab-net`），固定 `POLARIS_ACCOUNT_AUTHORITY=1`、
`POLARIS_AUTHORITY_ONLY=1`、`POLARIS_REQUIRE_LOGIN=1`，关闭主机隧道自启。
服务壳当前代码绑定 `0.0.0.0:8080`，因此隔离依靠容器网络与**不发布端口**，不虚构一个无效的 bind 环境变量。
Caddy 仅开放 `/api/account/*`、`/api/mesh/*`，其他路径 404；这里没有远程管理 Web 或项目执行入口。
不要将已有通用主机 compose 的端口、Docker socket 或自动更新 sidecar 叠加进来。

所有四个常驻服务有健康检查、`unless-stopped`、日志轮转、内存/CPU/PID 限制。
relay 本地健康检查仅证明 HTTP 进程活着；公网 TLS、UDP 必须另跑 smoke。
Docker 标记 unhealthy 本身不会自动重启容器，重启策略针对进程退出。

## 版本、证书与官方依据

- 仓库 `src-tauri/Cargo.lock` 的 `iroh` 与 `iroh-relay` **均为 1.0.1**，relay 选择同版，避免跨版本线协议猜测。
- `Dockerfile.relay` 从官方 crates.io 发布归档构建 `--locked --features server`，归档 SHA-256 固定为：
  `9f16a5505939f9250297ff2f1210b5142d13618f496eab03ce3f964fc47e2e2b`。
  该值同时匹配仓库 lock 与本地官方 crate 缓存；不能把源码归档校验和冒充成品二进制校验和。
- 构建阶段输出 `iroh-relay --version`；二进制校验和存入镜像 `/usr/local/share/iroh-relay.sha256`。
  部署输入 `RELAY_IMAGE` 强制包含版本标签及成品镜像 `@sha256:...`；尚未构建/取得 digest 时拒绝 prepare。
- authority 必须由包含 P6a-2 的审核源码构建，根 `Dockerfile` 已使用 `-p polaris-cli --bin polaris-server --features collab-net`。
  不能凭已有镜像的旧 tag 推断其包含本次协议。部署同样强制 `AUTHORITY_IMAGE` digest。
- Caddy 固定 `2.10.2-alpine`，HAProxy 固定 `3.2.26-alpine`；可在 `.env` 换明确版本或追加 digest。
  构建用 Rust `1.95.0-bookworm` 与 Debian `12.11-slim`。这些 tag 在本环境未拉取验证，发布负责人需审核成品与基础镜像。
- relay 使用官方内置 `cert_mode = "LetsEncrypt"`，不使用开发模式或自签证书。ACME TLS-ALPN-01 经 SNI 透传，证书缓存持久化到 `relay-certs`。
  同一证书由 relay 管理并供 HTTPS 与 QUIC 使用，避免外部导出/重载证书与权限问题。Caddy 为 id 单独申请证书，强制 HTTP-01，避免橙云截获 TLS-ALPN 挑战。

官方源码契约：[1.0.1 CLI 配置](https://docs.rs/crate/iroh-relay/1.0.1/source/src/main.rs)、
[1.0.1 服务实现](https://docs.rs/crate/iroh-relay/1.0.1/source/src/server.rs)、
[1.0.1 默认端口](https://docs.rs/crate/iroh-relay/1.0.1/source/src/defaults.rs)。
在线版本页面本环境抓取失败；字段与端口已直接核对校验和匹配的本地官方 1.0.1 crate。
特别注意 `https_bind_addr`、`quic_bind_addr` 属于 **`[tls]`**，不是 TOML 顶层。
当前版 QUIC 地址发现使用 UDP **7824**，不是旧 STUN 教程中的 3478。

HAProxy 版本发布日期见[官方 3.2 发布目录](https://www.haproxy.org/download/3.2/src/)。

参考：[Caddy 自动 HTTPS](https://caddyserver.com/docs/automatic-https)、
[Caddy PROXY listener](https://caddyserver.com/docs/caddyfile/options#listener-wrappers)、
[HAProxy SNI 获取](https://docs.haproxy.org/3.2/configuration.html#7.3.5-req.ssl_sni)。

## 主机、DNS 与防火墙

运行环境最低规划：Linux，2 vCPU、4 GiB 内存、20 GiB 可用持久 SSD，Docker Engine + Compose v2（支持 `--wait`），Bash、Python **3.11+**、`flock`、`realpath`。
备份宿主还须安装 `age`（默认）或 GnuPG：Debian/Ubuntu 运维可执行 `sudo apt-get install age` 或 `sudo apt-get install gnupg`。
这些是服务器侧工具，不加入 Cargo/npm/Python 依赖；工具缺失或加密失败不会退回明文。
这是低流量起点，不是压测容量承诺；镜像构建建议在独立构建机完成。relay 流量与出口带宽/流量配额通常更值得监控。
宿主准确同步时间。一个稳定公网地址即可，确认运营商允许入站 TCP/UDP 和出站 SMTP、ACME。

| 方向/端口 | 用途 | 说明 |
| --- | --- | --- |
| 入站 TCP 80 | id 的 ACME HTTP-01；relay HTTP captive portal | HAProxy 按 Host 路由；不关掉 HTTP 挑战 |
| 入站 TCP 443 | id HTTPS、relay HTTPS/长连接与 TLS-ALPN-01 | SNI 分流，relay 必须灰云；不可只允许 Cloudflare IP |
| 入站 UDP 7824 | relay QUIC 地址发现 | 直接映射同端口，防火墙和云安全组均放行 |
| 出站 TCP 443 / DNS | ACME、镜像下载和 DNS 解析 | 由系统/Docker 正常提供 |
| 出站 TCP 465 或配置的 SMTP_PORT | 验证码、找回密码邮件 | 服务端按 465 implicit TLS，其余 STARTTLS |
| SSH 管理端口 | 运维 | 自行限制来源；compose 不开放管理端口 |
| 仅容器内部 TCP 8080/8081/8443、2019/8404 | HTTP、relay TLS、管理/健康检查 | 不发布到宿主；无需开公网 |

Cloudflare 创建 `id` 和 `relay` 的 A（有真实 IPv6 才加 AAAA）记录，值均填服务器公网地址。
首次建议都设 DNS only。`relay` **永久灰云**；不能使用 Cloudflare Tunnel/Workers 承载。
`id` 可以在证书签发与 smoke 通过后橙云，TLS 模式 **Full (strict)**，不要用 Flexible。
对 `/api/*` 禁用缓存/交互式挑战，并保证 `/.well-known/acme-challenge/*` 可到 Caddy。
不要公布不可达的 AAAA，否则 ACME 或客户端可能优先走坏 IPv6。

## 发布与首次启动（以下均由运维在目标环境执行）

1. 发布负责人先确认源码版本与协议，通过现有根 `Dockerfile` 构建 authority；使用本目录 `Dockerfile.relay` 构建 relay。
   不增加 Cargo/npm 依赖；锁文件保持不动。示意（替换自己的 registry 与审核版本）：

   ```bash
   # 仓库根，构建机执行；本次任务没有运行这些命令。
   docker build -t registry.example.invalid/polaris-authority:p6c-reviewed .
   docker build -f deploy/singapore/Dockerfile.relay -t registry.example.invalid/iroh-relay:1.0.1 deploy/singapore
   ```

   按自己的发布流程分发镜像并记录 RepoDigest，核实镜像架构与服务器匹配。不能填示例假 digest。
   同架构可以跨服务器使用成品；跨架构需要分别构建/发布，不能硬搬二进制。
   如需核对 relay 成品，在构建机以 `--entrypoint sh` 执行：
   `cd /usr/local/bin && sha256sum -c /usr/local/share/iroh-relay.sha256`。
2. 将本目录复制到服务器固定目录，配置 DNS/防火墙。`cp .env.example .env && chmod 600 .env`。
   填域名、ACME 联系邮箱、两个镜像 digest、SMTP、备份位置。例中的 `.invalid` 只是占位，不能签证书。
   `.env` 为受限 dotenv 格式，脚本不会 `source`；不支持 `${变量}` 插值、多行值或行尾注释。
   密码包含 `$` 时用单引号包裹，程序会在生成的 Compose env 中保持字面量。
   运维脚本与 `scripts/compose.sh` 强制使用文件配置，避免 shell 中同名变量覆盖已验证镜像或项目卷身份。
3. 执行 `scripts/prepare.sh`；使用下面的数组处理后续 Compose 命令：

   ```bash
   dc=(./scripts/compose.sh)
   "${dc[@]}" config --quiet
   "${dc[@]}" pull
   # 在启动任何公网入口前做原生配置校验：
   "${dc[@]}" run --rm --no-deps gateway haproxy -c -f /usr/local/etc/haproxy/haproxy.cfg
   "${dc[@]}" run --rm --no-deps caddy caddy validate --config /etc/caddy/Caddyfile
   scripts/init-keys.sh
   ```

   init 只起 authority，无公开端口。通过 `/api/account/pubkey` 调用服务端已有密钥生成逻辑，打印完整 64 位 SHA-256。
   再次运行保留同一身份。发现已有库却没有私钥时拒绝运行，必须恢复原私钥，不能通过删除数据库“修复”。
   服务端无独立 keygen CLI；这里不添加第二套密钥算法。`kid` 只有 16 位，不能拿来配置客户端。
4. 把指纹填入 `.env` 的 `EXPECTED_FINGERPRINT`，重跑 `scripts/prepare.sh`。
   保护 volume、`.env` 与 Docker 管理权限。保持 `.generated/compose.env` 为 0600，代理模板是 0644 且不含密钥。
5. 首个账号：默认 `ACCOUNT_OPEN_SIGNUP=0`。若需要自助注册，先配好 SMTP，再显式设为 `1`、prepare 并重建 authority。
   若采用邀请制，可在公网入口尚未启动时临时开启注册，通过 `docker compose exec -T authority curl` 调内部 `/api/account/signup`，随后关闭注册并重建 authority。
   请求 JSON 通过标准输入提供 `username`、`password`、可选 `displayName`；不要把密码写入仓库/命令历史。
   不填邮箱可建普通账号；填邮箱必须先取得验证码。公开注册创建的是普通账号，不是 owner。
   此包不开放 `/api/collab/admin/*`；owner 引导、邀请制批量开户走项目已有受控本地管理流程，本任务不修改账号权限。
6. `"${dc[@]}" up -d --wait --wait-timeout 300`，等待两个域名证书签发。
   从服务器外另一网络执行 `scripts/smoke.sh`，确认真实防火墙路径。检查 stdout 的 **SKIP**，不可当作已测。
   需要测试注册/登录时填写独立的 `SMOKE_USERNAME/PASSWORD`；`SMOKE_SIGNUP=1` 会在登录 HTTP 400 后尝试注册。
   重跑优先登录既有账号，不重复创建；脚本不自动删除账号。邮箱验证码收取与找回密码需独立人工验收。

## 配置切换与客户端随包配置

完整参数、默认值与注释在 [.env.example](.env.example)，类型/范围由 `scripts/config.py` 校验。
切换后运行 prepare 并重建受影响服务：例如频控从每 IP 每 10 秒 120 次改成 60 次，修改 `AUTH_REQUESTS_PER_10S=60`，再 `"${dc[@]}" up -d --force-recreate gateway`。
自定义 relay 列表：`RELAY_URLS=https://relay.example.invalid,https://backup-relay.example.invalid`；需要公共 fallback 时显式设 `RELAY_FALLBACK=true`。
所有地址均为 HTTPS。可调整策略 B 的四个参数，但权威与成员必须一致；它们进入公开 `POLARIS_INTERCONNECT_CONFIG` JSON 接口。

频控有两层：HAProxy 按源 IP 限 HTTP 请求；relay 按官方 limits 限新连接与每客户端收包带宽。
默认 relay 是公开中继（`access="everyone"`），不等于账号私有中继；注意出口账单，私有鉴权要单独评审客户端兼容性。
Caddy 从受控 Docker 网络接受 PROXY v2，覆盖 XFF/X-Real-IP，忽略外来 CF-Connecting-IP。
橙云时按 Cloudflare 出口 IP 聚合，可能误伤共享出口；本包不凭不可信头绕过限速。
服务端邮件还有既有固定限制：60 秒重发间隔、每邮箱每小时 5 封、每 IP 每小时 20 封。
这些常量当前没有配置接口，**本任务不伪造可调环境变量**；要改需另开服务端模块。

部署运维负责把 `init-keys.sh` 输出与两域名通过可信渠道交给 **Claude 主控/客户端发布负责人**：

| 项 | 由谁修改/在哪改 |
| --- | --- |
| `authority.url` | 客户端发布负责人改 `src-tauri/crates/polaris-collab/src/collab/interconnect/defaults.json` 为 `https://id.<域名>` |
| `authority.publicKeyFingerprints` | 同文件写入 init 得到的完整指纹数组，不从未核实公网首次访问自动信任 |
| `relay.urls`、`relay.fallback` | 同文件写入自建 relay 列表；本包默认 false，不默默使用公共中继 |
| `security` | 同文件与服务器 `.generated/authority.json` 保持一致 |

[client-config.template.json](client-config.template.json) 明确使用不可通过验证的指纹占位；不可直接发版。
填写 EXPECTED_FINGERPRINT 后 prepare 会生成可用的 `.generated/client-interconnect.json`。
发布负责人审核后把相应字段合入随包 defaults，重新构建客户端；本次不修改随包生产默认值。
试运行可以不发版，在成员机器设置 `POLARIS_INTERCONNECT_CONFIG=/absolute/path/client-interconnect.json`。
环境变量优先于该 JSON 和随包默认；排查时清理过时的 `POLARIS_ACCOUNT_AUTHORITY_URL`、
`POLARIS_ACCOUNT_AUTHORITY_PUBLIC_KEY_FINGERPRINTS`、`POLARIS_RELAYS`、`POLARIS_RELAY_FALLBACK`。
已有钉住不同身份的设备仍需走项目的显式重新信任流程，修改默认值不能偷偷替换已钉的公钥。

## 备份、恢复、升级与回滚

- `scripts/backup.sh`：互斥锁→检查加密配置→记录运行状态→停止 authority→复制数据→SQLite backup API 归并 WAL→校验→加密并原子产出归档→清理明文→恢复原运行状态。
  备份仅含 `collab.db`、`account_authority.key` 和清单（含实际运行镜像引用与 SHA-256），不包含 SMTP 密码和 TLS 缓存。
  保存 `.env` 与 ACME volumes 需要独立的加密备份；数据库覆盖账号、吊销等权威状态，不能只备份主 DB 而漏 WAL。
  每次调用追加一份快照，不覆盖旧备份。默认要求 `BACKUP_ENCRYPT=age` 与 `BACKUP_RECIPIENT`；生成 `.tar.age`，也支持 gpg 的 `.tar.gpg`。
  未配置时拒绝；仅明确确认 `BACKUP_DIR` 是加密介质后设 `BACKUP_PLAINTEXT_ACK=1` 才允许 `.tar`，stderr 有醒目 WARNING。
  不默认设置自动删除保留策略；按容量与保留要求自行配置定时任务。
- `scripts/restore.sh /absolute/path/authority-....tar.age`：先检查预期指纹、解密并校验归档白名单、校验和、SQLite、32 字节种子格式，再备份现状、停机、恢复。
  接受 `.tar.age`、`.tar.gpg` 与历史 `.tar`；age 需 `BACKUP_IDENTITY`，gpg 用运维用户密钥环（非交互，私钥解锁须事先准备）。
  有现存数据库时，备份现状同样须配置收件人或明确确认明文例外；解密能力不能代替备份加密配置。
  拒绝路径穿越、重复成员、链接和未知文件；移除旧 WAL/SHM；失败保持停止，避免半恢复库上线。
  重启后从容器内 `/api/account/pubkey` 获取公钥，调用 `probe.py fingerprint` 比对 `EXPECTED_FINGERPRINT`。
  `RESTORE_VERIFY_TIMEOUT_SECS` 默认 30 秒，是 curl 的重试预算（`--retry-max-time`）；单次请求 `--max-time` 为 `min(10, 预算)` 秒。
  最后一次请求可能越过重试预算，因此 curl 请求等待总计至多预算加单次上限（默认 40 秒）；这不包含 Docker 调度、恢复和清理耗时。
  不符、无法取得公钥或响应无效均停止 authority 并非零退出；指引核对可信指纹与原备份，不能直接接受新指纹。
  无现有库时支持空卷恢复；若原容器停止，临时启动核验后恢复停止状态。公网 smoke 仍需运维另跑。
  缺少预期指纹默认在写入前拒绝；仅灾难恢复人工确认时可设 `RESTORE_UNPINNED_ACK=1`，会告警并输出实际指纹，不能证明身份可信。
  哈希只检查完整性，不证明备份来源可信，恢复文件应来自受控备份库。
- 升级前将 `.env` 的 `AUTHORITY_IMAGE` 改成审核的新版本 digest，再运行 `scripts/upgrade.sh`。
  脚本拉新权威镜像、备份现状、记录 `.generated/previous-authority-image.txt`、只升级 authority、等待健康并核对公网指纹。
  **先权威→再成员主机→再客户端重新登录**，依据 [P6a-2 部署顺序](../../docs/interconnect/p6a2-policy-b.md)。
  脚本不会擅自升级成员或 relay。旧会话不强制清空；旧权威缺新鲜清单会使高风险操作被拒绝。
- 升级失败返回非零，不自动降级数据库。回滚：保留故障快照，将 AUTHORITY_IMAGE 改回上次 digest，prepare；
  恢复与旧镜像配套的升级前快照，再 `up -d --no-deps --wait authority`、smoke。不要让旧版本直接写新 schema。
  恢复旧库可能恢复已经吊销的设备状态，需从故障快照/审计核对并重施吊销；不能把回滚当无损撤销。
- relay/代理维护另开窗口：先保留旧 digest/配置和证书卷，再 pull、逐个重建、完整 smoke；客户端两端互通仍要另测。
  避免 `docker compose down -v`，它会删除身份与数据库卷。

加密配置切换示例（修改 `.env`，不要仅设置同名 shell 环境变量）：

```dotenv
BACKUP_ENCRYPT=age
BACKUP_RECIPIENT=age1替换为可信收件人公钥
BACKUP_PLAINTEXT_ACK=0
BACKUP_IDENTITY=/secure/offline-recovery/age-identity.txt
BACKUP_CRYPTO_TIMEOUT_SECS=300
RESTORE_UNPINNED_ACK=0
RESTORE_VERIFY_TIMEOUT_SECS=30
```

age 备份只需公钥，恢复私钥应独立保管，恢复时再提供。切换 GnuPG：`BACKUP_ENCRYPT=gpg`，
`BACKUP_RECIPIENT` 填已导入密钥环且经独立核对的完整公钥指纹，`BACKUP_IDENTITY` 留空。
收件人由工具校验密钥格式；配置先拒绝空白/选项形式、未知加密模式，ACK 仅允许 `0/1`，identity 须绝对路径，恢复等待预算为 `1..300` 秒。
`BACKUP_CRYPTO_TIMEOUT_SECS` 为每次 age/gpg 加密或解密子进程的超时，默认 300 秒，必须是 `1..86400` 的整数。
大备份可在 `.env` 设置 `BACKUP_CRYPTO_TIMEOUT_SECS=600`；改回 `300` 或删除该项恢复默认。指纹重试预算可设 `RESTORE_VERIFY_TIMEOUT_SECS=60`（单次 10 秒，总请求等待至多 70 秒）。
子进程超时会被终止并回收，返回具名 `BackupCryptoTimeout`（含工具、加/解密操作和配置秒数）；沿用明文清理及部分密文删除，备份恢复原运行状态，解密失败在 Docker 操作前退出。
旧 `.env` 未含超时字段时自动合并 300 秒默认值；归档格式与密钥配置无需迁移。此限制不覆盖整个备份/恢复或外部工具管理的独立守护进程。
gpg 显式信任指定收件人，所以运维必须核对指纹；不会交互询问信任，也不支持本包传递私钥口令。
确需加密介质上的明文快照：`BACKUP_ENCRYPT=`、`BACKUP_RECIPIENT=`、`BACKUP_PLAINTEXT_ACK=1`；恢复默认策略时填回加密配置并将 ACK 改为 `0`。
历史 `.env` 自动合并新增默认值，因此旧部署未补加密配置时 backup/upgrade/恢复前备份会拒绝；历史归档内容格式无迁移。

复制、归并、校验与解密的临时明文放在系统临时目录（可通过 `TMPDIR` 指定），目录 0700、CLI 文件 0600。
成功和可捕获失败路径都会覆盖、fsync、删除临时文件，部分密文也会删除；备份失败恢复原运行状态，恢复写入失败保持停止。
CoW、SSD、日志文件系统不保证覆盖能抹除历史块，断电/SIGKILL 也无法执行清理；应使用加密盘或 tmpfs 的 `TMPDIR`，不能把删除当作介质级擦除。
例如为大库准备足够容量的受控加密临时盘，再执行 `TMPDIR=/mnt/encrypted-tmp ./scripts/backup.sh`（恢复同理）。
不要把恢复私钥放进快照或版本库；定期在隔离环境使用真实工具演练加密备份及解密恢复。

## 排查

- `prepare` 报 BLOCKED：补真实、审核过的镜像 digest。格式校验不能证明该镜像存在，也不替代签名/供应链审核。
- `curl` 不存在或 authority 启动失败：镜像不符合根 Dockerfile 的服务壳约定；检查构建版本、架构和 `collab-net`。
- relay TLS 无证书：确认灰云、SNI 域名、80/443/7824、时间、ACME 出站、CAA；查看 `"${dc[@]}" logs --tail 100 iroh-relay gateway`。
- id 签证失败：确认 80 的 HTTP-01 未被 Cloudflare 规则阻断；先灰云调试，不要关闭 TLS 验证。
- 指纹不符：停止后续升级/登录，核对卷是否换了、密钥是否丢失或域名是否指错；不得直接接受新指纹。
- UDP smoke 超时：检查云安全组、宿主 Docker 转发、NAT、错误 AAAA、7824/UDP 出入站；从外网复测。
  smoke 使用匹配随机连接 ID 的 QUIC 版本协商响应，不是 `nc -u` 的无响应成功；仍不能证明完整 QAD 或双端中继转发。
- 429 / 发码频繁：核对频控参数、是否橙云聚合来源，注意服务端固定邮件额度；别盲目信任客户端头。
- SMTP 不工作：检查全部 SMTP 项、服务商授权码、端口出站；无 SMTP 的用户名密码登录仍可用，但绑定邮箱/验证码找回不可验收。
- 运维脚本会自动恢复原本运行的 authority，恢复中断则保持停止；检查退出码，禁止忽略失败继续升级成员。

## 待提供与待验证

域名与 DNS 管理权限、ACME 邮箱、服务器 OS/架构/公网 IPv4/IPv6/防火墙条件、镜像仓库与两个成品 digest、
SMTP 参数与注册策略、独立测试账号、带宽预算、加密备份路径/保留策略、客户端发布负责人及指纹交接渠道。
以上信息不写入版本库。真实指纹只能由用户服务器初始化后取得。
本地 Docker 不可用且网络代理不可达：未构建、拉镜像、原生校验代理配置、签证书、发邮件或探测任何服务器。
Claude 重跑验证、Pi 独立审查后决定模块是否完成。
