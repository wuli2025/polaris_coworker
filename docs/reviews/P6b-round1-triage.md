# P6b 第 1 轮 Pi 审查处置（Claude）

原始报告：`docs/reviews/P6b-round1/`（含测试）。

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 | 外部 CSS 行数 | **无问题** | Pi 自行判 PASS。 |
| 2 中 | slow-timer 在组件卸载后写 error | **驳回** | 回调内已有 `if (!disposed)` 检查；JS 单线程，`dispose()` 与回调不会交错执行，Pi 自己也承认"安全跳过"。 |
| 3 中 | 未见 `mesh_leave` 实现 | **有证据驳回** | `src-tauri/src/mesh/policy_commands.rs:87` 已实现（P6a），并在 `desktop_commands.rs` 注册。 |
| 4 低 | `tick()` 无超时兜底 | **驳回** | 后端调用各自有请求超时（agentd/collab 配置），`busy` 期间跳过是正确的去重。 |
| 5 低 | 过期待批准项不主动清理 | **采纳** | 本地按 `expiresAt` 过滤过期项，避免离线时 UI 残留。 |
| 6 信息 | 登录失败是否泄露账号存在 | **核查后采纳一项既有漏洞（中-高）** | 密码登录统一返回"用户名或密码错误"（`collab/auth.rs:455,463,750,758`），无泄露；`auth.rs` 中其余"账号不存在"均为按内部 user_id 的 owner/管理员操作，非匿名可达。**但** `collab/http/email_routes.rs:72-100` 的发送验证码接口（匿名可达）：`signup` 对已注册邮箱返回"该邮箱已注册过账号"，`reset` 对未注册邮箱返回"该邮箱没有绑定任何账号"——可用于**账号枚举**。该问题重构前即存在（`b6d9e38:http.rs:1586`），但权威将部署在公网，必须修复：对外响应统一（"如该邮箱可用，验证码已发送"），差异只体现在邮件内容（已注册邮箱申请注册→收到"你已有账号"提醒；未注册邮箱找回/验证码登录→不发送）；保持现有频控；测试：已注册/未注册邮箱在 signup/reset/login 三种用途下 HTTP 响应完全一致。 |

## 第 2 轮（复审）
- 首次 `P6b-round2/`：MiniMax 额度用尽（429）BLOCKED。用户决定改用 openai-codex；`gpt-5.5`、`gpt-5.2-codex` 均因 ChatGPT 账号不支持而失败，`gpt-6-astra` 探测 LIVE_OK。**审查者与执行者同属 OpenAI 家族，独立性较弱。**
- `P6b-round2b/`（gpt-6-astra）结论 **INCOMPLETE**：材料缺频控/IP 提取/哈希实现与完整文件。处置：
| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 中 | 后台投递 `spawn_blocking` 无并发上限 | **采纳** | 公网权威可被大量不同邮箱请求占满阻塞线程池；加有界投递队列/并发许可与投递期限，队列满时对外响应不依赖账号状态。 |
| 2 中 | 验证码消费非原子 | **采纳（安全）** | `collab/mail.rs` 读（SELECT）与改/删（UPDATE/DELETE）分离且忽略错误：并发请求可同时读到未超限的次数，放大暴力尝试、或同一码多次成功。改为单事务（SQLite `BEGIN IMMEDIATE`）条件更新并检查受影响行数。该逻辑重构前即存在。 |
| 3 低 | 后台失败日志无阶段 | **采纳** | 记录脱敏阶段标识与关联 ID。 |
| C 高（主控核查发现） | 频控 IP 可伪造 | **采纳（安全）** | `collab/http/login_routes.rs:135-150` 的 `client_ip` 无条件优先信任客户端提供的 `X-Real-IP`，其次最左 `X-Forwarded-For`：任意客户端每次换一个伪造 IP 即绕过所有按 IP 的频控（密码登录、验证码、策略 B 接口）。重构前即存在（`b6d9e38:http.rs:529`）。新加坡部署中 Caddy 以 `header_up X-Real-IP {remote_host}` 覆盖（`deploy/singapore/templates/Caddyfile:26`）故该形态安全，但桌面托管、NAS、局域网直连等形态均可伪造。修：默认使用 TCP 对端地址；仅当对端属于配置的可信代理列表时才采信转发头（XFF 取最右侧不可信地址）；部署包配置可信代理为 Caddy 所在网络。 |
- 结论：P6b 达 2 轮上限，独立审查状态 **INCOMPLETE（非 PASS）**；以上采纳项转 R13 修复，修复后由主控自检，并纳入收尾的跨模块安全审查。
