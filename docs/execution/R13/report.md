# R13 执行报告：频控来源、原子验码、后台邮件容量

本报告是 Codex 的实现与本地验证记录，不是模块最终验收；由 Claude 重跑、Pi 独立审查后裁定。
已先读 `docs/reviews/P6b-round1-triage.md` 第 2 轮。未执行 commit/push，未改动已有的
`docs/execution/P7a-1/`、`docs/execution/R13-1/`、`drills/` 用户未跟踪内容。

## 行为与入口

- `http/source_ip.rs` 提供所有相关 HTTP 频控共用的 `client_ip`。默认取 ConnectInfo 的 TCP 对端；
  没有连接元数据时归入非空 `unknown-peer` 桶，不再跳过 IP 频控。仅可信 CIDR 内的对端可提供转发头。
  XFF 优先于 X-Real-IP，从右向左扫描，遇首个不可信地址即停止；重复 XFF 头也按链顺序处理。
  畸形右侧链、全可信链回落 TCP 对端，不使用最左伪造值或再回落 X-Real-IP；IPv4 mapped IPv6 来源规范化。
- `http/login_routes.rs` 的权威发码、`http/email_routes.rs` 的匿名发码均传入 TCP 对端。
  `http/rate_limits.rs` 的密码登录、策略 B 等频控也改用同一解析器，反代客户端不再共享代理 IP 桶。
- 服务端 `src-tauri/src/server.rs` 已启用 `into_make_service_with_connect_info::<SocketAddr>()`，未修改。
  桌面原 `hosting.rs` 为 606 行；仅将 `start_core` 服务装配及其随机口令辅助函数移入 `hosting/serve.rs`，
  保留公开 `hosting::start_core`，启用相同 ConnectInfo；拆分后 `hosting.rs` 为 485 行。
- `mail/verify.rs` 使用 `TransactionBehavior::Immediate`，即 SQLite `BEGIN IMMEDIATE`。
  条件 UPDATE 同时检查有效期与 `attempts < MAX_ATTEMPTS`，受影响行数必须为 1 才进入验码。
  错码提交尝试次数；正确码条件 DELETE，受影响行数必须为 1 才成功。更新、读取、删除、提交错误全部返回。
  有效码只有一个消费者；上限为 5 次尝试。耗尽/过期记录保留供后续发码覆盖；无 schema 迁移。
- `mail/delivery.rs` 用两组信号量限制已接纳总量与实际执行数；容量在账号查询之前检查，后台
  lookup/hash/smtp/store 都受许可约束。队列满返回和正常接纳一样的 `200 {"ok":true}`，
  记录 `queue_full` 告警；已消耗频控额度，不自动重试。不会按账号是否存在区分响应。
- 每个后台任务有进程内关联 ID、固定阶段和固定事件（queue_full/timeout/failed/worker_failed）。
  不输出邮件地址、验证码、SQL/SMTP 原始错误；旧匿名响应与发信分支保持不变。

## 配置、依赖与迁移

沿用 `interconnect::resolve/load`：内置默认值合并用户 JSON，类型/范围/未知字段严格校验。
读取 `POLARIS_INTERCONNECT_CONFIG` 指向的文件，默认路径仍为 `~/Polaris/data/interconnect.json`。
无新依赖，未修改 Cargo.toml/Cargo.lock。

| 配置 | 默认 | 校验/生效 |
| --- | --- | --- |
| `ingress.trustedProxyCidrs` | `[]` | IPv4/IPv6 CIDR 字符串数组，前缀 0..32/0..128；每请求读取；配置无效拒绝请求 |
| `mailDelivery.capacity` | 32 | 整数 1..1024，包含运行中任务 |
| `mailDelivery.concurrency` | 2 | 整数 1..64，且不超过 capacity |
| `mailDelivery.timeoutSecs` | 30 | 整数 1..300，含排队与各后台阶段；投递池首次使用时读取，变更后重启 |

配置示例（仅在实际受控反代场景使用对应网段）：

```json
{
  "ingress": {"trustedProxyCidrs": ["172.29.10.0/24", "172.29.11.0/24"]},
  "mailDelivery": {"capacity": 64, "concurrency": 4, "timeoutSecs": 45}
}
```

恢复直连默认：设 `trustedProxyCidrs: []`；移除 mailDelivery 覆盖并重启恢复 32/2/30。
旧 JSON 缺新增字段会补默认，无须改数据库或手工补字段。旧 `POLARIS_TRUST_PROXY` 不替代 CIDR 配置。

新加坡 `prepare` 的 `authority.json` 信任 Caddy 所在 edge 和直接对端 HAProxy 所在 account 网络，
Compose IPAM 与 JSON 使用同一组 `EDGE_SUBNET`/`ACCOUNT_SUBNET` 输入；默认
`172.30.80.0/24`/`172.30.81.0/24`，校验不重叠私有 IPv4 `/16../28`。
Caddy 显式覆盖 X-Real-IP 与 XFF。客户端配置不包含代理信任和邮件后台策略。
`.env` 同时提供 `MAIL_DELIVERY_CAPACITY/CONCURRENCY/TIMEOUT_SECS`，默认与 Rust 相同。
旧部署要在维护窗口重建使用固定 IPAM 的 Compose 网络；保留数据卷。详见部署 README。

## 期限边界

同步 lettre SMTP/系统 DNS 和已经启动的 spawn_blocking 无法安全强制取消。任务到期限时记录超时，
排队任务立即释放容量；执行中的任务仍持有实际并发及容量许可直到返回，不能通过超时制造新阻塞线程。
SMTP I/O 超时使用剩余预算；每阶段前后检查期限，超时 SMTP 返回后不落新码，存储锁等待也使用剩余预算。
因此这提供后台逻辑期限与实际并发上限，不承诺操作系统级 SMTP/DNS 在该秒数内强制退出；
超时邮件可能已经发出，但不会保存可用的新码。这一边界在测试与 README 中明确记录。
本次有界后台入口是 brief 指定的 `/api/collab/email/send_code`；原同步 `/api/account/send_code`
已更新来源 IP，仍保持同步响应模式。

## 实际验证

以下 Cargo 命令均在 `src-tauri/` 执行，Python 命令在仓库根目录执行。对应完整日志在本目录。

| 命令 | 真实结果 | 日志 |
| --- | --- | --- |
| `cargo check -p polaris-app --offline` | 退出 0，0 警告 | [desktop](check-desktop.log) |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0，0 警告 | [server-net](check-server-net.log) |
| `cargo check -p polaris-cli --bin polaris-server --offline` | 退出 0，0 警告 | [server](check-server.log) |
| `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | 退出 0；176 passed，0 failed；0 警告 | [collab tests](collab-tests.log) |
| `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s deploy/singapore/tests -p 'test_*.py'` | 退出 0；35 tests，OK | [deployment tests](python-tests.log) |
| `cargo check -p polaris-collab --features collab-host,collab-net --offline` | 退出 0，0 警告 | [collab check](check-collab.log) |
| `git diff --check` | 退出 0 | 无空白错误 |
| Python 检查本次变更文件物理行/每行字符数，并扫描新增 Rust `.unwrap()`/`.expect()` | 退出 0；每文件 ≤500 行、每行 ≤500 字符，新增 Rust 无上述调用 | [检查脚本](check_limits.py) |

过程记录：首次 crate check、首次 desktop check 误从仓库根执行，均退出 101（根无 Cargo.toml）；
随后在 `src-tauri/` 正确重跑通过。Python 首轮在新增 R13 部署测试前为 33 tests/OK，最终为 35 tests/OK。
除这些工作目录错误外，最终所列测试没有失败后跳过的条目。

新增/调整行为测试覆盖：直连伪造头、可信代理 X-Real-IP/XFF、多跳、重复头、伪造最左值、畸形链、
缺 ConnectInfo、IPv4 mapped 来源、CIDR/配置类型校验；真实路由中间件来源频控；8 线程竞争同一码
只有一个成功、错码尝试恰好封顶 5、UPDATE/DELETE 故障回滚；满队列所有用途/账号状态响应一致；
排队超时释放、执行超时不提前释放许可、SMTP 超时不存码且返回后容量恢复；脱敏阶段；生成配置与 IPAM 一致。

## 未运行及交接

未进行真实 SMTP 投递、Docker 容器/网络重建、Caddy/HAProxy 原生配置验证或公网部署。
离线部署测试使用隔离临时目录和既有假 Docker/HTTPS 服务，日志里的 HTTPS PASS 不表示公网已验收。
本地指定命令均通过，无工具/编译阻塞；阻塞 SMTP 的取消边界如上。Claude 的复跑和 Pi 的独立审查尚未执行。
完整改动文件及职责见 [files.md](files.md)。
