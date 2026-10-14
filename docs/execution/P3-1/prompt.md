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
# 任务 P3：权限闸、真沙箱、Polaris 工具、技能与专家团

在 `refactor/pi-agent` 分支上，补齐"换成 Pi 之后不能丢、且要比原来更好"的能力。先读：
- `docs/plans/2026-10-14-pi-refactor.md` 第 1 节、`docs/pi-migration/call-sites.md`（权限、沙箱、专家、技能、提示词相关行）
- `agentd/src/extensions/`（P1 预留的扩展装配点）、`agentd/src/protocol/`
- P2 产出的 Rust 后端 `polaris-kernel/src/chat/backend/`
- Pi 文档：`agentd/node_modules/@earendil-works/pi-coding-agent/docs/extensions.md`、`skills.md`、`containerization.md`，及 `examples/extensions/`（sandbox、gondolin 示例）

## 1. 四档权限（agentd 扩展 + Rust 往返）
现状（`chat/types.rs:10`、`pipeline.rs:64-101`）：Manual→default、AutoCurrent→acceptEdits、AutoAll→bypassPermissions、Deny→plan。**headless 下没人能点同意，所以 Manual 实际等于全放行**——这是要修的缺陷，不是要复刻的行为。
- 在 agentd 写权限扩展，挂 Pi 的工具调用拦截事件（以 extensions.md 中真实存在的事件名为准）：
  - `deny`（只读）：只放行只读工具与联网检索；写/执行直接拒绝并给模型可读的拒绝原因。
  - `manual`：写/执行前发协议事件 `permission.request`（callId、工具、参数摘要），等宿主 `permission.respond`（allow/deny/allowAlways）；超时（可配）按拒绝处理。
  - `auto-current`：工作目录内的读写自动放行；执行命令与目录外写入走 `manual` 流程。
  - `auto-all`：全部放行。
- 协议新增 `permission.request` 事件与 `permission.respond` 请求（同步更新 schema、Rust 类型与协议样例集）。
- Rust 侧：把 `permission.request` 转成前端已有的确认交互（先查前端是否已有授权弹窗组件；没有则新增一个最小确认事件，并在结果里说明需要前端配合的地方）。
- 测试：四档 × {读、写工作目录内、写目录外、执行} 的矩阵；manual 的允许/拒绝/超时/allowAlways。

## 2. 真沙箱（修复"开关不生效"）
现状：`use_sandbox` 打开后仍 `spawn_on_host`；`spawn_in_sandbox` 是死代码。
- 方案：agentd 与密钥留在宿主，**只把 bash 与写文件类工具路由进沙箱**（参照 Pi 的 gondolin/sandbox 示例思路）。复用 `polaris-sandbox` 已有的 Docker 容器管理；路由方式：agentd 以 Pi 支持的方式替换 bash/write/edit 的执行后端，执行请求经协议交给 Rust，由 Rust 调 `polaris-sandbox` 在容器里执行并回传结果。
- 沙箱不可用时明确报错，**禁止静默回落到宿主执行**。
- 删除 `spawn_in_sandbox` 死代码或并入新路径。
- 测试：沙箱模式下命令确实不在宿主执行（用假执行后端断言调用路径）；沙箱不可用时报错。

## 3. 联网能力（Pi 没有内置 WebSearch/WebFetch）
现状依赖 Claude Code 的 `WebSearch,WebFetch` 且默认开启。必须补上，否则"联网搜索/深度搜索"失效。
- `web_fetch` 自定义工具：只允许 http/https；拒绝私网/环回/链路本地地址（防 SSRF，解析后按 IP 判断，重定向后再判）；响应大小、超时、重定向次数上限可配；HTML 转纯文本。
- `web_search` 自定义工具：搜索后端做成可配置适配器（先查 Polaris 是否已有搜索提供商配置，如 `apihub.rs`/`provider/store.rs`；有就复用，没有就实现一个通用 HTTP 适配器接口 + 一个具体实现，Key 走凭据注入）。
- 测试：SSRF 拦截、大小/超时上限、搜索适配器契约（用本地假服务器）。

## 4. 子代理（替代 Claude Code 的 Task 动态编排）
Pi 不内置子代理。在 agentd 实现 `subagent` 工具：在会话注册表里开子会话执行子任务，返回结果摘要。
- 并发上限、深度上限（禁止无限递归）、子会话继承父会话的权限档位与沙箱设置（不能借子代理绕过权限）、父会话中止时级联中止。
- 事件里带 `parentCallId` 让前端能归组显示。
- 测试：并发/深度上限、权限继承、级联中止。

## 5. 上下文文件、技能、专家团
- Pi 读 `AGENTS.md`。兼容旧项目：项目里只有 `CLAUDE.md` 时作为上下文加载；两者都有时 `AGENTS.md` 优先。**不做自动改名**（不改用户文件），在 doctor/迁移文档里给出手动迁移说明。
- 技能：Polaris 技能目录经 `DefaultResourceLoader` 装载（路径进配置）。
- 专家团：保留，默认关闭（配置 `experts.enabled=false`）。开启时从"写 CLAUDE.md"改为"作为系统提示附加段装载"；`expert_apply` 等写文件入口保留但不再被对话主路径依赖。
- 测试：上下文优先级、技能被装载、专家开/关两组配置。

## 规则
- 改动范围：`agentd/`、`polaris-kernel/src/chat/`（含 backend）、必要的 `src-tauri/src/expert*` 与装配点、`docs/`。前端只在权限确认交互确有必要时做最小改动，并在结果中列出。
- 依赖不新增（agentd 与 Rust 都是）；如确实需要 HTML 转文本等能力，用已有依赖或手写最小实现。
- 每文件 ≤500 行、每行 ≤500 字符；可变值进配置；新 Rust 代码无 `unwrap/expect`（测试除外）。
- 必须通过：`cd agentd && npm run typecheck && npm test`（沙箱内 e2e 失败可说明）；`cargo check -p polaris-app --offline`、`cargo check -p polaris-cli --bin polaris-server --features collab-net --offline`、`cargo test -p polaris-kernel --lib --tests --offline`（只允许 baseline.md 的 4 个已知失败）；`npx vue-tsc --noEmit`；行数检查 `python3 "/mnt/c/Users/mi/Desktop/项目skill/scripts/check_lines.py" <改动目录> --limit 500`。
- 更新 `docs/feature-tree.md`。结束时输出：五个部分各自完成情况、改动文件与行数、命令结果摘要、需要前端配合或仍有疑问的点。不要声称做了没做的事。

## 与 P2.5 的衔接（Claude 补充）
- P2.5 已提供 `toolProfile`（档位名→工具清单在 agentd 配置映射）与 `session.configure`，目前映射是最小版本（Deny→readonly，其余→workspace）。本任务在此基础上实现完整四档：`toolProfile` 决定**哪些工具可见**，权限扩展决定**每次调用是否放行**，两者都要有，不能只靠隐藏工具。
- 协议版本：新增 `permission.request` 事件、`permission.respond` 请求等属于协议变更，`protocolVersion` 升到 3，同步样例集、schema 哈希与 Rust 握手。
- 图片与思考档位已在 P2.5b 接通，本任务不要改动它们。

--- END TASK DATA ---
