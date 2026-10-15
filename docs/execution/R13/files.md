# R13 改动文件

路径相对仓库根目录；未改 Cargo 依赖或用户已有未跟踪文件。

| 文件 | 行为/职责 |
| --- | --- |
| `deploy/singapore/.env.example` | 部署 CIDR 和邮件容量/并发/期限默认值。 |
| `deploy/singapore/README.md` | 可信代理链、参数切换、迁移及期限边界。 |
| `deploy/singapore/docker-compose.yml` | edge/account 网络 IPAM 绑定显式子网配置。 |
| `deploy/singapore/scripts/config.py` | 校验子网与邮件参数，生成权威信任与投递配置。 |
| `deploy/singapore/templates/Caddyfile` | 显式覆盖 XFF。 |
| `deploy/singapore/tests/test_r13.py` | 生成配置、网络一致性、非法输入行为测试。 |
| `docs/execution/R13/check-collab.log` | 实际运行命令的原始完整日志。 |
| `docs/execution/R13/check-desktop.log` | 实际运行命令的原始完整日志。 |
| `docs/execution/R13/check-server-net.log` | 实际运行命令的原始完整日志。 |
| `docs/execution/R13/check-server.log` | 实际运行命令的原始完整日志。 |
| `docs/execution/R13/check_limits.py` | 可复跑的行数/行长/新增 unwrap/expect/日志警告检查。 |
| `docs/execution/R13/collab-tests.log` | 实际运行命令的原始完整日志。 |
| `docs/execution/R13/files.md` | 逐文件职责清单。 |
| `docs/execution/R13/python-tests.log` | 实际运行命令的原始完整日志。 |
| `docs/execution/R13/report.md` | 执行结果、配置迁移、未运行内容与交接边界。 |
| `docs/feature-tree.md` | 更新 R13 入口、依赖、配置、迁移和验证索引。 |
| `src-tauri/crates/polaris-collab/src/collab/http.rs` | 接入共用来源 IP 模块。 |
| `src-tauri/crates/polaris-collab/src/collab/http/email_routes.rs` | 后台改为有界投递池，传入 ConnectInfo。 |
| `src-tauri/crates/polaris-collab/src/collab/http/email_tests.rs` | 适配 sender 接口；队列满、超时/容量恢复行为测试。 |
| `src-tauri/crates/polaris-collab/src/collab/http/login_routes.rs` | 权威发码使用可信代理来源解析。 |
| `src-tauri/crates/polaris-collab/src/collab/http/rate_limit_tests.rs` | 请求中间件中伪造头和多客户端行为测试。 |
| `src-tauri/crates/polaris-collab/src/collab/http/rate_limits.rs` | 密码登录/策略 B 频控共用来源解析。 |
| `src-tauri/crates/polaris-collab/src/collab/http/source_ip.rs` | TCP 对端优先，可信 CIDR 与 XFF 反向解析。 |
| `src-tauri/crates/polaris-collab/src/collab/http/source_ip_tests.rs` | 直连/代理/伪造/缺元数据/CIDR 与配置类型测试。 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/defaults.json` | ingress 与 mailDelivery 默认配置。 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/hardening.rs` | 无新依赖的 CIDR 解析及投递策略类型/校验。 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/mod.rs` | 配置解析、合并及新策略校验。 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/schema.json` | 新配置字段 schema。 |
| `src-tauri/crates/polaris-collab/src/collab/mail.rs` | 保持公共入口，拆分验证码与投递；SMTP timeout 参数化。 |
| `src-tauri/crates/polaris-collab/src/collab/mail/code.rs` | 哈希、SMTP、落库阶段与期限检查。 |
| `src-tauri/crates/polaris-collab/src/collab/mail/delivery.rs` | 总容量/并发许可、期限、关联 ID 与脱敏告警。 |
| `src-tauri/crates/polaris-collab/src/collab/mail/delivery_tests.rs` | 满队列、排队/执行超时与许可、脱敏阶段行为测试。 |
| `src-tauri/crates/polaris-collab/src/collab/mail/requests.rs` | 后台 lookup/store/smtp 阶段和可注入 sender。 |
| `src-tauri/crates/polaris-collab/src/collab/mail/verify.rs` | IMMEDIATE 事务、条件计次、唯一消费及错误传播。 |
| `src-tauri/crates/polaris-collab/src/collab/mail/verify_tests.rs` | 并发成功/错码与 UPDATE/DELETE 错误回滚测试。 |
| `src-tauri/src/hosting.rs` | 保留 start_core 公共路径，服务装配拆分至 500 行内。 |
| `src-tauri/src/hosting/serve.rs` | 桌面托管 ConnectInfo serve 装配及原随机口令辅助函数。 |
