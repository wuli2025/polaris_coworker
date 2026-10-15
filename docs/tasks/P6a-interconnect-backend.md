# 任务 P6a：互联后端——登录即成网、relay 随登录下发、新设备策略 B

先读：`docs/plans/2026-10-14-p6-interconnect.md`（含用户决定）、`polaris-collab/src/collab/` 的 `authority.rs`、`mesh.rs`、`grants.rs`、`identity.rs`、`tunnel.rs`、`gateway.rs`、`http.rs`（路由）、`auth.rs`；`src-tauri/src/` 中 mesh/hosting/beam 的装配。

## 1. 权威地址与公钥：内置配置
- 新增互联配置（schema + 默认值文件，随包分发）：`authority.url`、`authority.publicKeyFingerprints`（预置，用于首次接触即校验，消除 TOFU 首次窗口；允许多个以便轮换）、`relay.fallback`（是否允许 n0 公共 relay 作后备，默认允许）。环境变量 `POLARIS_ACCOUNT_AUTHORITY_URL` 等仍可覆盖，优先级：环境变量 > 用户配置 > 随包默认。
- 预置指纹与权威实际公钥不符时拒绝连接并给出明确错误（不静默 TOFU）。
- 默认值先用占位（部署后由主控填真实域名与指纹），**占位状态下客户端表现为"未配置权威"，回落现有本地模式，不报错崩溃**。

## 2. relay 随登录下发
- 权威侧：配置 relay 列表（URL，可多个）；在登录、设备报到（mesh 取清单）的响应中附带 relay 列表。
- 客户端：收到后自动 `apply_relay_config`；共享 Endpoint 已建立时按现有约束（需断开重连）在下次重连生效，并在状态中显示"待生效"；列表为空时按 `relay.fallback` 决定是否用 n0。
- 不再需要手工调用 `/api/collab/tunnel/relays`（保留为高级覆盖）。

## 3. 新设备策略 B（用户已决定）
- 同账号新设备首次接入：**立即全权**（取消 24 小时只读冷静期）。
- 账号内所有已有设备收到"新设备接入"通知（设备名、平台、时间、来源 IP 粗粒度位置如可得），通知里提供一键吊销（复用 mesh 的按设备吊销 + grants 的 revoked）。
- **高风险操作需已有设备确认**：先在代码中识别并列出高风险操作清单（至少：删除远程盘/挂载、修改设备权限/角色、吊销其他设备、改账号密码/绑定邮箱），新设备（接入后 N 小时内，N 进配置，默认 24）执行这些操作时，生成确认请求推送到已有设备，已有设备批准后才执行；超时拒绝。账号只有这一台设备时不需要确认。
- 安全不退化：断言 TTL、设备密钥只存哈希、对端准入仍由本机成员资格与契约决定，均保持。
- 测试：新设备立即可读写；旧设备收到通知并能吊销，吊销后新设备立即失去访问；新设备在窗口期内执行高风险操作被挂起直到确认/超时；单设备账号无确认；预置指纹不符拒绝。

## 4. 登录流程收敛
- 登录成功后的自动流程（登记设备 → 拉清单 → 应用 relay → 按契约建隧道/挂盘）在一个可测试的编排函数里完成，每步失败有明确状态，可重试，不卡死 UI。

## 规则
- 允许修改：`src-tauri/crates/polaris-collab/`、`src-tauri/src/` 中与互联相关的装配与命令注册、新增配置文件、测试、`docs/`。前端留给 P6b（只需保证类型/命令可用，并在结果中列出前端需要的接口）。
- 触及的超长文件（`authority.rs` 1490、`http.rs` 4800+、`tunnel.rs` 1144、`auth.rs` 1534 等）只拆本次触及的职责；未触及部分列入技术债。
- 无新依赖；≤500 行/≤500 字符（新文件）；新 Rust 代码无 unwrap/expect；编译警告保持 0；可变值进配置。
- 必须通过：两壳 `cargo check --offline`（0 警告）；`cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`（基线 112 全过，沙箱 TCP 项除外并列出）；`cargo test -p polaris-kernel --lib --tests --offline`；`npx vue-tsc --noEmit`。不 commit。
- 结束时输出：高风险操作清单及依据、配置项说明、前端所需接口、改动文件与行数、命令结果。
