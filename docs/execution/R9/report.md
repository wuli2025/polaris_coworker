# R9 第 2 次执行报告：修复已落地，联合必测 BLOCKED

执行日期：2026-10-15。依据 `docs/reviews/P6a-round1-triage.md` 与本轮 mesh.rs 范围例外。
本报告仅供 Claude 重跑、Pi 独立审查，最终由 Claude 决定模块是否完成。
保留进入任务时已有的 R9 修复；未提交、未推送，无新依赖；未触碰 `deploy/` 或并行 P6c 文件。

## 改动文件与行为

路径缩写：C=`src-tauri/crates/polaris-collab/src/collab`；K=`src-tauri/crates/polaris-kernel/src`；W=`src-tauri/crates/polaris-wiki/src`。

| 项目 | 文件 | 实际行为 |
| --- | --- | --- |
| P6a #5（保留） | `C/interconnect/revocations.rs` | 缓存验签或账号匹配失败时清空正文，本次高风险操作返回错误。保留版本/签发时间下限与已知吊销；条件更新避免清除并发刷新的不同正文。空正文不满足新鲜度检查。 |
| P6a #5（保留） | `C/interconnect/sync.rs`、`security_tests.rs` | 心跳调用共用的可注入账号刷新函数。测试覆盖篡改缓存→拒绝高风险且清空正文→拒绝旧版本/离线刷新→下一次心跳刷新路径恢复，并确认已知吊销保留。 |
| P6a #7 | `C/grants.rs`、`grants/contract.rs` | 删除 `in_cooldown`、无生产调用的 `trust_now`、文件/执行门禁的失效冷静期分支。保留显式权限、撤销和 `trust_fully` 恢复授权。 |
| P6a #7 | `C/grants/{gate_tests,tests}.rs` | 按门禁/契约职责移出原内联测试，删除仅测冷静期的 `trust_clears_cooldown` 和失效断言，保留立即全权、手动降权、重连、撤销等测试。迁出的测试用 Result 传播错误，无 unwrap()/expect()。 |
| P6a #7 | `C/http/peer_routes.rs` | 设备契约列表兼容字段固定 `inCooldown=false`、`cooldownUntil=0`。新增旧库未来时间戳回归，验证读写/执行、显式只读/ask、撤销、重新信任与响应字段。 |
| P6a #7（已批范围例外） | `src-tauri/src/mesh.rs` | 两个冷静期响应字段改为 false/0，删除说明中的冷静期引用；仅改写冷静期相关内容，872 行不增加、字符数减少。 |
| R8 #1（保留） | `K/chat/backend/mod.rs` | 选择 Default 仅单元测试可用；生产统一显式构造。状态/JSON 测试验证 Pi 初始 false、非 Pi null。 |
| R8 #1（保留） | `W/wiki/compile/tests.rs` | 跨 crate 测试替身显式填充 BackendStatus 字段，避免依赖生产 Default。 |
| P6a #3、R8 #3（保留并更新） | `docs/feature-tree.md` | relay 信任边界为 HTTPS 获取列表、iroh 端到端加密；仅篡改列表最坏拒绝服务。专家上限为原始 UTF-8 字节，包含尾部空白。更新冷静期清理入口、依赖、迁移及验证入口。 |
| R8 #3（保留） | `docs/execution/R9/expert-prompt.env.example` | 配置注释说明 UTF-8 字节、默认 32768、正 usize 校验、错误名、65536 切换与 unset 恢复。 |
| 执行记录 | `docs/execution/R9/report.md`、`validation.json`、`file-sizes.json` | 本轮命令、真实结果、文件大小与阻塞。 |

未修改驳回项对应的业务逻辑。`COOLDOWN_SECS` 在进入任务时已不存在；最终全 Rust 源码对 `\b(in_cooldown|COOLDOWN_SECS|trust_now)\b` 检索零匹配。
登录失败节流 `login_cooldown_check` 是另一安全机制，保持原行为。

## 为满足文件长度约定而拆分的 HTTP 文件

`C/http.rs` 原有 4519 行且包含必须修改的调用点，本轮拆至 323 行；不采用长文件例外或压行。
以下均为新增职责模块（路径均相对 `C/http/`）：

| 文件 | 职责 |
| --- | --- |
| `login_routes.rs`、`session_routes.rs` | 登录、会话、账号管理 |
| `connect_routes.rs` | 设备码、挑战、连接节流及原挑战测试 |
| `email_routes.rs`、`team_routes.rs` | 邮箱、注册与团队 |
| `telemetry_routes.rs`、`mirror_routes.rs` | 遥测、设备与账号镜像 |
| `project_routes.rs`、`task_routes.rs`、`task_chat_routes.rs` | 项目、任务操作/检查触发、任务对话 |
| `lead_routes.rs`、`merge_routes.rs` | 检查配置、主 Agent、合并 |
| `tunnel_routes.rs`、`git_routes.rs`、`websocket_routes.rs` | 隧道、网关、Git 代理与 WebSocket |
| `fs_read_routes.rs`、`fs_write_routes.rs` | 文件读取与写入 |
| `peer_routes.rs`、`router.rs`、`tests.rs` | 设备契约、路由装配、原 HTTP 单元测试 |

原公开接口仍由 `http.rs` 导出；处理函数通过 `pub(super)` 在 HTTP 模块内部连接，任务宏显式导出给检查处理模块。
逐段与 HEAD 对比并归一化可见性/import：除 `peer_routes` 和 `fs_read_routes` 外，上表路由模块函数正文原样迁移。
`fs_read_routes` 两处既有响应构造 unwrap 改为通过已有 `err_resp` 返回错误，以满足新增文件无 unwrap()/expect()；成功路径不变。
原 HTTP IP 测试也改为 Result 传播解析错误。

## 配置与迁移

无新配置、新依赖或数据库迁移。旧 `cooldown_until` 列与 Grant 序列化字段保留作存储兼容，不参与权限判定；HTTP/mesh 旧兼容字段固定 false/0。
`trust_now` 无调用方，删除它不会改变现有信任入口，当前入口继续调用 `trust_fully`。
缓存失效只清空正文，原行保留作为单调版本/时间下限。
安全心跳继续使用 `security.revocationPollSecs` 默认 30 秒，新鲜度 `revocationMaxAgeSecs` 默认 600 秒，沿用原类型/范围校验。
切换示例：`POLARIS_INTERCONNECT_CONFIG=/absolute/path/interconnect.json`，内容 `{"security":{"revocationPollSecs":60,"revocationMaxAgeSecs":600}}`；其余字段沿用默认。
专家配置切换示例：`POLARIS_EXPERT_PROMPT_MAX_BYTES=65536 ./polaris-server`；unset 恢复默认 32768 UTF-8 字节，必须为正 usize。

## 实际验证

全部 Cargo 命令在 `src-tauri/` 执行，使用当前默认沙箱权限，无跳过失败测试参数。
最终代码验证结果由 `/tmp/r9_verify.py` 顺序执行并记录，见同目录 `validation.json`；日志在 `/tmp/r9-2-*.log`。

| 命令 | 真实结果 | 日志 |
| --- | --- | --- |
| `cargo check -p polaris-app --offline` | exit 0；0 警告 | `/tmp/r9-2-desktop.log` |
| `cargo check -p polaris-cli --bin polaris-server --offline` | exit 0；0 警告 | `/tmp/r9-2-server.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | exit 0；0 警告 | `/tmp/r9-2-server-net.log` |
| `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | exit 0；0 警告；test result: ok. 138 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 26.16s | `/tmp/r9-2-collab-tests.log` |
| `cargo test -p polaris-kernel -p polaris-wiki --lib --tests --offline` | exit 101；0 警告；test result: FAILED. 219 passed; 6 failed; 2 ignored; 0 measured; 0 filtered out; finished in 30.26s | `/tmp/r9-2-kernel-wiki-tests.log` |
| `cargo test -p polaris-wiki --lib --tests --offline` | exit 0；0 警告；test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s | `/tmp/r9-2-wiki-tests.log` |
| `cargo test -p polaris-kernel --test backend_config --test catalog_protocol --test credential_lock_interop --test provider_switch --offline` | exit 0；0 警告；四个二进制共 8 passed | `/tmp/r9-2-kernel-integration.log` |

`git diff --check` 通过。Python 逐文件扫描确认：本次新增/编辑文件均 ≤500 行、每行 ≤500 字符，唯 mesh.rs 使用已批既有长文件例外，行数不增加、字符数减少。
新增 Rust 行及全部新增 Rust 文件无 unwrap()/expect()。`rg` 确认旧冷静期函数/常量零引用，生产 BackendStatus 构造显式；HTTP 分段对比结果见上文。

中间执行情况（不算最终通过）：最初 `python /tmp/r9_cleanup.py` 因无 python 命令失败，改用 python3；两次误在仓库根运行 cargo check 报无 Cargo.toml，随后切换 src-tauri。
初次模块拆分后的 server check 报任务宏/生成函数不可见（7 个错误、1 个警告）；补内部可见性和宏导出并去掉冗余导入后，三种最终 check 均零警告。
协作测试首轮 138 通过；迁移测试的错误传播调整后再次执行完整命令，最终仍 138 通过，未用首轮结果代替最终验证。

## 未完成与阻塞

联合必测 `cargo test -p polaris-kernel -p polaris-wiki --lib --tests --offline` exit 101，kernel 219 passed、6 failed、2 ignored，Cargo 随即中止后续目标。
失败项：

1. `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`
2. `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`
3. `provider::claude_login::tests::loopback_run_end_to_end_local`
4. `provider::claude_login::tests::loopback_run_rejects_wrong_state`
5. `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`
6. `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`

前四项明确报 TCP bind `PermissionDenied / Operation not permitted`；后两项报无法绑定 8765–8789 本地端口。
补跑 wiki 7 通过及 kernel 四个集成测试二进制共 8 通过，不能替代联合必测通过。
当前环境禁止提权；需要 Claude 在允许本地监听的环境重跑联合命令。本轮没有修改这些网络测试、忽略失败或改写权限配置。
P6a #7 原范围阻塞已解决；本次 BLOCKED 仅指联合必测未达到要求，不声称模块验收完成。

未运行：真实跨主机/relay 联测、墙钟 30 秒后台循环、前端测试、原 ignored 测试。
缓存恢复测试注入权威响应并调用心跳共用刷新路径，不覆盖 ureq 真实传输或定时调度。
collab 既有两项隧道测试在未设置 `POLARIS_NET_TEST=1` 时提前返回，138 passed 不表示已执行真实网络断言。
