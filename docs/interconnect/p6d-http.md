# P6d：浏览器登录编排与审批设备名称（执行报告）

本报告供 Claude 重跑验证、Pi 独立审查；不代表最终验收决定。未 commit、push、部署，未新增依赖，未修改 `mobile/`。

## 入口与职责

- `polaris-collab/src/collab/http/browser_routes.rs`：密码登录、状态、重试及会话设备的安全收件箱/确认/审批/吊销 HTTP 适配。由现有 `http/router.rs` 注册，桌面 hosting 与 server 壳共用。
- `interconnect/browser/mod.rs`：持久状态、独立会话设备、远程成员准入与策略 B 适配；与桌面 `src-tauri/src/mesh/orchestration.rs` **调用同一个** `interconnect::login::run`，共享 Register → Discover → Relay → Connect 次序及 `Progress` 模型。
- `browser/authority.rs`：本机权威直接调用既有账号/mesh 服务；委托主机先校验预置指纹并钉住权威，再调用其公开 API。登录使用既有 `auth::authority_login`，保留账号密码失败冷却。
- `browser/connections.rs`：通过已部署的 `/h/{NodeId}` 网关登录对端并读取 `/api/fs/caps`，确认 HTTP 文件访问可用。它复用网关已有 iroh 隧道，不在服务器上替浏览器映射系统盘符。
- `gateway/request.rs`：抽出既有请求头和 query 组装，增加透传 `x-polaris-device-assertion`，让目标主机能够验证当前发起设备。仍不信任客户端代理 IP 头。
- `src/features/interconnect/browserTransport.ts`：Tauri 继续调用原命令；非 Tauri 的密码登录/状态/重试/安全操作改用上述 HTTP 路径。纯前端预览返回同形状态并明确标记演示，**不生成真实 token、不接管账号**。HTTP 错误不降级成预览成功。
- `useInterconnect.ts` / `useLogin.ts` / `DeviceList.vue`：接管 HTTP 会话、轮询、显示会话设备及 HTTP 连接状态；仍使用既有协作 store 的 `me` 确认身份。`src/tauri.ts` 只导出既有后端探测结果，净增 5 行；`InterconnectView.vue` 未改。

## HTTP 契约

所有路径均在当前服务壳同源下；没有新增 CORS 或调用者自选权威 URL。请求/响应为 JSON。该入口面向配置了权威的账号，不把未配置权威的本地账号模式伪装成已互联。

| 方法与路径 | 输入/凭据 | 输出与语义 |
| --- | --- | --- |
| `POST /api/interconnect/login` | `{username,password,deviceName?,platform?}`；不需要旧 Bearer | 成功返回本机会话 `token`、`uid`、`nodeId`、`enrolled`、`interconnect`、`relayConfig`、`peers`、`devices`、`storageMode:"http"` |
| `GET /api/interconnect/status` | `Authorization: Bearer <上述 token>` | 当前会话的持久编排快照；验证设备仍有效并报到；另返回短期 `deviceAssertion`；不重建连接 |
| `POST /api/interconnect/retry` | 同上，空 JSON 对象 | 从 Discover 开始，保留同一 NodeId、deviceId、meshKey、登记时间；不会重发密码或重新登记 |
| `GET /api/interconnect/security/inbox` | 同上 | 当前会话设备的 `notifications` 和 `approvals` |
| `POST /api/interconnect/security/ack` | 同上，`{deviceId}` | 确认通知 |
| `POST /api/interconnect/security/decide` | 同上，`{requestId,approve:boolean}` | 沿用“接入前已有设备才能批准、不能自批、超时拒绝” |
| `POST /api/interconnect/devices/revoke` | 同上，`{nodeId}` | 通过策略 B 后吊销同账号目标设备；底层沿用 mesh revoke |
| `POST /api/collab/logout` | 同上 | 继续使用既有登出；本机会话及持久编排记录级联删除 |

`interconnect` 仍为 `{stage,state,error,retryable}`，阶段取 `register/discover/relay/connect`。登记失败返回 401 和统一 `interconnect_login_failed`；未知账号、错误密码、停用、准入失败、密码冷却均不返回可区分账号存在性的错误文本。登记之后的连接失败仍返回 200、`enrolled:true` 和具体失败阶段，不能视为 ready。已登记会话的瞬时连接故障用 retry 恢复；被吊销/失效会话不得 retry。

状态/重试只接受该入口产生的有效绑定会话；既有任意 session、机器管理员口令和客户端伪造的 NodeId 都不能借此取得主机的 meshKey 或状态。并发重试按会话单飞，重复返回 409 `interconnect_retry_busy`；不同会话互不串状态。所有新路径响应设置 `Cache-Control: no-store`。

### 浏览器/手机设备身份与策略 B

每次新的密码登录由服务端生成随机 `web_…` NodeId，并在权威 mesh 目录取得独立 `dev_…` deviceId、登记序号与密钥。客户端只能填写展示名/平台，不能指定旧设备、`console`、权威地址或本机所有者；未知字段拒绝。登记前只走现有远程 `ExistingOnly` / 已认定主人的 `SelfOwned` 成员准入，绝不调用 `LocalConsole`，不会认领一个未绑定的本地同名账号。

这些设备会触发与普通设备相同的旧设备通知、窗口期判断和审批队列。会话必须绑定到这次生成的设备；高风险 HTTP 操作经 `gate::session_proven` 进入浏览器的在线权威审批适配，签名许可绑定账号、会话设备、操作摘要和随机 nonce，一次性消费。审批超时、拒绝、吊销或权威不可达均不放行。新增的浏览器会话不会继承承载服务器作为旧设备的审批豁免；原策略的首设备/单设备例外继续保留。

`web_…` 是会话设备而非可监听的 iroh 主机，因此从自动拨号的 `mesh::peers` 清单排除；仍保留在完整设备台账、通知、审批及吊销清单中。status 请求报到保持会话设备在线；登出/过期后停止报到，设备历史不自动删除或解除策略 B 约束。再次登录创建新的登记，旧审批设备丢失时仍需已有设备/既有恢复流程处理，不能靠登出制造豁免。

meshKey 仅存服务端协作 DB，不返回浏览器。部署切换权威地址后旧会话拒绝继续使用原设备密钥，需重新登录。会话 token 沿用既有 30 天有效期及吊销检查；审批仍以 mesh 设备目录为依据。

### 连接与手机后续接入

Connect 对 Discover 返回的在线主机逐个通过网关换取**设备绑定**的目标主机会话，再验证文件读能力；网关未配置、未挂牌、不可达、拒绝准入或无文件读权限时返回失败。离线设备显示 offline；无在线对端时可以 ready，但这不构成真实网络已连通的证据。HTTP 连接不标注“直连”，也不将 `mounted` 改成 true。

`peers` 中连接描述含 `nodeId/name/os/state/error/url/token/caps/storageMode`；敏感的对端 token 只随已认证的当前会话响应返回，界面不渲染凭据。对端连接凭据在重试时复用；目标返回 401/403 时用同一设备的新鲜断言重新取得对端会话，不重新登记设备。

手机可以采用同一会话设备协议（`platform:"android"` / `"ios"`），把 `peers[].url` 作为目标 `/api/fs/*` 的基址，发送对应 `peers[].token`。高风险请求还应从 status 获取新鲜 `deviceAssertion`，通过 `x-polaris-device-assertion` 传给目标；网关只透传，最终验证与角色权限仍由目标主机负责。非同源网关的浏览器访问还取决于部署已有的同源反代/CORS；本任务不修改网关的 CORS 策略。

本轮只切换默认**密码登录**入口；现有邮箱验证码组件与历史兼容登录协议没有改写。没有修改移动端或新增原生手机隧道/挂盘实现。旧协议与未绑定会话的历史策略仍由 P6a2 负责，本轮没有把它们迁移成新会话设备。

## 频控、配置与切换

新路由全部进入 P6b 的持久频控中间件，且位于认证、网络 I/O 和 mutation 之前：login 使用登录预算；status/inbox 使用读预算；retry/ack/decide/revoke 使用写预算。登录桶按真实 socket 来源限额，不可通过更换 Bearer 绕过；每路径另受来源/全局上限限制。429 保持 `interconnect_rate_limited`；未注入 ConnectInfo 的壳共享 unknown-peer 来源桶。

配置继续用 `POLARIS_INTERCONNECT_CONFIG` JSON；旧文件自动合并随包默认值，无新环境变量。新增 `browser` 节点及 JSON schema：

| 字段 | 默认值 | 类型/校验 |
| --- | --- | --- |
| `deviceName` | `Browser session` | 1–40 个可打印字符，非空；请求可覆盖 |
| `platform` | `web` | 同上；请求可覆盖 |
| `gatewayUrl` | `null` | 可空 HTTP/HTTPS URL，拒绝凭据、query、fragment；null 时使用已配置 authority URL |
| `requestTimeoutSecs` | `10` | 整数 1–60，单次上游 HTTP 超时；禁止重定向 |
| `maxPeers` | `32` | 整数 1–128；超出上限返回 Connect 失败，不静默丢掉设备 |

配置示例（域名/指纹由部署者填写）：

```json
{
  "authority": {"url":"https://id.example.com","publicKeyFingerprints":["部署公钥的完整64位SHA256十六进制"]},
  "browser": {"deviceName":"Web session","platform":"web","gatewayUrl":"https://gw.example.com","requestTimeoutSecs":15,"maxPeers":64},
  "rateLimit": {"windowSecs":60,"readRequests":120,"writeRequests":30,"loginRequests":20,"globalMultiplier":20}
}
```

`export POLARIS_INTERCONNECT_CONFIG=/absolute/path/interconnect.json`。删除 `browser` 节点恢复随包默认，或设 `gatewayUrl:null` 改回权威域名的网关。URL 来自受信任服务端配置，HTTP 参数无法选择网关地址；生产域名应使用 HTTPS。未知字段、错误类型、零/负数/小数和超范围数值均拒绝；URL 仍使用 P6a 校验。

## 审批设备名称

`interconnect::risk::pending` 在只读查询中按**同账号、当前 deviceId**关联 `mesh_device_ids` 与 `mesh_nodes`，输出 `actorName` / `actorPlatform`。覆盖 security inbox/watch、announce 和桌面消费的公共审批数据。已吊销或目录项缺失返回 null，保留原 actor ID，不借用其他账号或再登记设备的名称。

`ApprovalPanel.vue` 展示设备名、平台和稳定 ID；空值/旧后端响应显示“未知或已吊销设备”“平台未知”。展示由 Vue 转义，不把设备名当 HTML。元数据不改变审批资格；未知设备请求仍不能通过 `decide`。

## 迁移与依赖

`interconnect/security.sql` 新增 `browser_interconnect(token,record)`，外键引用 `bound_sessions`，随既有幂等迁移建立；不改旧 session、通知或审批结构。记录包含设备凭据、进度、发现结果和对端连接凭据，密码不入库。登出/撤销本机会话会级联删除记录。旧配置补 browser 默认值；前端继续使用既有账号 session 存储，无新增 localStorage 密钥。

无新增 Cargo/npm 依赖。公共依赖为既有 auth/authority/mesh/binding/risk、SQLite、serde、tokio、ureq、网关与 Vue/现有桥。无 collab-net 的壳仍能通过外部网关执行 HTTP Connect；本地 relay 状态明确为 unavailable，不声称本构建提供 iroh。

## 实际验证与未验证部分

完整命令、结果和文件清单见 [P6d 验证记录](p6d-validation.md)。目前要求的三种 Cargo check、collab 测试、Vue 类型检查、全量 Vitest 和 Vite build 均已执行。

未执行真实多机、NAS、手机、真实权威/relay/网关的端到端联测；当前执行沙箱禁止 socket 创建（本机 loopback 探测返回 `PermissionError: Operation not permitted`），且未提供真实部署配置。连接单元测试注入传输并检查绑定断言、对端 token、caps、故障和恢复；它不是实际 iroh 成功证据。现有 `POLARIS_NET_TEST` 环境开关控制的网络测试本轮未开启，Rust 的 passed 数不能替代该验证。

未接入浏览器原生系统盘符、历史邮箱登录协议或移动端 UI；前两者的具体边界见上文，移动端按任务明确排除。最终是否满足主控对“浏览器等价”的验收，应按上述 HTTP 文件访问语义与真实部署复测判断，不能由本执行报告推断。
