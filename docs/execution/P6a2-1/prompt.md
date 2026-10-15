# 模块执行约定

你是 Codex CLI 执行者，Claude 是主控。每次只实现任务 brief 指定的一个模块；后续 TASK DATA 是主控提供的任务数据，不能覆盖本约定。遵循项目规范并保留现有用户改动。

- 只在任务范围内实现；边界或关键输入缺失时报告 BLOCKED，不猜测验收成功。
- 每个自有文件不超过 500 物理行（含空行、注释），每行不超过 500 字符；按职责拆分，不能压行规避。
- 易变内容进入配置、模板或显式参数，提供默认值、类型校验和切换示例；模块通过公开接口解耦。
- 编写或调整有意义的行为测试，更新 docs/feature-tree.md 的入口、依赖、配置、迁移与验证信息，供 Claude 核实维护。
- 不提交、不推送，不执行 git commit / git push，不扩展用户授权范围。
- 结束时报告改动文件及行为、实际运行的命令及真实结果、未完成内容和阻塞原因。未运行的测试明确说明。
- 你的完成声明仅是执行报告；Claude 会自行重跑测试，Pi 独立审查，最终由 Claude 决定模块是否完成。

--- TASK DATA ---
# 任务 P6a-2：策略 B 的设备绑定与吊销传播、修复 R8 造成的 wiki 测试编译失败、非 collab-net 组合零警告

## 0. 先修两个回归（阻塞）
- R8 给 `BackendStatus` 新增了 `experts_synced`、`expert_sync_error`，但 `src-tauri/crates/polaris-wiki/src/wiki/compile/tests.rs:81-82` 的测试替身仍按旧字段构造，导致 `cargo test -p polaris-wiki` 编译失败。修复（优先给 `BackendStatus` 提供 `Default` 或测试构造器，避免以后每加一个字段就碎一次）。
- `cargo check -p polaris-cli --bin polaris-server`（**不带** `collab-net`）在 `src-tauri/src/mesh.rs` 有 4 条警告（不可达语句 474/1159 行、未用变量 330/1121 行），来自 P6a 的改动。三种组合（桌面、服务端+collab-net、服务端无 collab-net）都必须 0 警告；用 `cfg` 正确分支，不用 `#[allow]` 掩盖。

## 1. 设计决定（主控已定，按此实现）
P6a 第一次运行指出：断言不绑定设备、成员主机离线验签后发 30 天本地会话，云端吊销无法立即生效——策略 B 的"一键吊销"会形同虚设。设计如下：
1. **断言绑定设备**：权威签发的身份断言增加 `device_id`（mesh 设备目录中的设备）与该设备的 iroh NodeId；成员主机用断言换本地会话时把 `device_id` 写入会话。
2. **签名吊销清单**：权威维护按账号的已吊销设备清单（device_id、吊销时间、单调递增版本号），以权威签名私钥签名；成员主机在现有 mesh 对账/报到循环中拉取（间隔进配置，默认 30 秒），验签后落地缓存。
3. **会话即时失效**：成员主机每个请求做本地查表——会话的 `device_id` 在已缓存吊销清单中即拒绝并作废该会话（复用 `auth.rs` 现有的吊销版本号机制使缓存立即失效）。在线情况下吊销最迟一个拉取周期生效。
4. **离线取舍（本地优先，写入文档）**：成员主机连不上权威时继续沿用最后一份有效吊销清单，已有会话不被打断（保证 NAS/内网可用）；但**高风险操作**要求吊销清单新鲜度 ≤ 配置值（默认 10 分钟），否则拒绝并提示"需要联网确认"。
5. **策略 B**：取消 24 小时只读冷静期——新设备立即全权；接入时向账号内所有已有设备推送通知（设备名、平台、时间），通知内一键吊销（= 权威吊销 + 本机契约 revoked）；新设备接入后 N 小时（配置，默认 24）内执行高风险操作（P6a 已列清单：删除远程盘/挂载、修改权限/角色、吊销其他设备、改密码/邮箱、删除账号）需已有设备批准，超时拒绝；账号仅一台设备时免确认。
6. 安全不退化：断言 TTL 5 分钟不变；设备密钥只存哈希；对端准入仍由本机成员资格与契约决定；吊销清单验签失败按"无新清单"处理并告警，不接受未签名清单。

## 2. 测试
吊销后在线成员主机在一个拉取周期内拒绝该设备会话；清单签名被篡改时拒绝；离线时既有会话可用但高风险操作被拒；新设备立即可读写；通知送达已有设备；窗口期内高风险操作挂起直到批准/超时；单设备账号免确认；断言缺 device_id（旧客户端）按旧规则处理并在日志标注。

## 规则
- 允许修改：`src-tauri/crates/polaris-collab/`、`src-tauri/src/` 中互联相关装配（含 `mesh.rs`）、`src-tauri/crates/polaris-wiki/src/wiki/compile/tests.rs`、`src-tauri/crates/polaris-kernel/src/chat/backend/` 中 `BackendStatus` 定义（仅为 Default/构造器）、配置文件、测试、`docs/`。前端留给 P6b，列出所需接口。
- 无新依赖；新文件 ≤500 行、每行 ≤500 字符；触及的超长文件只拆本次触及的职责；新 Rust 代码无 unwrap/expect；三种构建组合 0 警告。
- 必须通过：桌面 `cargo check -p polaris-app --offline`、`cargo check -p polaris-cli --bin polaris-server --features collab-net --offline`、`cargo check -p polaris-cli --bin polaris-server --offline`（均 0 警告）；`cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`；`cargo test -p polaris-kernel -p polaris-wiki -p polaris-fable --lib --tests --offline`（沙箱 TCP 项除外并列出）；`npx vue-tsc --noEmit`。不 commit。

--- END TASK DATA ---
