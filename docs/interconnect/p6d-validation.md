# P6d 验证记录与责任文件

这是执行记录，不是 Claude/Pi 的验收结论。实现、API、配置、迁移和未验证边界见 [P6d HTTP 报告](p6d-http.md)。无 commit/push。

## 实际命令与最终结果

Cargo 命令目录为 `src-tauri/`，前端命令目录为仓库根。下列结果来自最后一次执行，不以静态阅读替代运行。

| 命令 | 最终结果 | 原始日志 |
| --- | --- | --- |
| `cargo check -p polaris-app --offline` | 退出 0，0 警告 | `/tmp/p6d-desktop.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0，0 警告 | `/tmp/p6d-server-net.log` |
| `cargo check -p polaris-cli --bin polaris-server --offline` | 退出 0，0 警告 | `/tmp/p6d-server.log` |
| `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | 退出 0；154 passed，0 failed，0 ignored；0 编译警告 | `/tmp/p6d-collab-tests.log` |
| `npx vue-tsc --noEmit` | 退出 0，无输出 | `/tmp/p6d-tsc.log` |
| `npx vitest run` | 退出 0；18 文件、84 测试通过 | `/tmp/p6d-vitest.log` |
| `npx vite build` | 退出 0；仍有大于 500 kB chunk 提示 | `/tmp/p6d-vite.log` |
| `git diff --check` | 退出 0，无输出 | 终端 |
| `python3 /tmp/p6d-files.py` | 责任文件行数/字符、Tauri 净增、View 未改、新 Rust 无 `.unwrap()` / `.expect()` 扫描通过 | `/tmp/p6d-file-table.md` |

其他实际验证：

- `cargo check -p polaris-collab --features collab-host,collab-net --offline`：最初误在根目录执行，退出 101（根目录无 Cargo.toml）；改到 `src-tauri/` 后退出 0。
- 定向 Rust `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib http::browser_tests --offline`：首轮 4 通过/3 失败。一个测试错误假设 owner UID 必须为空，忽略既有 owner 登录的旧库补写；该断言失败使共享测试锁中毒，连带两项失败。改成验证预置所有者保持不变后，7 项全部通过；随后又增加两项 HTTP 测试，全部包含在最终全量 154 项中。
- `npx vitest run src/features/interconnect`：首轮 17 通过/2 失败（既有 View 测试 mock 缺少新增的后端探测导出）；更新 mock 后，3 文件/19 项全部通过。
- 本机 socket 探测 `python3 /tmp/p6d-check-socket.py` 返回 `PermissionError: [Errno 1] Operation not permitted`。没有因此跳过规定的 Cargo/npm 命令；实际网关和 iroh 联测未运行。

## 新增行为覆盖

Rust 新增 12 项（此前 142 项）：

- HTTP 密码登录产生独立绑定会话，状态只对当前合法 token 开放，不改变已有主机所有者；匿名不能取得编排记录，吊销后禁止重试。
- Connect 失败保持 enrolled；重试保留设备 ID、meshKey 哈希和登记时间，不增加目录记录。
- 缺账号、错密码、停用账号返回相同 401/错误模型；既有 5 次密码错误冷却仍生效。
- HTTP 登录/查询/重试使用 P6b 登录/读/写预算，改变 Bearer 无法绕过登录来源预算。
- 浏览器新设备通知旧设备；真实 `/api/peer/revoke` 在窗口期内等待批准、不提前 mutation，拒绝自批，旧设备批准后执行。
- 审批从当前同账号设备目录映射名称/平台；已吊销和目录项缺失为 null。
- 客户端不能指定旧 NodeId、console 模式或权威 URL；非法标签类型拒绝；登出级联删除持久编排状态。
- browser 配置的旧默认、类型、范围、未知字段和 URL 校验。
- 注入传输的连接测试验证目标 URL、设备绑定断言、目标 token、caps、故障保留凭据、恢复、过期会话重新认证，且不谎称 OS 挂盘。
- 单会话重试锁与不同会话隔离；网关保留签名设备断言但不透传客户端代理信任头。

Vitest 新增 7 项（此前 77 项）：原生命令/HTTP 选择、HTTP 登录接管/查询/重试、错误与 429 不降级、预览不接管真实账号、安全 ack/decide 的会话鉴权、审批名称/平台及转义、未知/吊销/旧响应占位。

未运行：真实权威、真实 relay、跨机器/NAS/手机端到端、原生盘符挂载验证。已有网络测试在未设置 `POLARIS_NET_TEST=1` 时内部提前返回；154 passed 不等于这些网络测试实际执行。浏览器采用 HTTP 文件接口，`mounted:false`；手机代码按任务未修改。

## 责任文件清单

`src/tauri.ts` 依任务允许保留既有超长：净增 **5** 行。`InterconnectView.vue` **0** 改动。其余本轮责任文件均不超过 500 物理行，每行不超过 500 字符。gateway 旧文件从 523 行拆到 479 行，请求头/query 职责进入单独文件。未改依赖清单。

保留执行期间主控写入的 `docs/feature-tree.md` R10b 行，以及其他执行/审查材料；下表只列本任务责任文件。入口职责见 [实现报告](p6d-http.md)。

<!-- FILE_TABLE -->
| 文件 | 物理行 | 单行最大字符 |
| --- | ---: | ---: |
| `docs/feature-tree.md` | 500 | 404 |
| `docs/interconnect/p6b-frontend.md` | 117 | 207 |
| `docs/interconnect/p6d-http.md` | 98 | 263 |
| `docs/interconnect/p6d-validation.md` | 88 | 274 |
| `src-tauri/crates/polaris-collab/src/collab/gateway.rs` | 479 | 109 |
| `src-tauri/crates/polaris-collab/src/collab/gateway/request.rs` | 51 | 122 |
| `src-tauri/crates/polaris-collab/src/collab/http.rs` | 327 | 97 |
| `src-tauri/crates/polaris-collab/src/collab/http/browser_routes.rs` | 59 | 124 |
| `src-tauri/crates/polaris-collab/src/collab/http/browser_tests.rs` | 217 | 142 |
| `src-tauri/crates/polaris-collab/src/collab/http/rate_limits.rs` | 72 | 155 |
| `src-tauri/crates/polaris-collab/src/collab/http/router.rs` | 221 | 92 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/browser/authority.rs` | 100 | 117 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/browser/connections.rs` | 83 | 144 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/browser/connections/tests.rs` | 78 | 119 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/browser/mod.rs` | 224 | 128 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/defaults.json` | 30 | 36 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/gate.rs` | 68 | 137 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/mod.rs` | 231 | 98 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/risk.rs` | 131 | 123 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/schema.json` | 148 | 81 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/security.sql` | 51 | 102 |
| `src-tauri/crates/polaris-collab/src/collab/mesh.rs` | 393 | 113 |
| `src/features/interconnect/ApprovalPanel.vue` | 24 | 189 |
| `src/features/interconnect/DeviceList.vue` | 29 | 125 |
| `src/features/interconnect/browserTransport.test.ts` | 93 | 133 |
| `src/features/interconnect/browserTransport.ts` | 43 | 115 |
| `src/features/interconnect/contracts.ts` | 20 | 180 |
| `src/features/interconnect/copy.ts` | 28 | 108 |
| `src/features/interconnect/defaultView.test.ts` | 38 | 164 |
| `src/features/interconnect/useInterconnect.ts` | 127 | 111 |
| `src/features/interconnect/useLogin.ts` | 73 | 127 |
| `src/tauri.ts` | 2644 | 225 |
