# Polaris → Pi 底座重构规划

日期：2026-10-14 · 分支：`refactor/pi-agent`（克隆在 `~/work/polaris-pi`，不动 `D:\polaris\polaris-app` 里未提交的改动）

## 0. 现状诊断（读代码得出的结论）

| 区块 | 现在怎么做 | 问题 |
| --- | --- | --- |
| Agent 内核 `polaris-kernel/chat` | Rust 维护 `claude` CLI 子进程池（`session_pool.rs` 1170 行），解析 stream-json（`pipeline.rs` 2080 行，`prompt.rs` 1278 行） | 绑死 Claude Code 的 CLI 参数与输出格式；CLI 一升级就可能断；单文件远超 500 行 |
| Codex 接入 `integrations/codex_proxy.rs` | 2269 行的代理层 + `codex_login.rs` | 第二套完全不同的协议，维护两份 |
| 专家团 `src/expert*` | 100 张能力卡 → 运行时 RRF 召回 → **写进项目 `CLAUDE.md`** | 靠改文件注入人格，和 CLI 强耦合；按指示保留但默认关闭 |
| 提供商 `provider/store.rs`、`apihub.rs` | 自管 Key 与切换，再翻译成 Claude CLI 的环境变量 | Pi 的 `ModelRuntime` 原生支持多家，可直接注入 |
| 互联 `polaris-collab` | 已有 argon2 账号密码、云端账号权威签发身份、同账号设备网、iroh 隧道、n0 公共 relay | 用户侧仍要处理 host.key / device.key / 授权码 / 手动批准设备，链路过长；relay 用的是 n0 公共节点 |
| 其他依赖 Claude CLI 的模块 | `headless.rs`、`doctor/*`（检测/安装 claude）、`wiki/compile.rs`、`voice`、`sandbox/commands.rs`、飞书桥 | 都要改走同一个 Agent 后端 |
| 规模 | Rust 99k 行 + 前端 67k 行；数十个文件 >500 行（最大 4823 行） | 只拆分本次触及的文件，其余列清单不擅动 |

**另一处发现**：`D:\Canvas666\infinite-canvas-agent\canvas-agent` 10 月 12 日接了一个 Pi 运行时，但 `runtime.ts` 引用的 `./pi.js` 根本不存在，只有 13 行类型声明——这正是"空套一个 package"的反例。本次在 Polaris 用 SDK 真接，验证通过后可以把同一个 agentd 给 Canvas 复用。

## 1. 目标架构（去粗取精，只取 Pi 的这几层）

```
Vue 前端 ──Tauri/HTTP──▶ Rust 壳(polaris-kernel)
                             │  AgentBackend trait（新）
                             ├─ PiBackend（默认）──stdio JSONL──▶ polaris-agentd（Node 22, TypeScript）
                             │                                     ├─ @earendil-works/pi-coding-agent  createAgentSession / SessionManager
                             │                                     ├─ @earendil-works/pi-ai            ModelRuntime（多提供商）
                             │                                     └─ 扩展：权限闸、沙箱 bash、知识库工具、技能/专家装载
                             └─ ClaudeCliBackend（旧，配置可切回，默认关）
```

取：`pi-coding-agent` SDK（会话、工具、压缩、重试、steer/followUp）、`pi-ai`（模型）、扩展机制（`tool_call` 钩子做权限、`customTools` 做 Polaris 工具）、`DefaultResourceLoader`（技能、AGENTS.md）。
不取：Pi 的 TUI、`pi-server/client/protocol`（实验性；Polaris 已有自己的协作协议）、`chord`、`durable`。

### 关键设计
1. **一个常驻 agentd 进程，多会话**：取代"每会话一个 claude 子进程"。Rust 侧只做进程守护、请求路由与事件转发；崩溃自动拉起，会话从 JSONL 恢复。
2. **协议放 `polaris-protocol` crate + `agentd/src/protocol.ts`**：请求 `session.open/prompt/steer/followUp/abort/close`、`models.list`、`config.apply`；事件 `turn.started/text.delta/reasoning.delta/tool.started/tool.completed/usage/turn.completed/error`。两端各有 schema 校验与契约测试，前端事件形状不变（由 Rust 适配层翻译）。
3. **配置不硬编码**：`config/agent.default.json` + JSON schema；模型、超时、并发上限、重试、工具白名单、权限档位、专家团开关、沙箱模式、relay 列表全部走配置；提示词进 `prompts/`。
4. **权限与沙箱**：Pi 本身没有权限系统。Polaris 的四档授权由 agentd 扩展在 `tool_call` 上拦截：只读/询问/自动/完全；"询问"经 Rust 弹窗回传；沙箱模式把 bash/写文件路由进现有 Docker 沙箱。
5. **专家团保留但默认关**：从"写 CLAUDE.md"改为"作为 Pi 技能/系统提示附加段装载"，开关 `experts.enabled=false`。
6. **上下文文件**：Pi 读 `AGENTS.md`；兼容读取旧 `CLAUDE.md`，迁移脚本一次性改名。

## 2. 互联：账号密码一登录即互联

现状已有账号权威与设备网，问题是"要 token、要配对"。改成：
1. 任何一端（桌面 / NAS / 手机）只填**账号 + 密码** → 权威签发会话 + 设备证书，设备 key 在本机自动生成、自动登记，**不再展示或手填 token/授权码**。
2. 同账号设备默认互信（可在设置里收紧为"新设备需确认"）；登录后后台自动对账设备目录、自动建隧道。
3. 跨账号协作仍走团队邀请（权限表不变）。
4. **Relay 部署**：iroh relay 需要长连接 + UDP/QUIC，**Cloudflare Workers 跑不了**。建议：
   - 新加坡服务器：`iroh-relay`（中继）+ 账号权威服务（`polaris-server --features server,collab-net`），Docker Compose 一键部署，健康检查 + 自动重启；
   - Cloudflare：只做 DNS + TLS + 可选 Tunnel/WAF 挡在账号权威 HTTP 前面。
   - relay 地址进配置，客户端登录时由权威下发（已有 `apply_relay_config`，改为登录自动下发）。
   - 部署到你的服务器需要你提供 SSH 访问/域名，**部署前会单独确认**。

## 3. 分阶段执行（每阶段闭环：实现 → 测试 → 行数检查 → 功能树更新 → Codex CLI 交叉审查 → 处置意见）

| 阶段 | 内容 | 验收 |
| --- | --- | --- |
| P0 基线 | 装 Node 22/Rust、克隆分支、跑现有 `cargo check`/`vue-tsc`/测试记基线、产出依赖 claude 的调用点清单、`docs/feature-tree.md` | 基线报告；清单覆盖所有 `claude`/`codex` 调用点 |
| P1 agentd | 新建 `agentd/`（TS），Pi SDK 接入、协议、配置 schema、会话持久化、模型列表、事件映射 | 单元测试 + 用 Pi 的 faux 提供商跑真实 agent loop（工具调用、steer、abort、崩溃恢复）；**真实模型冒烟一次** |
| P2 Rust 后端抽象 | `AgentBackend` trait、`PiBackend`（进程守护/路由/回压/超时）、旧 Claude 后端收进适配器；拆 `session_pool/pipeline/prompt` 到 ≤500 行 | `cargo test`；契约测试两端对齐；切换配置两组输入验证 |
| P3 权限/沙箱/工具 | 四档授权扩展、沙箱路由、知识库/产物工具、技能装载、专家团（默认关） | 各档位拦截用例；沙箱内执行验证 |
| P4 提供商 | provider store → Pi ModelRuntime；Claude/Codex 登录改走 Pi 的 OAuth/Key | 两个提供商切换验证 |
| P5 其他模块 | headless、doctor（检测 agentd 而非 claude）、wiki 编译、语音、飞书桥、前端运行时选择器 | 各模块冒烟 |
| P6 互联 | 账号密码一登即连、设备自动登记、relay 配置自动下发、relay/权威部署包 | 两台设备（本机 + 容器模拟）登录即互通的演练 |
| P7 演练与交付 | 端到端演练：真实对话任务、工具调用、长会话压缩、断网重连、agentd 崩溃恢复、并发会话压测；Docker 镜像 | 演练记录 + Codex 审查记录 + 已知缺口清单 |

## 4. 规则

- 新写/改动的自有代码：单文件 ≤500 行，单行 ≤500 字符（两条都检查）；不压行凑数。
- 交叉审查用 **Codex CLI**（不用 Grok）。审查脚本放 `scripts/review/codex_review.py`，只发必要文件，不发密钥与用户数据。
- 未触及的历史超长文件只列清单，不在本次擅自全库拆分。
- 不提交到 `main`、不推远程、不部署服务器，除非你明确同意。

## 5. 需要你后续提供/确认的事

1. 新加坡服务器的 SSH 访问方式与域名（P6 部署时）。
2. P1 真实模型冒烟用哪个提供商的 Key（默认读 Polaris 已配置的提供商）。
3. Codex CLI 在 WSL 里沿用 `C:\Users\mi\.codex` 的登录。
