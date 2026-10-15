# P6a 验证与自有文件清单

执行日期：2026-10-15。结果是执行报告，模块仍为 BLOCKED，详见 [行为、配置、接口与阻塞](p6a-backend.md)。

## 实际验证命令

Cargo 命令在 `src-tauri/` 执行，npx 在仓库根执行。没有 commit/push，没有新增依赖。

| 命令 | 最终真实结果 | 日志 |
| --- | --- | --- |
| `cargo check --offline` | 退出 0，0 警告，桌面壳 | `/tmp/p6a-desktop-check-final.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0，0 警告，实际服务端入口 | `/tmp/p6a-server-check-final.log` |
| `cargo check --offline --no-default-features --features server,collab-net` | 实施期间通过，退出 0，0 警告；最终服务端以实际入口复核 | `/tmp/p6a-server-check.log` |
| `cargo check -p polaris-collab --features collab-host,collab-net --offline` | 实施期间通过，退出 0，0 警告 | `/tmp/p6a-collab-check.log` |
| `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | 退出 0；124 passed，0 failed，0 ignored；含下述门控短路 | `/tmp/p6a-collab-tests-final.log` |
| `cargo test -p polaris-kernel --lib --tests --offline` | 退出 101；216 passed、6 failed、2 ignored；库测试失败使集成测试未继续 | `/tmp/p6a-kernel-tests-final.log` |
| `cargo test -p polaris-kernel --test backend_config --test provider_switch --test catalog_protocol --test credential_lock_interop --offline` | 补跑集成测试，退出 0；4+1+1+2=8 passed | `/tmp/p6a-kernel-integration.log` |
| `npx vue-tsc --noEmit` | 退出 0，无输出 | `/tmp/p6a-vue-tsc-final.log` |
| `git diff --check -- src-tauri/crates/polaris-collab src-tauri/src/mesh.rs src-tauri/src/desktop_commands.rs docs/feature-tree.md` | 退出 0 | 终端输出 |
| `rustfmt --edition 2021 --config skip_children=true <本轮新增 Rust 文件>` | 退出 1，当前 stable 工具链未安装 rustfmt；未下载组件 | 终端输出 |

实施中的错误与修复：首次 crate check 错在仓库根运行（无 Cargo.toml），随后切到 src-tauri 重跑；拆分后的 Tauri 命令宏未一并导出、测试用 RelayMode import 缺失、gateway 子模块相对路径错误均已修正并通过最终检查。
执行期间其他任务的 kernel experts 临时缺少 trait 导致一次 E0599；之后并行修改已修复，未由本任务改动内核。最终仍存在的失败是下面 6 项端口测试，不能把全量 kernel 测试声称为通过。

## 未实际运行的协作网络断言

原有两个测试在没有 `POLARIS_NET_TEST=1` 时直接 return，因此虽计入 passed，未验证实际网络连通：

- `collab::tunnel::tests::loopback_echo_roundtrip`
- `collab::tunnel::tests::mdns_lan_only_roundtrip`

本次未强行启用；未执行真实 relay、跨机器挂载、网关重连部署探针。新增 relay pending 测试验证策略与使用者生命周期，不能替代真实 Endpoint 网络重连测试。

## 内核失败名与真实错误

- `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`：bind 假上游，`PermissionDenied / Operation not permitted`。
- `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`：同上。
- `provider::claude_login::tests::loopback_run_end_to_end_local`：loopback bind，`PermissionDenied / Operation not permitted`。
- `provider::claude_login::tests::loopback_run_rejects_wrong_state`：同上。
- `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`：无法在 8765–8789 间绑定本地端口。
- `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`：同上。

前四项直接证实沙箱拒绝绑定；后两项报告端口范围绑定失败，未进一步改变沙箱或占用端口状态。
策略 B 的立即读写、通知吊销、审批/超时、单设备豁免测试未编写/未运行，原冷静期测试继续通过；不得拿旧测试当作策略 B 验收。

## 新增行为测试

- 随包占位/本地模式、配置优先级、显式清空、未知字段/类型/URL/短指纹拒绝。
- 多指纹接受与陌生指纹拒绝、已缓存 pin 在配置变更后重新校验、无指纹首次接触在联网前拒绝。
- 真正调用登录、登记、报到 handler，断言三处 relay 一致且身份断言仍为 300 秒。
- 空 relay 与本机 fallback、手工覆盖/环境覆盖、非法响应拒绝、旧 Endpoint 策略 pending 随最后使用者释放而消失。
- 编排每一步失败都不执行后续步骤；登记后重试从 Discover 开始，不再次轮换设备密钥。

## 自有文件与物理行数

下表只计本轮 P6a 文件；执行期间出现的 kernel、运行时前端、R8 文档及 execution/reviews 文件未改写、未纳入本轮成果。
既有长文件保留未触及职责；新文件均不超过 500 行，每行不超过 500 字符。新增 Rust 文件没有 `.unwrap()`/`.expect()`。

| 文件 | 行数 | 最大行长 | 行为/职责 |
| --- | ---: | ---: | --- |
| `docs/feature-tree.md` | 485 | 369 | 追加 P6a 入口、依赖、配置、迁移、验证和阻塞 |
| `docs/interconnect/p6a-backend.md` | 94 | 191 | 执行报告、配置切换、前端接口和未完成项 |
| `docs/interconnect/p6a-validation.md` | 89 | 211 | 真实命令、测试限制与文件清单 |
| `src-tauri/crates/polaris-collab/src/collab/authority.rs` | 1339 | 112 | 信任职责拆分，代登录消费 relay |
| `src-tauri/crates/polaris-collab/src/collab/authority/trust.rs` | 181 | 98 | 权威配置选择及首次/缓存 pin 校验 |
| `src-tauri/crates/polaris-collab/src/collab/gateway.rs` | 523 | 109 | 网关 Endpoint 类型与装配 |
| `src-tauri/crates/polaris-collab/src/collab/gateway/endpoint.rs` | 35 | 123 | 网关采用 relay 策略并延迟重建 |
| `src-tauri/crates/polaris-collab/src/collab/http.rs` | 4507 | 125 | 账号、mesh handler 职责拆分 |
| `src-tauri/crates/polaris-collab/src/collab/http/account_routes.rs` | 186 | 99 | 账号信息、公钥指纹、登录 relay 响应 |
| `src-tauri/crates/polaris-collab/src/collab/http/mesh_routes.rs` | 150 | 94 | 登记、报到 relay 响应及既有目录路由 |
| `src-tauri/crates/polaris-collab/src/collab/http/interconnect_tests.rs` | 36 | 98 | 登录→登记→报到响应行为测试 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/defaults.json` | 10 | 31 | 随包占位默认 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/schema.json` | 34 | 81 | 用户覆盖 schema |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/mod.rs` | 179 | 98 | 配置解析、指纹和 relay 响应接口 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/login.rs` | 87 | 95 | 可测试的顺序编排和进度契约 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/risk_inventory.rs` | 51 | 108 | 高风险入口清单，未执行策略 B 授权 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/test_env.rs` | 45 | 103 | 串行且恢复环境变量的测试夹具 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/tests.rs` | 96 | 99 | 配置和指纹行为测试 |
| `src-tauri/crates/polaris-collab/src/collab/mod.rs` | 50 | 79 | 公开互联模块 |
| `src-tauri/crates/polaris-collab/src/collab/tunnel.rs` | 1022 | 107 | 共享 Endpoint 生命周期及 relay 状态装配 |
| `src-tauri/crates/polaris-collab/src/collab/tunnel/endpoint.rs` | 79 | 122 | Endpoint 构建、最后释放后重建 |
| `src-tauri/crates/polaris-collab/src/collab/tunnel/relay.rs` | 196 | 100 | relay 策略、覆盖优先级、pending 状态与测试 |
| `src-tauri/src/desktop_commands.rs` | 354 | 79 | 注册本机密码登录命令 |
| `src-tauri/src/mesh.rs` | 1408 | 122 | 移除硬编码权威，报到/连接装配和状态 |
| `src-tauri/src/mesh/login.rs` | 250 | 97 | 三条登录入口与登记适配 |
| `src-tauri/src/mesh/orchestration.rs` | 109 | 101 | 登记、清单、relay、连接的真实 I/O 编排 |

技术债：authority.rs 的签发/成员准入/历史测试、http.rs 其他业务 handler、tunnel.rs 数据转发/旧测试、gateway.rs 转发业务、src/mesh.rs 台账/设备码/连接逻辑仍超长。
未修改的 auth.rs（1534 行）、grants.rs、identity.rs、hosting.rs、beam.rs 等原有职责仍保持原状，不把大规模拆分混入本模块。
