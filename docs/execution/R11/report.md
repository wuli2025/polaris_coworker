# R11 执行报告：P6b 采纳项 #5 / #6

本报告供 Claude 重跑和 Pi 审查，不替代最终验收。仅处理 triage 的 #5、#6；驳回项未修改，无新依赖、无 commit/push。保留已有未跟踪目录 `docs/execution/R11-1/`、`docs/reviews/P6d-round1/`。

## 入口与行为

- `collab/http/email_routes.rs::collab_email_send_code`：配置/邮箱格式/purpose 校验和频控完成后统一返回 HTTP 200、`{"ok":true}`，兼容原成功响应。账号查找、Argon2、SMTP 都在独立后台阻塞任务执行，HTTP 不等待投递结果。
- `collab/mail/requests.rs::CodeRequest::{prepare,deliver}`：类型化 signup/reset/login；已有账号 signup 仅发提醒并清除旧 signup 码，新邮箱 signup 发验证码；reset/login 仅给已绑定邮箱发验证码，未绑定邮箱不发信。
- `mail.rs::verify_code`：缺失/空码/错误/过期/耗尽/重复使用统一为“验证码无效或已过期”；无记录、过期和耗尽仍执行同参数 Argon2，避免明显省略哈希的时间差。保留一次性消费、10 分钟有效期和 5 次错误限额。已绑定邮箱的旧 signup 码也不能注册。
- 本地 `collab_email_reset`/`collab_email_login`、权威 `account_reset`：验证后邮箱已被解绑/账号已删除时，仍返回统一验码错误。权威 `account_signup` 复用统一校验。已验证身份后的停用、联邦账号权限限制仍保留。
- 频控沿用 SQLite：同邮箱跨用途 60 秒间隔、每小时 5 次；包括提醒、不发信和 SMTP 失败请求。该路由现在传入已有 `client_ip` 提取结果，复用每 IP 每小时 20 次限制；无来源 IP 时仍使用邮箱限制。
- SMTP 失败仅记无邮箱/验证码的通用服务日志，不改变匿名响应；不生成可用新码，不覆盖旧码。成功响应表示请求受理，不能据此推断邮件确实送达。
- `useSecurity` 在事件、收件箱、refresh 和现有 tick 中按 Unix 秒 `expiresAt` 清理到期项；离线、请求挂起、业务 busy 时 tick 仍先清理。`ApprovalPanel` 复用原有每秒时钟过滤展示行和空标题，到期边界为 `expiresAt * 1000 <= Date.now()`。

## 接口、依赖、配置与迁移

依赖仅复用 auth/db、Tokio、Axum、lettre、Vue；假发送器通过显式闭包注入 `CodeRequest::deliver`，无真实网络。旧 `issue_code` / `issue_code_from` 接口保留。独立 `/api/account/send_code` 的首登自动建号业务保持原语义，本次只改变指定 `/api/collab/email/send_code` 的按账号投递行为。

无数据库迁移、无新增运行时开关。旧 `email_codes`/`login_attempts` 继续使用。邮箱请求 purpose 必须为 signup/reset/login；邮箱沿用归一化与格式验证。

- SMTP 沿用 `POLARIS_SMTP_*` 优先、meta 兜底；USER/PASS 无默认，缺失则返回配置错误；HOST 默认 `smtp.qq.com`、PORT 默认 `465`（已有 u16 解析回落），FROM 默认 USER。
- 本地注册沿用 `POLARIS_EMAIL_SIGNUP=0` 或 meta `email_signup=0` 关闭；SMTP 已配置时默认开。权威发 signup 码沿用配置齐即可的既有判定，账号实际注册仍受 `POLARIS_ACCOUNT_OPEN_SIGNUP` 默认关闭的闸控。
- 切换示例：已有 SMTP 凭据部署可设 `POLARIS_SMTP_HOST=smtp.example.com POLARIS_SMTP_PORT=587`；禁用本地邮箱注册设 `POLARIS_EMAIL_SIGNUP=0`。提醒正文模板 `mail/existing_account.txt` 编译期嵌入，修改后重新构建即可切换文案。
- 前端复用 `timing()`：retryMs 默认 5000、pollMs 默认 10000，均校验整数且范围 1000–300000；`useInterconnect(..., { retryMs: 1000 })` 可缩短缓存清理周期；展示沿用组件已有 1 秒时钟。

## 行为测试

`http/email_tests.rs` 使用隔离 DB、可恢复环境变量和假发送器，比较 signup/reset/login 的已注册与未注册邮箱的状态码及响应体字节；验证正确收件人/用途、提醒不含验证码、不发信分支、重发/小时/IP 限流、SMTP 阻塞时先响应和失败不泄露。还覆盖本地注册/找回/验证码登录及权威注册/找回的统一错误、失效旧码和邮箱解绑。测试等待后台任务结束后再清理 DB。

`approvalExpiry.test.ts` 覆盖过期事件、过期收件箱、到期边界、离线及 busy 清理、界面隐藏过期行及空标题。原批准决策测试改为使用未来时间，并验证决策前确实有待批准项。

## 实际验证

Cargo 命令在 `src-tauri/` 执行，前端命令在仓库根执行。所有下列命令均实际运行。

| 命令 | 最终结果 | 日志 |
| --- | --- | --- |
| `cargo check -p polaris-app --offline` | 退出 0，0 警告 | `/tmp/r11-check-desktop.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0，0 警告 | `/tmp/r11-check-server-net.log` |
| `cargo check -p polaris-cli --bin polaris-server --offline` | 退出 0，0 警告 | `/tmp/r11-check-server.log` |
| `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | 退出 0，160 passed，0 failed/ignored，0 编译警告 | `/tmp/r11-collab-tests-final.log` |
| `npx vue-tsc --noEmit` | 退出 0 | `/tmp/r11-vue-tsc-final.log` |
| `npx vitest run` | 退出 0，19 文件、87 测试通过 | `/tmp/r11-vitest-final.log` |
| `git diff --check` | 退出 0 | 无输出 |
| Python 物理行/字符扫描与新增 Rust unwrap/expect 扫描 | 退出 0；所有自有文件 ≤500 行、每行 ≤500 字符；新增 Rust 无 unwrap/expect | 终端输出 |

首轮 Rust 测试在补充权威路径测试和最终验码耗时处理前为 159 passed；之后已全量重跑为上述 160 passed。前端测试调整后也全量重跑，不以首轮结果替代最终结果。

## 改动文件清单

`C` = `src-tauri/crates/polaris-collab/src/collab/`，`UI` = `src/features/interconnect/`。

| 文件 | 改动责任 |
| --- | --- |
| `C/http/email_routes.rs` | 受理/后台任务分离，响应统一，后续缺账号错误统一 |
| `C/http/login_routes.rs` | 权威 reset 的验码后缺账号错误统一 |
| `C/mail.rs` | 提取发送器接口、保留旧接口、统一验码失败及哈希工作量、更新旧断言 |
| `C/mail/requests.rs`（新增） | 类型化目的、统一准入和后台账号相关邮件策略 |
| `C/mail/existing_account.txt`（新增） | 已注册邮箱提醒正文模板 |
| `C/http/email_tests.rs`（新增） | 六项 Rust 行为测试及隔离 fixture |
| `UI/useSecurity.ts` | 本地缓存到期过滤 |
| `UI/ApprovalPanel.vue` | 复用时钟，过滤到期展示行和空标题 |
| `UI/approvalExpiry.test.ts`（新增） | 三项到期行为测试 |
| `UI/interconnect.test.ts` | 决策测试使用真实未来到期时间并断言待批准项存在 |
| `docs/feature-tree.md` | 入口、依赖、配置、迁移与验证索引；维持 500 行 |
| `docs/execution/R11/report.md`（新增） | 本报告 |

任务指定实现和必测无未完成项或 BLOCKED；以下是未运行的外部联测与工具限制。最终是否采纳由 Claude/Pi 核实。

## 未运行与限制

- 未做真实 SMTP 投递或生产环境延迟分布测试；假发送器验证响应不等待 SMTP，不能将其当作实际送达或恒定时间证明。
- 未启用 `POLARIS_NET_TEST=1`；既有 tunnel 两项网络测试在未设置时提前返回，统计 passed 不代表真实 relay/mDNS 联测。
- 尝试 `rustfmt --edition 2021 --config skip_children=true` 格式化本次 Rust 文件，退出 1：当前 stable 工具链未安装 rustfmt。未安装组件或新增依赖。
