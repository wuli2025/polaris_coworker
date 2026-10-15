# P6a 后端执行报告（部分实现，BLOCKED）

本报告不是验收结论。Claude 重跑验证、Pi 独立审查后决定模块是否完成。
未提交、未推送、未部署；保留已有及执行期间出现的其他任务改动。

## 实现范围

- 互联配置的 schema、随包默认、用户文件与环境变量解析；默认文件和 schema 经 `include_str!` 编进程序，无外部资源路径依赖。
- 首次接触权威必须匹配预置的完整 SHA-256 公钥指纹，不再创建新的 TOFU 信任；多指纹支持轮换。已配置指纹时，缓存公钥的离线验证也重新检查指纹。
- 密码登录、验证码登录、注册、mesh 登记及报到响应附带 `relays: string[]`。桌面登录、登记、报到与成员主机代密码登录自动消费该字段。
- relay 策略统一供主机、客户端和网关 Endpoint 使用。活跃 Endpoint 与期望配置不同时返回 `pending_reconnect`；客户端最后一个使用者释放后，下次建 Endpoint 采用新配置。
- 主机需要停止并重启监听；网关在旧连接全部结束后的下一次拨号重建 Endpoint。不会为了应用 relay 中断现有连接。
- 验证码、密码、已有断言三条桌面入口共用 `Register → Discover → Relay → Connect` 编排。失败阶段、错误和可重试标志可查询；登记成功后的重试不重新颁设备密钥。
- 提供高风险操作的代码清单，但**未实现策略 B 的执行闸**。原冷静期、现有通知和吊销语义未变。

## 阻塞与未完成内容

已向主控提出协议边界问题，执行期间未收到答复：是否允许增加设备身份绑定，并在权威不可达时拒绝同账号远程访问；或保留离线访问并明确接受吊销传播延迟。

依据：`authority::Claims` 只签账号身份，不含设备身份；`http::collab_login_assertion` 将客户端提供的 `deviceId` 传入登录。`mesh::enroll` 也接受账号断言中的 uid 配合自行填写的 NodeId。这不能安全证明调用方是“已有设备”。设备码链路的 possession proof 不能自动覆盖账号断言链路。

此外，云端 `mesh::revoke` 撤销设备密钥，本机独立签发的会话和离线验签并不立即向权威查询吊销状态。单纯把 cooldown 置零或新增一个确认 API，无法满足“已有设备批准”和“吊销立即失去访问”，会留下绕过路径。

因此以下内容未实现，也未声明通过：

- 新设备立即全权；旧设备通知及一键吊销的全网传播。
- 高风险请求持久化、向旧设备推送、批准后执行、超时拒绝、单设备豁免。
- 新设备窗口 N 小时的配置与迁移（默认 24 的要求尚待策略 B 接线）。
- 上述策略 B 行为测试；跨真实设备、真实权威与真实 relay 的端到端验证。
- 浏览器/手机的自动编排接入；新增密码编排命令只注册到本机 Tauri，未把 LocalConsole 准入暴露给远程 HTTP。

## 高风险操作清单及依据

公开代码入口：`collab::interconnect::risk_inventory::HIGH_RISK_OPERATIONS`，是待接线清单，不是授权判断。

| 操作 | 已识别入口 | 风险依据 |
| --- | --- | --- |
| 删除远程盘/挂载、远程删除 | `fs_unmount`、`fs_share_set/save`、`POST /api/fs/op/delete`、`mesh_device_remove`、`mesh_leave` | 撤去访问入口或删除数据 |
| 修改设备权限/角色 | `peer_grant_set`、`peer_trust`、`fs_share_save`、`POST /api/peer/grant`、`/api/peer/trust`、`/api/collab/admin/account_update` | 扩大盘读写、远程执行或管理权限 |
| 吊销其他设备 | `mesh_kick`、`mesh_device_remove`、`peer_revoke`、`POST /api/mesh/revoke`、`/api/peer/revoke`、`/api/collab/admin/device_revoke` | 新设备可能撤掉原有审批设备 |
| 改密码/绑定邮箱 | `POST /api/account/reset`、`/api/collab/email/reset`、`/api/collab/admin/user_reset_password`、`/api/collab/admin/account_update` | 改变认证或账号找回凭据 |
| 删除账号 | `POST /api/collab/admin/account_delete` | 级联撤销会话、设备、成员关系 |

后续实施还需审查 owner 远程执行对以上操作的间接调用，不能只保护 UI 按钮或列出的 HTTP 路由。

## 配置与切换

文件：`src-tauri/crates/polaris-collab/src/collab/interconnect/{schema,defaults}.json`。
默认用户文件：`~/Polaris/data/interconnect.json`；`POLARIS_INTERCONNECT_CONFIG` 可指定路径。显式指定的文件不存在或 JSON/类型错误会返回明确错误，不静默当成默认配置。
优先级：环境变量 > 用户 JSON > 随包默认。未知字段拒绝；URL 只允许 HTTP/HTTPS，拒绝凭据、查询、片段和空主机名。

| 配置 | 默认值 / 类型 | 环境变量 |
| --- | --- | --- |
| `authority.url` | `null` / 可空字符串；null 或空串表示未配置 | `POLARIS_ACCOUNT_AUTHORITY_URL` |
| `authority.publicKeyFingerprints` | `[]` / 完整 SHA-256 十六进制字符串数组，每项 64 字符 | `POLARIS_ACCOUNT_AUTHORITY_PUBLIC_KEY_FINGERPRINTS`，逗号分隔 |
| `relay.urls` | `[]` / URL 字符串数组；权威据此下发 relay | `POLARIS_RELAYS`，逗号分隔 |
| `relay.fallback` | `true` / 布尔值；列表为空时使用 n0，false 时禁用 relay | `POLARIS_RELAY_FALLBACK`，仅 true/false/1/0 |

部署后主控填写真实 URL 和指纹。本轮随包默认不含旧硬编码 IP，也不主动连接任何账号中心。
下例中的域名及 `DEPLOYED_SHA256` 必须由部署者替换；未提供有效指纹时首次连接被拒绝：

```bash
export POLARIS_ACCOUNT_AUTHORITY_URL=https://id.example.com
export POLARIS_ACCOUNT_AUTHORITY_PUBLIC_KEY_FINGERPRINTS="$DEPLOYED_SHA256"
export POLARIS_RELAYS=https://relay-a.example.com,https://relay-b.example.com
export POLARIS_RELAY_FALLBACK=false
```

用户 JSON 可只覆盖一部分，例如 `{"relay":{"fallback":false}}`。切回本地账号模式可显式设置 `POLARIS_ACCOUNT_AUTHORITY_URL=''`；这不会删除既有 mesh 登记或旧会话，退出设备网仍使用现有退网操作。

relay URL 来源优先级：`POLARIS_RELAYS` > 高级手工覆盖 > 权威下发 > 用户/随包配置。空的环境变量是明确清空；高级覆盖只在当前进程保留。权威不能覆盖客户端的 fallback 偏好。

## 迁移与依赖

- 没有新依赖、数据库表或 schema 迁移。仍使用已有 serde、ureq、sha2、Ed25519、iroh、tokio、parking_lot。
- 默认占位状态继续本地模式。曾经已有的 TOFU pin 在未配置 URL/指纹时保留兼容；任何新 pin 都要求预置指纹。旧部署若显式配置了权威 URL，须补充完整指纹。
- 旧 `kid` 仅用于展示，不能当完整指纹使用；`GET /api/account/pubkey` 新增 `fingerprint`，须经独立可信渠道核对。
- 配置了多个指纹仍保留显式 `repin` 流程，不静默替换已钉住的公钥。指纹不符时不写入新 pin。
- 旧权威响应不含 `relays` 时保持本地策略；字段存在但类型非法则报错。
- 断言 300 秒 TTL、设备密钥在权威只存哈希、本机成员资格和契约准入均未移除。没有新增远程路由，沿用现有登录频控；策略 B 新接口的频控尚未实现。
- 长文件只抽出本次职责：authority 信任配置、账号/mesh HTTP handler、Endpoint/relay、桌面登录/编排。其余内容仍是技术债，见文件清单。

## P6b 可用接口

- 本机 Tauri：`account_login_password({username,password,url?,inviteCode?,allowInsecureHttp?})`；已有 `account_login_code`、`mesh_join` 共用同一编排。
- 登录返回仍有 `enrolled/token/uid/nodeId`，增加 `interconnect: {stage,state,error,retryable}` 与 `relayConfig`。登记成功而连接失败时 `enrolled=true`，不能等同于互联 ready。
- `mesh_status`、`mesh_devices` 增加 `authorityConfigured`、`configurationError`、`interconnect`、`relayConfig`。未配置时应展示“未配置权威”，允许用户继续本地模式。
- `mesh_sync` 重试 Discover 起的后三步；Register 失败需重新登录。已有逐设备 `phase/errorCode/error` 继续可用。
- 桌面 hosting 存活时推送 `interconnect:progress`；也可轮询状态。无 hosting 时不能依赖该事件。
- `relayConfig` 含 `urls/fallback/source/state/pending/endpoints`；`state=pending_reconnect` 表示待生效，需断开全部相关连接后重建。
- HTTP：登录/注册、`/api/mesh/enroll`、`/api/mesh/announce` 增加 `relays: string[]`。高级 `/api/collab/tunnel/relays` 继续接受 `{relays:[{url:...}]}`。
- 通知/审批/超时/新设备窗口接口尚不可用，前端不能把策略 B 当成已经交付。

验证命令、真实结果及文件行数见 [验证与文件清单](p6a-validation.md)。
