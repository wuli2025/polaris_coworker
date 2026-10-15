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
# 任务 P6d：浏览器端登录编排 HTTP 入口、审批设备名称映射

P6b 报告的两项缺口（`docs/interconnect/p6b-frontend.md`）：

## 1. 浏览器/手机等价的登录编排 HTTP 入口
- 现状：登录编排（登记设备 → 拉清单 → 应用 relay → 建隧道/挂盘）只有本机 Tauri 命令 `account_login_password`，Web 版（服务端壳）与手机端无等价入口。
- 要求：在 polaris-collab 的 HTTP 路由新增等价入口，复用同一编排函数与状态模型（`interconnect: {stage,state,error,retryable}`、`relayConfig`），并提供状态查询与重试接口。
  - 鉴权与频控：沿用现有登录频控与 P6b 的可配置频控；不得把本机 LocalConsole 准入暴露给远程（P6a 报告第 30 行的约束）。
  - 浏览器场景下设备身份：说明浏览器会话对应的设备记录如何产生（例如浏览器作为"会话型设备"或仅作为已登记主机的客户端），并保证策略 B 的通知/审批语义一致——不能让浏览器会话成为绕过"新设备窗口期高风险审批"的通道。
  - 失败信息不泄露账号是否存在。
- 前端：Web 版（非 Tauri）登录面板改用该 HTTP 入口（`src/features/interconnect/` 内，按运行环境选择 Tauri 命令或 HTTP），浏览器预览 mock 同步。
- **不修改 `mobile/` 目录**：用户的原始仓库中 `mobile/` 有大量未提交的删除，表明手机端正在另行改造；本任务只提供后端接口与文档，供手机端后续接入。
- 测试：HTTP 入口成功/失败/重试、频控、未登记设备在窗口期内的高风险操作仍需批准、错误信息不区分账号是否存在。

## 2. 审批中发起设备的名称
- 待批准列表显示发起设备的名称/平台（不仅是 deviceId）：后端在收件箱/审批数据中附带设备名与平台（来自 mesh 设备目录，只读），前端显示；设备已被吊销或未知时显示占位。
- 测试：名称映射、未知设备占位。

## 规则
- 允许修改：`src-tauri/crates/polaris-collab/`、`src-tauri/src/` 中互联装配与命令注册、`src/features/interconnect/`、`src/tauri.ts`（既有超长，净增 ≤10 行，新逻辑放新文件）、测试、`docs/`。不改 `mobile/`。
- 无新依赖；新文件 ≤500 行、每行 ≤500 字符；新 Rust 代码无 unwrap/expect；三种构建组合 0 警告；`InterconnectView.vue` 不得净增。
- 必须通过：三种 `cargo check --offline`（0 警告）；`cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`；`npx vue-tsc --noEmit`；`npx vitest run`；`npx vite build`。不 commit。

--- END TASK DATA ---
