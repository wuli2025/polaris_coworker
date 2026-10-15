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
# 任务 R11：P6b 审查采纳项（含既有账号枚举漏洞）
先读 `docs/reviews/P6b-round1-triage.md`。修复 #5 与 #6；驳回项不要改。
- #6（中-高，安全）：`src-tauri/crates/polaris-collab/src/collab/http/email_routes.rs` 的发送验证码接口对匿名调用者统一响应，不再透露邮箱是否已注册/已绑定：
  - signup：已注册邮箱 → 不发验证码，改发"你已有账号，请直接登录或找回密码"提醒邮件（如邮件服务可用）；对外返回与正常发送相同的成功响应。
  - reset / 验证码登录：未绑定邮箱 → 不发送任何邮件；对外返回相同成功响应。
  - 响应时间也不应明显区分两种情况（例如统一在后台任务发送邮件，接口不等待 SMTP 结果）。
  - "邮件服务未配置/未开放注册"这类与账号无关的配置错误可以照常返回。
  - 保持现有频控；提醒邮件也计入频控，防止借此骚扰他人邮箱。
  - 同步检查注册/找回的**后续**步骤（提交验证码时）不泄露（统一"验证码无效或已过期"）。
  - 测试：已注册/未注册邮箱在 signup、reset、login 三种用途下 HTTP 状态码与响应体完全一致；对应邮件行为正确（用假邮件发送器断言）。
- #5（低）：`src/features/interconnect/` 待批准列表按 `expiresAt` 本地过滤过期项；vitest。
允许修改：`src-tauri/crates/polaris-collab/src/collab/`（email 路由、mail、auth 中与验证码相关部分）、`src/features/interconnect/`、测试、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；三种构建组合 0 警告。
必须通过：三种 `cargo check --offline`；`cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`；`npx vue-tsc --noEmit`；`npx vitest run`。不 commit。

--- END TASK DATA ---
