# P6a-2 文件与实际验证

这是执行报告，供 Claude 重跑与 Pi 审查；不替代最终验收。协议、配置、迁移和 P6b 接口见 [策略 B 报告](p6a2-policy-b.md)。没有 commit/push，没有新增依赖。工作区原有 R8、P6a 及前端改动均保留。

## 实际命令与结果

Cargo 命令在 `src-tauri/` 运行，或在仓库根加 `--manifest-path src-tauri/Cargo.toml`；前端命令在仓库根运行。

| 命令 | 真实结果 | 日志 |
| --- | --- | --- |
| `cargo check -p polaris-app --offline` | 最终退出 0，0 警告 | `/tmp/p6a2-final-desktop.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 最终退出 0，0 警告 | `/tmp/p6a2-final-server-net.log` |
| `cargo check -p polaris-cli --bin polaris-server --offline` | 最终退出 0，0 警告 | `/tmp/p6a2-final-server.log` |
| `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | 最终退出 0：137 passed，0 failed，0 ignored；0 编译警告 | `/tmp/p6a2-final-collab-tests.log` |
| `cargo test -p polaris-kernel -p polaris-wiki -p polaris-fable --lib --tests --offline` | 退出 101：kernel 218 passed、6 TCP 绑定失败、2 ignored；Cargo 因此未继续 wiki | `/tmp/p6a2-engine-tests.log` |
| 上一条加下列 6 个 `--skip` | 退出 0：338 passed，9 ignored，6 filtered；含 wiki 全部 7 项 | `/tmp/p6a2-engine-tests-filtered.log` |
| `npx vue-tsc --noEmit` | 退出 0 | `/tmp/p6a2-vue-tsc.log` |
| `cargo test … --lib interconnect::security_tests --offline` | 首轮新增核心测试 9 passed；之后纳入全量测试重跑 | `/tmp/p6a2-security-tests.log` |
| `git diff --check` | 退出 0 | 终端输出为空 |
| Python 文件物理行/字符扫描 | 本次新增文件均不超过 500 行；所有本次触及文件每行不超过 500 字符 | 下方文件表 |

两次早期 `cargo check` 误在没有 Cargo.toml 的仓库根运行，退出 101；随后已使用正确目录/manifest 重跑，未把这些未执行的编译当成通过。

kernel/fable/wiki 的测试编译存在原有 `polaris-fable/src/fable/scan.rs:1016` 的 `unused import: super::*` 警告。本次允许改动范围不含该实现文件，未修改。三种要求零警告的 `cargo check` 均为零警告。

## 沙箱 TCP 项

原始命令确实执行并失败的六项：

- `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`
- `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`
- `provider::claude_login::tests::loopback_run_end_to_end_local`
- `provider::claude_login::tests::loopback_run_rejects_wrong_state`
- `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`
- `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`

错误为 `PermissionDenied / Operation not permitted`，本地路由测试将绑定失败包装成“无法在 8765–8789 间绑定本地端口”。没有申请提权或将这些项改成通过。

实际重跑命令：

```sh
cargo test -p polaris-kernel -p polaris-wiki -p polaris-fable --lib --tests --offline -- \
  --skip integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp \
  --skip integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp \
  --skip provider::claude_login::tests::loopback_run_end_to_end_local \
  --skip provider::claude_login::tests::loopback_run_rejects_wrong_state \
  --skip provider::store::tests::local_route_tests::openai_provider_routes_via_local_router \
  --skip provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

分项结果：fable 105 passed/7 ignored；kernel 218 passed/2 ignored/6 filtered；kernel 集成测试 8 passed；wiki 7 passed。

## 新增行为测试与未运行内容

新增 13 项测试覆盖：独立 authority/member 数据库间签名清单传播、热缓存会话失效和持久删除、NodeId 冒用拒绝、签名篡改/未签名/账号混淆/回退拒绝、离线会话与过期高风险拒绝、重新登记不复活旧设备代际、通知送达所有既有设备与 ack、立即读写、单设备豁免、先批准再执行、一次性消费、自批/新设备互批拒绝、超时与已吊销批准者拒绝、旧断言兼容、配置类型与边界、HTTP 删除 query 参数、长轮询即时通知、跨账号新鲜断言。

保留并调整旧 grants 行为测试以验证策略 B 的立即读写；设备码进入的外部设备仍按原只读/禁执行默认值。

未运行/未声称覆盖：

- 前端策略 B 页面、通知 UI、批准交互由 P6b 实现；本次只提供接口和事件。
- 未做真实跨机器、真实 relay、真实 NAS 挂盘及墙钟 30 秒 SLA 联测；新增 HTTP 测试使用进程内 router/handler，不依赖沙箱 TCP。
- 原有 `collab::tunnel::tests::{loopback_echo_roundtrip,mdns_lan_only_roundtrip}` 在未设置 `POLARIS_NET_TEST=1` 时内部提前返回；计入 passed，不代表网络断言执行。
- fable/kernel 原有 9 个 ignored 未启用；六个具名 TCP 项仅记录原始失败与过滤重跑，不能视为通过。

没有其他已知的编译/测试阻塞。真实网络部署验收及 P6b 前端仍待对应阶段完成。

## 本次改动责任

`BackendStatus` 仅增加 Default，wiki 测试替身改用它。未改 R8 后端 supervisor/experts/provider 成本和已有前端工作。

协作域的会话、断言、登录准入、自动契约、有效权限、风险路由均拆到职责文件；原超长父文件只保留调用/导出/原有其他职责。设备登记/吊销事务在原目录文件加装配，迁移 SQL 独立。mesh 的连接与变更命令、卸载职责也拆出。

新的网络分支未使用 `#[allow]` 隐藏警告。迁移出来的原 Tauri camelCase 参数兼容注解继续保留，未改变前端参数契约。新增 Rust 文件无 `.unwrap()` / `.expect()`。

以下列表只包含本次编辑文件，不把工作区原有修改全部算成本次工作；`C`=`src-tauri/crates/polaris-collab/src/collab`，`H`=`src-tauri/src`。物理行数为报告生成时的值；已有超长文件按本次职责拆分，未整体重写。

| 文件 | 物理行数 | 单行最大字符 |
| --- | ---: | ---: |
| `src-tauri/crates/polaris-kernel/src/chat/backend/mod.rs` | 180 | 172 |
| `src-tauri/crates/polaris-wiki/src/wiki/compile/tests.rs` | 234 | 129 |
| `C/auth.rs` | 1436 | 101 |
| `C/auth/sessions.rs` | 105 | 124 |
| `C/authority.rs` | 1137 | 112 |
| `C/authority/assertions.rs` | 125 | 102 |
| `C/authority/sessions.rs` | 119 | 100 |
| `C/db.rs` | 746 | 113 |
| `C/grants.rs` | 575 | 98 |
| `C/grants/automatic.rs` | 42 | 89 |
| `C/grants/contract.rs` | 48 | 86 |
| `C/http.rs` | 4519 | 125 |
| `C/http/mesh_routes.rs` | 165 | 98 |
| `C/http/security_routes.rs` | 156 | 114 |
| `C/http/security_tests.rs` | 122 | 114 |
| `C/mesh.rs` | 393 | 113 |
| `H/apihub/auth.rs` | 217 | 100 |
| `H/apihub/desktop.rs` | 138 | 98 |
| `H/apihub/engines.rs` | 349 | 106 |
| `H/apihub/invoke.rs` | 294 | 116 |
| `H/desktop_commands.rs` | 357 | 79 |
| `H/fsmount.rs` | 2241 | 151 |
| `H/fsmount/unmount.rs` | 27 | 90 |
| `H/fsshare.rs` | 95 | 90 |
| `H/mesh.rs` | 872 | 122 |
| `H/mesh/connections.rs` | 433 | 105 |
| `H/mesh/login.rs` | 258 | 110 |
| `H/mesh/policy_commands.rs` | 154 | 111 |
| `H/mesh/security.rs` | 89 | 96 |
| `docs/feature-tree.md` | 493 | 369 |
| `docs/interconnect/p6a2-policy-b.md` | 103 | 182 |
| `C/interconnect/mod.rs` | 208 | 98 |
| `C/interconnect/defaults.json` | 16 | 32 |
| `C/interconnect/schema.json` | 80 | 81 |
| `C/interconnect/security.sql` | 47 | 102 |
| `C/interconnect/binding.rs` | 60 | 100 |
| `C/interconnect/devices.rs` | 73 | 102 |
| `C/interconnect/gate.rs` | 67 | 137 |
| `C/interconnect/notifications.rs` | 46 | 113 |
| `C/interconnect/revocations.rs` | 139 | 122 |
| `C/interconnect/risk.rs` | 128 | 123 |
| `C/interconnect/risk_inventory.rs` | 51 | 108 |
| `C/interconnect/sync.rs` | 54 | 111 |
| `C/interconnect/security_tests.rs` | 206 | 126 |

本验证报告本身亦低于 500 行/行 500 字符。
