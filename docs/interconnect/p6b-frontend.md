# P6b 执行报告：互联默认界面与 HTTP 频控

本报告供 Claude 重跑、Pi 独立审查，不是最终验收结论。没有 commit/push，没有新增依赖；保留工作区其他任务改动。

## 入口与行为

- `src/features/interconnect/InterconnectView.vue`：默认未登录仅账号密码与已有邮箱验证码入口；桌面密码登录调用 `account_login_password`，启动本机 hosting 后接管返回的本机会话。已登录显示我的设备、协作者、退出登录。
- `LoginPanel.vue` / `LoginProgress.vue` / `useLogin.ts`：展示 Register → Discover → Relay → Connect 的真实阶段；`enrolled` 不当成 ready。登记后失败调用 `mesh_sync`，登记失败回登录表单重新发起；订阅 progress 并轮询 mesh 状态。慢请求明确提示并防重复提交，不自动重放凭据。
- `DeviceList.vue`：mesh 目录、mesh_status 的 NodeId/端口映射与 tunnel status 联合展示在线、直连/中继、平台、挂盘与逐设备错误；无链路证据时不声称直连。
- `AdvancedPanel.vue`：原生 details 默认折叠，内部通过 v-if 懒挂载。包含 relay 覆盖、`pending_reconnect`、PLRS1 邀请、手工主机 HTTP/HTTPS 地址。打开地址使用系统浏览器访问主机，不把远程主机替换成本机账号会话。原设备码/PLRK1/NodeId/令牌兼容接入、资源管理、应用发布和拓扑保留在其 slot 内。
- `SecurityNotices.vue` / `useSecurity.ts`：订阅 `interconnect:security`，按 deviceId 合并重复接入通知；显示名字、平台和时间。ack 成功才删除，忽略当前页面会话内后续重复事件；失败保留可再次确认。一键吊销调用 `mesh_device_remove`，partial 不当成成功。
- `ApprovalPanel.vue`：操作类别、发起设备 ID、剩余秒数、批准/拒绝；发起方等待期间保留同一操作及冻结后的参数，每 5 秒单飞重试。后端一次性消费批准；成功后清除重试。拒绝/超时或离线错误后停止自动重试，允许用户明确再次发起。卸载/登出清理定时器、订阅和内存操作。
- 高级区的改权限、信任、吊销、共享目录保存也经过同一审批适配；移除手工远程源先成功卸盘，再断隧道并删除本地记录。退出登录先过 mesh_leave 闸，再清协作会话，避免批准尚在等待时先清凭据。
- 新文案在 `copy.ts`；类型/参数在 `contracts.ts`；新逻辑只依赖现有 Vue、Tauri 桥、账号/collab store 与公开 HTTP 接口。

## 后端频控

入口 `polaris-collab/src/collab/http/router.rs` → `rate_limits.rs`，在认证、长轮询及风险 mutation 之前执行。复用 `db::rate_gate` 与既有 `login_attempts` 表；同进程互斥保护读计数/写计数，窗口记录重启后保留；仅清理当前频控 scope 已过期记录。

- read：通知 inbox/watch、签名吊销清单、设备清单、announce、assert。
- write：通知 ack、risk decide/authorize、mesh revoke/rename、peer revoke、admin device_revoke、账号和邮箱 reset。
- login：账号密码/验证码登录与 signup、mesh enroll、login_assertion、collab login/email login/join。
- 每路径独立预算，合法 Bearer 字符串只保存 SHA-256 摘要作为桶键。登录类不使用可任意更换的 Bearer 绕开来源预算。全局与来源上限同时限制随机凭据洪泛；请求头不替代业务认证。
- 超限返回 HTTP 429 和 `interconnect_rate_limited`；配置/存储错误返回 500，不能静默放行。
- 仅信任外壳注入的 ConnectInfo socket IP，不信任客户端 X-Forwarded-For。未注入 ConnectInfo 的外壳共享 unknown-peer 来源桶；反代/NAT 用户可能共享来源预算，可调整配置。未改外壳与代理信任边界。

## 配置、默认值与切换

配置沿用 P6a 的 `POLARIS_INTERCONNECT_CONFIG` 用户 JSON 合并随包默认值；无需新环境变量。新增 `rateLimit`，未知字段、数字字符串、负数、零、小数、超界值均拒绝。

| 字段 | 默认 | 类型/范围 |
| --- | ---: | --- |
| windowSecs | 60 | 整数 1–3600 |
| readRequests | 120 | 整数 1–10000，每路径/设备/窗口 |
| writeRequests | 30 | 整数 1–10000，每路径/设备/窗口 |
| loginRequests | 20 | 整数 1–10000，每路径/来源/窗口 |
| globalMultiplier | 20 | 整数 1–10000，全局及来源上限乘数 |

例如缩小修改类预算，保留正常收件箱/5 秒审批轮询：

```json
{"rateLimit":{"windowSecs":60,"readRequests":120,"writeRequests":20,"loginRequests":20,"globalMultiplier":20}}
```

```sh
export POLARIS_INTERCONNECT_CONFIG=/absolute/path/interconnect.json
```

`useInterconnect` 第四个参数接受 `Partial<Timing>`；默认 pollMs=10000、retryMs=5000、slowMs=30000，均须整数 1000–300000。示例：

```ts
useInterconnect(collab, account, refresh, { pollMs: 15000, retryMs: 10000, slowMs: 60000 });
```

relay 仍使用 P6a 的环境变量 > 高级覆盖 > 权威下发 > 用户/随包配置优先级；高级覆盖仅当前进程，改变活跃 Endpoint 时展示待重连。邀请默认 collaborator 角色，由现有 admin ticket 接口校验权限。所有新参数和默认值均无新 npm/Cargo 依赖。

## 迁移与接口边界

无 DB schema 迁移、无本地存储格式迁移。旧 authority/relay/security 配置自动补 rateLimit 默认值。原有超长 View 本次仅拆触及职责和样式，仍保留 legacy 工具技术债。

- 桌面所需 P6a/P6a2 登录、状态、通知、ack、决定、吊销接口均存在。
- P6b 当时缺口（P6d 的密码 HTTP 会话设备方案见 [后续报告](p6d-http.md)）：浏览器/手机仍缺与 `account_login_password` 等价的安全登录编排 HTTP 入口，以及 mesh 本机设备密钥代理安全命令；本次后端授权范围仅频控，未扩展 LocalConsole 到远程 HTTP。浏览器继续原账号登录，明确提示自动互联需要桌面端；不声称浏览器“登录即互联”已交付。
- P6b 当时缺口（P6d 已添加只读名称/平台与未知占位，见 [后续报告](p6d-http.md)）：批准收件箱返回发起设备 `dev_*` ID，没有设备名/平台或当前本机登记 ID 的映射接口；批准面板如实显示 ID，不能猜设备名。后台仍校验接入次序与禁止自批；本页面发起的 pending ID 不显示自批按钮。
- 后端没有独立的批准结果事件；发起方定时重试完全相同操作，经既有批准闸判断 pending/approved/expired。请求闭包仅保存在当前页面内，重新打开页面不会自动恢复并执行旧 mutation。
- 未配置真实权威 URL/指纹时默认仍是本地模式，不能把配置占位状态当作跨设备成功。真正登录需主控部署并填入 P6a 配置。
- 未运行真实多机器、NAS、iroh relay、网络掉线与挂盘端到端联测；SSR/状态机测试不代表这些集成已经验收。

## 实际验证与文件清单

最终命令结果在下方补记。Cargo 命令工作目录为 `src-tauri/`；前端命令在仓库根。

| 实际命令 | 最终结果 | 日志 |
| --- | --- | --- |
| `npx vue-tsc --noEmit` | 退出 0 | `/tmp/p6b-tsc.log` |
| `npx vitest run` | 退出 0；17 个文件、77 项通过；本模块新增 12 项 | `/tmp/p6b-vitest.log` |
| `npx vite build` | 退出 0；有大于 500 kB chunk 提示 | `/tmp/p6b-vite.log` |
| `cargo check -p polaris-app --offline` | 退出 0；0 警告 | `/tmp/p6b-desktop.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0；0 警告 | `/tmp/p6b-server-net.log` |
| `cargo check -p polaris-cli --bin polaris-server --offline` | 退出 0；0 警告 | `/tmp/p6b-server.log` |
| `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | 退出 0；142 passed、0 failed、0 ignored；0 编译警告 | `/tmp/p6b-collab-tests.log` |
| `git diff --check` | 退出 0 | 终端为空 |
| Python 物理行/字符扫描 | 24 个责任文件检查通过；仅允许保留的旧 View 超 500 行，各行均 ≤500 字符 | `/tmp/p6b-file-table.md` |

首次定向 Vitest 为 8 passed/1 failed：测试错误要求 `<details>` 没有 scoped 属性，已修正为验证实际标签和默认关闭状态。首次 Rust 命令因在 src-tauri 工作目录使用仓库根相对路径创建测试文件失败，编译报告缺测试文件；修正路径后重跑通过。没有把失败或未执行的检查计为通过。

新增 Vitest 覆盖：四阶段状态、登记成功但 relay 失败、mesh_sync 恢复、慢请求防重入/最终成功、通知去重/ack 成功失败/迟到事件、待批准单飞重试/成功停止/拒绝与离线显式重试/取消、批准拒绝接口、实际默认页无敏感输入、高级 slot 默认不挂载、设备链路/平台/挂盘、时序参数类型与边界。

新增 Rust 四项覆盖：多类 HTTP 正常请求与 429、凭据摘要及不同设备/登录来源预算、并发超限与窗口复位、配置非法值及旧配置兼容、生产路由实际安装频控。原有隧道 loopback 测试未设置 `POLARIS_NET_TEST=1` 时内部提前返回；本次 142 passed 不等于真实网络联测执行。

`InterconnectView.vue`：**2757 → 2209 行，净减少 548 行**。样式移至两个 scoped 外部 CSS，既有工具仍留高级区；不靠压行达到约束。以下为责任文件，排除其他任务同时修改的 deploy、review、execution 文件：

| 文件 | 物理行数 | 单行最大字符 |
| --- | ---: | ---: |
| `docs/feature-tree.md` | 500 | 369 |
| `docs/interconnect/p6b-frontend.md` | 117 | 205 |
| `src/features/interconnect/AdvancedPanel.vue` | 84 | 153 |
| `src/features/interconnect/ApprovalPanel.vue` | 24 | 127 |
| `src/features/interconnect/DeviceList.vue` | 28 | 107 |
| `src/features/interconnect/InterconnectView.vue` | 2209 | 183 |
| `src/features/interconnect/LoginPanel.vue` | 31 | 122 |
| `src/features/interconnect/LoginProgress.vue` | 18 | 151 |
| `src/features/interconnect/SecurityNotices.vue` | 16 | 108 |
| `src/features/interconnect/contracts.ts` | 19 | 131 |
| `src/features/interconnect/copy.ts` | 26 | 108 |
| `src/features/interconnect/defaultView.test.ts` | 38 | 137 |
| `src/features/interconnect/interconnect.test.ts` | 163 | 138 |
| `src/features/interconnect/legacy-devices.css` | 267 | 222 |
| `src/features/interconnect/legacy-layout.css` | 273 | 305 |
| `src/features/interconnect/useInterconnect.ts` | 124 | 111 |
| `src/features/interconnect/useLogin.ts` | 70 | 120 |
| `src/features/interconnect/useSecurity.ts` | 89 | 127 |
| `src-tauri/crates/polaris-collab/src/collab/http/rate_limit_tests.rs` | 88 | 120 |
| `src-tauri/crates/polaris-collab/src/collab/http/rate_limits.rs` | 71 | 128 |
| `src-tauri/crates/polaris-collab/src/collab/http/router.rs` | 213 | 92 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/defaults.json` | 23 | 32 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/mod.rs` | 228 | 98 |
| `src-tauri/crates/polaris-collab/src/collab/interconnect/schema.json` | 111 | 81 |
