# P6c 执行记录

日期：2026-10-15。只产出 `deploy/singapore/` 和功能树入口；未连接用户服务器，未部署，未 commit/push。
执行报告不等于验收，最终由 Claude 重跑、Pi 独立审查后决定。

## 文件与行为

| 文件（相对本目录） | 行为 |
| --- | --- |
| `docker-compose.yml` | authority、iroh-relay、Caddy、HAProxy；内网权威、四层 TLS 分流、UDP 直达、持久卷、健康/重启/日志/资源限制 |
| `.env.example`、`.gitignore` | 所有运维参数的默认值/注释；忽略本地密钥配置、快照、生成配置与 Python 缓存 |
| `Dockerfile.relay` | 官方 1.0.1 crate SHA-256 校验、locked 构建、记录成品校验和；运行镜像按外部发布 digest 固定 |
| `templates/{Caddyfile,haproxy.cfg,relay.toml}` | ACME、SNI 与 PROXY v2、HTTP 请求和 relay 官方流量限制；不含私钥 |
| `scripts/config.py`、`prepare.sh` | 参数类型/范围校验、拒绝未知项与浮动镜像、生成配置；无 shell source；阻止环境变量覆盖已验证输入 |
| `scripts/common.sh`、`compose.sh` | 统一 Compose 接口、固定项目身份、维护互斥、停止/恢复原运行状态 |
| `scripts/init-keys.sh` | 调用已有 pubkey API 生成/复用签名身份；打印完整指纹，已有库丢密钥时失败 |
| `scripts/backup.sh`、`snapshot.py` | 停机一致性快照、归并 WAL、SQLite 完整性检查、校验清单、明文私钥加密保存提示 |
| `scripts/restore.sh` | 停机前校验归档，备份现状，删除旧 WAL/SHM 后恢复；失败保持停止，可重复恢复 |
| `scripts/upgrade.sh` | 拉审核镜像→备份→只升级权威→健康与公网指纹检查；提醒随后升级成员和重新登录 |
| `scripts/smoke.sh`、`probe.py` | HTTPS/模式/完整公钥指纹；可选测试账号；relay HTTPS 与 UDP QUIC 响应匹配，明确未覆盖双端中继 |
| `client-config.template.json` | 不能误发版的指纹占位；实际配置由 prepare 在得到可信指纹后生成 |
| `tests/{test_behavior.py,fake_docker.py,test_static.py}` | 临时目录/SQLite/伪 Docker 行为测试，YAML/TOML/端口/版本/文件限制静态测试 |
| `README.md`、`VALIDATION.md` | 中文拓扑、端口、DNS、构建/部署/备份/升级/回滚、客户端交接和实际验证限制 |

`docs/feature-tree.md` 增加一行入口，移除一个多余空行以维持 500 物理行。
保留工作区其他模块及并发写入的改动，没有修改 Rust、前端、Cargo/npm 依赖或生产客户端默认配置。
Python 脚本只用标准库；静态 YAML 测试使用环境已有 PyYAML 6.0.1，未安装依赖。

## 实际命令与结果

| 命令 | 真实结果 |
| --- | --- |
| `docker compose version` | 退出 1；PATH 中有 Windows Docker 启动器，但当前 WSL distro 未启用集成 |
| `docker compose -f deploy/singapore/docker-compose.yml config --quiet` | 同一环境限制，未能运行 Compose 原生校验 |
| `python3 -c 'import yaml; print(yaml.__version__)'` | 退出 0，PyYAML 6.0.1 可用 |
| `for file in deploy/singapore/scripts/*.sh; do bash -n "$file" || exit; done` | 退出 0，所有 shell 脚本语法通过 |
| `command -v shellcheck` | 不存在；shellcheck 未运行，不标记通过 |
| `python3 -m unittest discover -s deploy/singapore/tests -p 'test_*.py' -v` | 最终 19 项通过；见下方范围 |
| `sha256sum /home/mi/.cargo/registry/cache/*/iroh-relay-1.0.1.crate` | 匹配 `9f16a5505939f9250297ff2f1210b5142d13618f496eab03ce3f964fc47e2e2b` 与 Cargo.lock |
| `git diff --check -- deploy/singapore docs/feature-tree.md` | 退出 0 |
| `curl -I --max-time 15 https://registry-1.docker.io/v2/` | curl 报错 7，环境预设本地代理连接失败；未能核验任何镜像 digest |

测试过程：首批 16 项通过；加入升级顺序测试后，两次 18 项运行都出现测试桩读取裸 `compose version` 参数时的 `IndexError`。
修正测试桩的参数解析后 18 项通过；加入环境变量覆盖防护测试后最终 19 项通过。
这些通过输出中的 HTTPS/注册 PASS 来自显式 mock，**不是曾连接真实 HTTPS 或创建真实账号**。

19 项覆盖：参数默认/切换/拒绝、模板生成/权限与过期指纹清理、dotenv 不执行 shell、Compose 环境优先级、
带 WAL 的备份与原运行状态、复制失败重启原服务、缺私钥拒绝初始化、重复恢复/恢复失败停机、恶意归档拒绝且不调用 Docker、
升级 pull 失败不停止服务、升级顺序与作用范围、完整指纹不匹配、QUIC 响应匹配、指纹失败不发凭据、可选注册复跑、
Compose 暴露面/运维配置、relay 1.0.1 官方配置层级/lock 校验和、所有自有文件 ≤500 行且每行 ≤500 字符。

静态 YAML 校验只解析与检查契约，不能替代 Docker Compose 完整 schema/interpolation 或运行时验证。
官网检索核对了 Caddy PROXY wrapper/HTTP-01、HAProxy 3.2 发布目录和 Compose dotenv 行为；来源链接见 README。
iroh 1.0.1 官网页面抓取失败，版本专用字段以校验和匹配的本地官方 crate 为依据。

## 未执行与阻塞

- **BLOCKED：成品固定值缺失。** 没有审核后的 authority 与 relay 镜像版本/digest；没有编造 digest。
  relay 已固定官方源码版本和校验和，并提供记录二进制 SHA-256 的构建步骤；它不等价于已有成品可直接部署。
- **BLOCKED：本地容器验证不可执行。** 未构建/拉取/启动镜像、运行原生 Compose/Caddy/HAProxy 校验或容器内健康检查。
  固定 tag 的远端可拉取性、CPU 架构、实际权限、ACME 首次签发/续期仍待目标环境验证。
- 未执行真实数据库迁移或恢复演练；行为测试使用临时 SQLite 库与假 Docker，不是生产数据库。
- 未运行 SMTP、验证码/密码找回、真实注册/登录、TLS/UDP 公网自检、两端 iroh 中继或完整 QUIC 地址发现。
  UDP smoke 只证明收到匹配的 QUIC 版本协商响应，不承诺完整中继业务可用。
- 邮件服务端固定额度没有参数接口，本任务仅提供代理/relay 频控；要调整服务端邮件常量另开模块。
- 未运行全量 Rust/前端测试：本任务未改应用代码或依赖，测试仅针对部署模块。
- 未替用户填域名/IP/SMTP/真实指纹；未修改客户端生产 defaults；这些是实际部署与发布交接输入。

待用户/发布负责人提供：域名、ACME 邮箱、服务器 OS/架构/公网与防火墙条件、镜像仓库及两个审核成品 digest、
SMTP 与注册策略、专用测试账号、带宽预算、加密备份路径和保留策略、客户端发布负责人。
真实指纹必须在服务器运行 init 后通过可信渠道交接。详见 README 的拓扑、端口清单和首次部署步骤。
