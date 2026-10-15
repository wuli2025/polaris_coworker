# P7c：无头 server 账号密码入网 + D4 成网演练补齐

## 背景（Claude 已核实）

用户硬性需求：**"登录以后账号密码能够打通，然后互相连接……直接账号密码一登录就直接互联了。"**

D4 中 `login / relay / access / offline` 仍输出 blocked。根因不只在脚本，产品也有缺口：

1. 桌面端 `src-tauri/src/mesh/login.rs::account_login_password` 已实现"账号密码 → 断言 → 入网 → 发现 → 下发 relay → 连接"的整条编排（`orchestration::complete_login`）。
   但它只在 `desktop_commands.rs` 注册为 Tauri 命令。**server 形态（polaris-server，collab-net）没有任何入口**能让本机作为主机用账号密码入网。
2. `mesh::spawn_if_enrolled()` 只在 `lib.rs` 的桌面启动流程中调用，`server.rs` 没有调用。server 重启后不会自动重连设备网。
3. `/api/interconnect/login` 是浏览器会话设备（`web_*`），不是 iroh 主机 NodeId，不能替代第 1 点。

## 目标

A. 产品：无头 server 的 owner 用**账号 + 密码**一步入网，效果与桌面 `account_login_password` 一致。重启后自动重连，不需要 token、设备码或手工 tunnel/start。

B. 演练：`drills/d4.mjs` 的 `login / relay / access / offline` 改为真实步骤，使用 A 的产品入口。不得伪造，不得绕过 TLS 校验。

## 要求

### A 产品

1. 先查清前端（`src/` 的 InterconnectView 等）在 server/web 形态下如何调用 `account_login_password`（例如 `/api/invoke` 的分发表）。优先把**同一个**命令接入 server 分发，让现有 Web UI 不改即可用。
   不要复制编排逻辑，必须复用 `mesh::account_login_password` / `orchestration::complete_login`。如果 mesh 模块某些部分依赖 Tauri，就做最小的条件编译拆分。
2. 鉴权：只允许本机已登录的 owner（`role_rank >= 3`），未登录返回 401/403。走现有 login 频控桶。`allowInsecureHttp` 在 server 入口固定为 false，除非现有策略已有显式开关。
3. 密码不得进入日志、审计、错误文本或响应。错误信息采用固定文案。
4. `server.rs` 在 collab-net 下启动时调用 `mesh::spawn_if_enrolled()`。确认入网后主机隧道会启动（不依赖 `POLARIS_TUNNEL_AUTOSTART`）；若依赖，就在入网成功或已入网时启动，并保持幂等。
5. 同时检查：`mesh_join`、`account_login_code`、`account_send_code`、设备列表/吊销等 mesh 命令在 server 形态下是否也缺入口。缺失的列入报告；只有 Web UI 实际调用的才补，补法与第 1 点相同。
6. 测试：未登录被拒、非 owner 被拒、密码不出现在任何输出中、分发表包含该命令、server 启动路径调用 spawn_if_enrolled（可测试的函数级断言即可）。三种 cargo check 均需 0 警告。

### B 演练（`drills/lib/d4-topology.mjs`、`drills/d4.mjs`，必要时新建 `drills/lib/d4-mesh.mjs`）

- **login**：member-a 和 member-b 先用本机 owner 会话登录（拓扑已设 `POLARIS_REQUIRE_LOGIN=1`，按现有方式创建本机 owner；若需种子账号，参照 `seed_authority_account` 示例）。
  然后调用 A 的入口，以 `drill-user` 和 `h.password` 入网，authority URL 为 `https://authority.drill`。断言返回成功，且 authority 的设备名册（`/api/mesh/devices` 或等价接口）同时出现两个主机 NodeId。
- **relay**：断言两台成员收到并应用的 relay 为 `https://relay.drill`，且连接实际经过该 relay。使用产品已有状态接口，例如 tunnel status 的 home relay 或连接路径。
  拓扑已关闭 QUIC 地址发现，跨容器直连可能仍可行。若无法证明"实际经 relay"，就如实只断言"已应用"，并在 evidence 中写明。
- **access**：member-a 通过产品的跨主机文件路径（mesh 挂盘或 peer HTTP-over-iroh，以产品实际实现为准）在 member-b 的 `share` 中写入随机内容的文件，再读回。同时从宿主机目录 `h.data['member-b']/share` 核对内容。记录登录完成到首次读写成功的耗时。
- **offline**：`compose stop authority` 后，已建立的 a→b 访问在一定时间内仍可读。属于高风险的操作（参照 risk 清单）应 fail-closed。完成后重新 `start` authority，避免影响后续步骤。
- `revocations` 保持 blocked。若 `revoke` 能改为吊销**主机** NodeId 并在 member 侧观察到传播，就补一项 `host-revoke`；否则不做。
- 拓扑中成员容器需要能启动 iroh 隧道。如需 `POLARIS_TUNNEL_AUTOSTART` 或其他环境变量，在 compose 中按需设置，并说明理由。
- 继续遵守：所有秘密加入 `h.secretValues`；不禁用 TLS 校验；不新增依赖；镜像 tag 唯一；cleanup 完整。

## 约束

- 单文件 ≤500 行、单行 ≤500 字符；多用配置和变量，不硬编码；不新增依赖；不改 `mobile/`。
- 沙箱内无法运行 Docker、无法监听端口、无网络：演练脚本只做 `node --check` 和可行的单元测试，由 Claude 在沙箱外实跑。
  Rust 测试中涉及 HTTP 监听的部分如果在沙箱内跑不了，就在报告中列出，由 Claude 重跑。
- 不 commit、不 push。
- 报告写到 `docs/tasks/P7c-execution-report.md`，内容包括：改动文件、入口契约、测试列表、沙箱内实际跑过的命令与结果、需要 Claude 在沙箱外重跑的命令、仍然 blocked 的项及原因。
