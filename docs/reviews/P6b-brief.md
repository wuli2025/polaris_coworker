# 审查 brief：P6b 互联前端收敛 + 新接口频控
附 `P6b.patch`（前端组件、composables、文案模块、vitest 测试；Rust 频控实现与测试）。设计与用户决定见 `docs/plans/2026-10-14-p6-interconnect.md`；后端接口见 `docs/interconnect/p6a2-policy-b.md`。
## 验收条件
未登录只见账号密码（及邮箱验证码）登录；登录编排分阶段显示、失败可重试、不卡死；已登录见我的设备与协作者；主视图不出现连接码/令牌/地址；高级区默认折叠，含手工连接、relay 覆盖与待生效状态、邀请他人；新设备接入通知按 deviceId 去重、一键吊销与 ack；`approval_pending:<id>` 显示等待批准、批准后自动重试同一操作（只重试一次）、`approval_expired_or_denied:<id>` 明确提示；离线高风险被拒提示"需要联网确认"；策略 B 新 HTTP 接口频控（可配置，超限 429）；`InterconnectView.vue` 只减不增，新组件 ≤500 行，文案集中。
## Claude 实测
vue-tsc 通过；vitest 77/77；vite build 通过；三种构建 0 警告；collab 142 两次全过；主视图组件中无"连接码/令牌/PLRK1/token"字样。
## 请重点审查
自动重试是否可能重复执行高风险操作（幂等与只重试一次）；通知去重与 ack 的竞态；吊销按钮是否有二次确认、是否可能误吊销当前设备；频控键（按 IP/账号/设备）能否被绕过或误伤 NAT 后多用户；前端是否在任何地方渲染或记录令牌/密钥；登录失败信息是否泄露账号是否存在。
