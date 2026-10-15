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
