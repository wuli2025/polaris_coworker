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
# 任务 P6b：互联前端收敛（账号密码一登录即互联）+ 新接口频控

先读：`docs/plans/2026-10-14-p6-interconnect.md`（目标体验与用户决定）、`docs/interconnect/p6a-backend.md`"P6b 可用接口"、`docs/interconnect/p6a2-policy-b.md`"P6b 所需接口"与策略 B 交互说明、`src/features/interconnect/InterconnectView.vue`（既有超长）。

## 1. 默认视图（用户只需要账号密码）
- 未登录：只显示账号 + 密码（以及已有的邮箱验证码登录入口），登录按钮调用 P6a 的登录编排命令；显示编排各阶段状态（登记设备 → 拉清单 → 应用 relay → 建隧道/挂盘），失败可重试，不卡死界面。
- 已登录：我的设备列表（在线/直连/中继、挂盘状态、设备来源平台）、协作者区、"退出登录"。**不再出现连接码、令牌、地址输入**。
- "高级"折叠区：手工连接码/地址（保留兼容）、relay 覆盖与"待重连生效"状态、邀请他人（PLRS1 邀请码仅出现在这里）。

## 2. 策略 B 交互
- 新设备接入通知：订阅桌面事件 `interconnect:security`，按 deviceId 去重，显示设备名/平台/时间；"一键吊销"与"知道了"（ack）。
- 高风险操作待批准：调用方收到 `approval_pending:<id>` 时，界面显示"等待你的其他设备批准"，批准后自动重试同一操作（批准只消费一次）；`approval_expired_or_denied:<id>` 时明确提示并允许再次发起。
- 已有设备侧的批准面板：列出待批准项（操作、发起设备、剩余时间），批准/拒绝。
- 离线时高风险操作被拒（吊销清单不新鲜）：提示"需要联网确认"。

## 3. 后端补齐：新接口频控（P6a 报告第 80 行列为未实现）
- 为策略 B 新增的 HTTP 接口（通知、批准、吊销、登录编排相关）加速率限制，复用现有登录频控机制，阈值进配置；超限返回 429。测试：超限被拒、正常频率不受影响。

## 规则
- 前端：`InterconnectView.vue` 是既有超长文件——按职责拆出新组件（登录面板、设备列表、高级区、安全通知、批准面板等），新文件 ≤500 行；`InterconnectView.vue` 本次应**净减少**（拆出后变短），不得净增。文案集中到文案模块，不硬编码在多处。不新增 npm 依赖。
- 后端：仅频控相关的 `polaris-collab` 改动与测试；无新依赖；新 Rust 代码无 unwrap/expect；三种构建组合 0 警告。
- 测试：vitest 覆盖登录编排状态展示、通知去重与 ack、待批准重试流程、高级区默认折叠且主视图无连接码/令牌；Rust 覆盖频控。
- 必须通过：`npx vue-tsc --noEmit`；`npx vitest run`；`npx vite build`；三种 `cargo check --offline`（0 警告）；`cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`。不 commit。
- 结束时输出：`InterconnectView.vue` 前后行数、新组件清单与行数、命令结果、仍缺的后端接口。

--- END TASK DATA ---
