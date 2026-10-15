# 任务 R13：频控来源 IP、验证码原子消费、投递并发上限（安全加固）
先读 `docs/reviews/P6b-round1-triage.md` 第 2 轮。
1. **可信代理与来源 IP（高）**：`collab/http/login_routes.rs` 的 `client_ip` 改为：默认取 TCP 对端地址（axum `ConnectInfo<SocketAddr>`，确认服务端与桌面托管的 serve 调用启用了 connect info）；仅当对端在配置的可信代理 CIDR 列表内时，才采信 `X-Real-IP`/`X-Forwarded-For`（XFF 自右向左取第一个不在可信列表内的地址）。可信列表进配置（默认空 = 不信任任何转发头）。所有调用 `client_ip` 的频控点同步改用新函数。部署包：`deploy/singapore` 生成的权威配置把 Caddy 所在 Docker 网络设为可信代理，并在 README 写明。测试：直连伪造头不生效；可信代理转发时取真实客户端；XFF 多跳与伪造最左值。
2. **验证码原子消费（中，安全）**：`collab/mail.rs` 的验证流程改为单事务（`BEGIN IMMEDIATE`）：条件更新尝试次数（`WHERE attempts < max`）并检查受影响行数，成功时条件删除并以受影响行数=1 判定唯一消费者；不得忽略更新/删除错误。测试：并发提交同一有效码仅一个成功；并发错误码不超过次数上限。
3. **投递并发上限（中）**：`collab/http/email_routes.rs` 后台投递改为有界队列或信号量（容量、并发、单次投递期限进配置），队列满时对外响应与正常相同（不泄露账号状态），并记录告警；测试：队列满时行为、超时释放。
4. **日志阶段（低）**：后台失败记录脱敏的阶段标识（lookup/hash/smtp/store）与关联 ID，不含邮箱、验证码。
允许修改：`src-tauri/crates/polaris-collab/src/collab/`、宿主中 axum serve 装配（`src-tauri/src/`，仅为启用 connect info）、`deploy/singapore/`、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；三种构建 0 警告。
必须通过：三种 `cargo check --offline`；`cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`；`PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s deploy/singapore/tests -p 'test_*.py'`。不 commit。
