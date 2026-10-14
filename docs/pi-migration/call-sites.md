# Pi 迁移调用点与处理清单

基线：2026-10-14，`refactor/pi-agent`，HEAD `0be46d7474d4ec29f764cf253036eb79fda482b8`。
本文是静态源码分析；未运行 CLI、测试、安装器或联网请求。处理列表示后续改造建议，本任务不实施。
总体目标依据 `docs/plans/2026-10-14-pi-refactor.md:25`、`:37`、`:60`；下列现状以代码为准。

## 路径与覆盖口径

为控制行宽，`K/`=`src-tauri/crates/polaris-kernel/src/`，`F/`=`src-tauri/crates/polaris-fable/src/`，`W/`=`src-tauri/crates/polaris-wiki/src/`。
`C/`=`src-tauri/crates/polaris-collab/src/`，`S/`=`src-tauri/crates/polaris-sandbox/src/`，`H/`=`src-tauri/src/`；其余路径相对仓库根。
同格 `file:10,20` 表示同一文件两个证据点。附表 [call-sites-index.md](call-sites-index.md) 列出全部受版本管理文本中的关键词命中行，包括注释、测试、模板与历史文档。
检索使用不区分大小写的 `claude|codex|stream[-_]json`，再追踪 `run_claude_readonly*`、`scope_child_claude*`、`chat_send`、进程启动与命令注册。
关键词命中不等于运行时依赖；真正的调用、存储格式、配置与提示词依赖在下表裁定。动态调用不靠函数名猜测。

## 需要先纠正的现状判断

| 证据 | 结论及迁移含义 |
| --- | --- |
| `K/provider/codex_login.rs:56,182,280,412`；`K/integrations/codex_proxy.rs:1575,1687` | Codex CLI 只有 `--version` 探测；登录由 Rust HTTP OAuth 完成并读写 `~/.codex/auth.json`，推理由代理访问上游。不能描述为 Codex CLI Agent 进程池。 |
| `K/provider/mod.rs:15` 对照 `K/provider/store.rs:640,1427` | 模块头“不路由、授权委托 CLI”的旧注释已不符合实现；以 store/proxy/login 为准。 |
| `K/chat/pipeline.rs:703,727,1726`；`src-tauri/crates/polaris-kernel/Cargo.toml:53` | `useSandbox` 仅阻止常驻路径，随后仍调用宿主 `spawn_on_host`；`spawn_in_sandbox` 是 `dead_code` 保留函数，kernel 没有 sandbox crate 依赖。P3 需要真正接工具路由。 |
| `S/e2b.rs:75,80,89,130` | CubeSandbox 只有配置和 curl 可达性探测，不能把“可达”当成 Agent 已在 E2B 执行。 |
| `H/voice/mod.rs:174,200,534,549,569`；`H/voice/asr.rs:64,136` | 语音关键词主要为纠错词表；ASR 本地识别，润色为 HTTP chat/completions，借 provider 的 MiniMax Key。不是 Claude CLI 调用。 |
| `H/expert.rs:230,407,560,703,760`；`K/chat/pipeline.rs:508` | 手动应用专家/团队才写项目文件；运行时召集/路由返回 prompt 块。`expert_team_spawn` 不是创建真实 Agent 子进程。 |

## Agent 执行、协议与提示词

| 文件:行号 | 模块 / 做什么 | Pi 后处理 |
| --- | --- | --- |
| `K/chat/pipeline.rs:150,233,262,703,727` | chat / 发送、预热、后台拼装；常驻或一次性宿主进程 | 替换为 Backend 调度；保留请求 ID、会话、落库和事件契约。 |
| `K/chat/pipeline.rs:1850,1868,1921,1934,1939` | chat / CLI 路径、print/stream-json、目录、模型、权限、供应商环境 | 保留为旧后端；Pi 请求不再拼 CLI 参数。 |
| `K/chat/pipeline.rs:1726,1741` | chat / 未接入的 docker exec claude | 删除 Pi 主路径中的死分支；旧后端如保留须明确桥接。 |
| `K/chat/session_pool.rs:45,134,150,317,461,551,632,719,776` | chat / 开关、指纹、最多两个常驻会话、JSONL user 输入、result 分轮 | 保留为旧后端；Pi 改单 agentd、多会话及恢复协议。 |
| `K/chat/session_pool.rs:349,380,388,839,878` | chat / 取消、回收、stderr、超时 | 替换进程生命周期实现；保留取消/故障可观测语义。 |
| `K/chat/pipeline.rs:1275,1339,1390,1535,1552,1577,1665` | chat / stream_event/assistant/tool_use/result 解析及去重 | 保留为旧后端；Pi 事件适配到 [agent-events.md](agent-events.md)。 |
| `K/chat/pipeline.rs:64,83,101,673`；`K/chat/types.rs:10,18` | chat / 四档权限转换与工具名单 | 替换为 Pi 权限钩子；不能机械等同 CLI permission-mode。 |
| `K/chat/pipeline.rs:291,508,554,626,663` | chat / CLAUDE.md、专家、技能、raw 模式提示注入 | 替换资源装载；专家默认关闭是目标，当前并非已有开关。 |
| `K/chat/prompt.rs:475,551,855,1091,1145` | prompt / KB、拒答、长任务、目标、Task 子代理编排指令 | 替换 CLI/Task 专属指令；通用业务约定保留。 |
| `K/chat/mod.rs:38`；`K/chat/bridges.rs:48,76` | chat 门面 / 导出命令与 KB/专家依赖倒置接口 | 替换后端实现，保留上层端口，不制造 kernel→fable/expert 依赖。 |
| `K/headless.rs:27,37,52,55,59,125,150` | headless / 只读 Claude、超时、assistant/result 解析 | 替换公共无 UI Agent 调用；旧实现收进旧后端。 |
| `K/conv.rs:194,968` | conv / 人格文件写入；删会话时杀常驻 Claude | 替换上下文路径及 Backend close；保留历史数据。 |

## 供应商、认证、用量与安装

| 文件:行号 | 模块 / 做什么 | Pi 后处理 |
| --- | --- | --- |
| `K/provider/store.rs:3,192,501,640,692,1224,1403` | provider / Anthropic env 形态、providers 存储、路由、初始化、CLI settings | 替换成模型配置映射；Key 与供应商选择保留；CLI 联动归旧后端。 |
| `K/provider/store.rs:1427,1518,1548,1600,1637,1687,1712,1744` | provider / 本地代理 URL、私有 claude-home、onboarding、逐进程 env、联动/路由开关 | 保留为旧后端；Pi 不依赖 `.claude.json`、`CLAUDE_CONFIG_DIR`。 |
| `K/provider/store.rs:2250,2284` | provider 测试 / 构造 claude Command 检查 env，没有 spawn | 替换/保留旧后端回归测试；不是生产调用。 |
| `K/provider/codex_login.rs:36,56,182,267,273,280,412` | 登录 / 版本探测、PKCE 回环、设备码、轮询取消、写 auth.json | 替换 Pi OAuth/Key；CLI 探测删除主流程依赖，旧凭据兼容待确认 U1。 |
| `K/provider/claude_login.rs:24,53,84,159,165,183,243` | 登录 / 读取 credentials、回环或回贴 code、存 OAuth | 替换 Pi OAuth/Key；旧后端登录可保留，凭据迁移待确认 U1。 |
| `K/provider/oauth_loopback.rs:33,38` | 登录 / 两家共用回调槽及 PKCE 流程 | 替换调用接线；通用回环工具是否复用由 Pi 接口决定（U1）。 |
| `K/integrations/codex_proxy.rs:71,177,525,868,1311,1575,1687,1772` | integrations / 本地 Anthropic→Responses/OpenAI/Anthropic 路由、SSE 翻译、token 刷新 | 保留为旧后端；Pi 默认直接走 ModelRuntime。此文件不是 CLI stdout parser。 |
| `K/provider/usage.rs:96,100,103,350` | 用量 / 扫共享与私有 Claude JSONL 账本 | 替换为 Pi usage 持久化；旧账本只读展示保留，跨后端去重待确认 U2。 |
| `K/doctor/probe.rs:262,337,427`；`K/doctor/path.rs:295,392` | doctor / CLI/npm 路径定位、环境净化、Git Bash | 替换默认检测目标为 agentd/Node；通用 PATH/代理净化保留，Claude 特有项归旧后端。 |
| `K/doctor/check.rs:30,166`；`K/doctor/types.rs:22,36,174` | doctor / readiness、PATH 修复及 UI 报告类型 | 替换 CLI 就绪定义；同步调整前端类型。 |
| `K/doctor/install.rs:36,585,809`；`K/doctor/update.rs:102,220` | doctor / 下载原生 CLI、安装日志、检查/更新 | 保留为旧后端的显式动作；Pi 默认安装改 agentd/Node。 |
| `K/doctor/autopilot.rs:81,92,118` | doctor / 启动后自动检查并静默安装 Claude | 替换默认托管对象，避免 Pi 模式继续自动安装 Claude。 |
| `K/doctor/verify.rs:112,173,437,507,559,1031` | doctor / 多安装冲突、配置/登录校验、真实 print 冒烟 | 替换默认验证；旧 CLI 检查保留在旧后端。 |
| `K/doctor/bundled.rs:37,104,147`；`scripts/fetch-runtimes.mjs:14,192` | 运行时 / 内置 Python/uv/Git Bash，Claude shell 兼容布局 | 通用运行时保留；Claude 命名/专属设置归旧后端。 |

## 上下文、专家和模板

| 文件:行号 | 模块 / 做什么 | Pi 后处理 |
| --- | --- | --- |
| `K/claude_md.rs:24,65,76,118,137,175,185,218,261` | 上下文 / 项目与 KB 文件初始化、读写、渲染 | 替换为 AGENTS.md 入口，兼容读旧文件；冲突规则 U3。 |
| `H/persona.rs:151,177`；`H/expert.rs:407,448,560,617,640` | 人格/专家 / 写文件、绑定 persona_id、导出 | 替换为技能/系统提示附加段；保留能力卡，默认关闭；旧导出格式可兼容。 |
| `H/expert/expert_docs.rs:22`；`H/expert/teams.rs:142`；`H/expert/expert_groups.rs:18,36` | 专家 / 模板构建、团队编排与元数据 claude_md_ref | 替换名称和装载方式；能力卡内容保留。 |
| `K/conv.rs:82,142,194`；`F/kb/packs.rs:70` | 内置资料包 / 毛主席人格随包安装 | 替换上下文输出，不能只迁移专家中心的按钮。 |
| `F/kb/scan.rs:141`；`F/kb/access.rs:225,381` | KB / 初始化 CLAUDE.md；清空时保护上下文 | 替换生成/保护名单为新旧上下文兼容。 |
| `W/wiki/compile.rs:64`；`H/templates/project_claude.md:3`；`H/templates/wiki_index.md:16,26` | 提示模板 / 要求模型先读 CLAUDE.md，说明旧工具入口 | 替换提示词；`H/templates/kb_claude.md:1`、`H/templates/mao_persona_claude.md:1` 由上述 include 引用，也纳入。 |
| `H/templates/skills/wechat-md-typesetter/scripts/wechat_yiban.py:1655,1670,1686,1690` | 技能脚本 / 探测并 subprocess 调 Claude（text 输出）生成样式 JSON | 替换为统一 Agent 接口；不能遗漏 Python 子进程。 |
| `H/templates/skills/gz-notion-infographic.md:114,117` | 技能提示 / 明文 `claude -p` 并行执行命令 | 替换 Pi 子任务/技能接口；`:353` 的 Design Codex 是普通词，不是 CLI。 |
| `H/templates/skills/harness-practices.md:1,27,36,52` | 技能提示 / Claude harness、供应商、子代理使用指南 | 替换后端专属操作指令；通用实践保留。 |
| `H/templates/skills/xhs-mao-pipeline.md:36` | 技能提示 / 明确要求使用 `~/.claude/skills/post-to-xhs/` | 替换技能资源定位；保留发布前确认约束。 |
| `H/templates/skills/browser-use/scripts/browser_use_runner.py:84` | 技能脚本 / 从 ANTHROPIC_MODEL 选模型，默认 Claude 模型名 | 无关 CLI 启动；provider 配置迁移时保留/适配独立浏览器模型。 |
| `H/templates/skills/polaris-story-video/manifest.json:9`；`H/templates/skills/polaris-video-studio/manifest.json:9` | 技能元数据 / claude-code、claude-ai 兼容标记 | 替换/扩充 Pi 标记；不是子进程调用。 |
| `PolarisKB/CLAUDE.md:1` | 内置知识库 / 仓库随附旧上下文文件 | 替换命名并兼容读旧文件；不属于历史研究文档。 |
| `K/skills/templates.rs:186,199`；`K/skills/seed.rs:311` | 技能分发 / 内嵌与落盘含 CLI 调用的 Python | 替换源脚本同时更新分发链；避免只改运行时副本。 |

## 间接 Agent 调用与宿主入口

| 文件:行号 | 模块 / 做什么 | Pi 后处理 |
| --- | --- | --- |
| `W/wiki/compile.rs:120,140,155,224` | wiki / 单独写权限 Claude 进程编译知识网，自己解析 stream-json | 替换统一 Backend，保留 kb:compile 进度、写入约束及维护锁。 |
| `F/kb/mod.rs:32`；`F/kb/enrich.rs:205,476` | KB / 转发 headless；自动补链、合并去重 | 替换共享 headless 入口，保留 Rust 校验和落盘。 |
| `F/fable/echo.rs:660,993` | 回声 / 对话记忆抽取与资料分析 | 替换无 UI Agent；保留调度、记忆存储。 |
| `F/fable/ontology.rs:476`；`F/fable/retrieve/ai.rs:205` | 检索 / 本体三元组抽取、查询扩写 | 替换只读推理；保留失败降级及结果校验。 |
| `F/fable/files/llm.rs:243,262,891,997`；`F/fable/files/profile.rs:404` | 文件中心 / 归类、标题、画像；未配独立模型才用 headless | 替换 fallback；独立 HTTP 模型不能误删。 |
| `F/fable/files/tests.rs:618` | 测试 / 显式评测开关下真实模型调用 | 替换评测后端；本次未执行。 |
| `K/integrations/feishu.rs:589,836,856,890` | 飞书 / Node 长连接桥→chat_send→等待会话回复 | 替换 chat 后端即可复用入口；更新 Claude 文案。Node 桥不是 Claude CLI。 |
| `H/lib.rs:177,201,203,458,477,503,530,630` | 桌面装配 / 上下文、doctor 自动启动、命令表、退出回收 | 替换默认启动与生命周期接线；命令兼容适配。 |
| `H/server.rs:53,84,375,472` | server / cwd、上下文初始化、Claude 配置健康报告 | 替换为 Agent 运行时健康信息；cwd 保持业务语义。 |
| `H/apihub.rs:2218,2256,2320,2409` | HTTP 外壳 / chat、上下文、登录、doctor 分发表 | 替换对应实现/兼容命令，不把它当 provider 存储层。 |
| `H/exec.rs:135,308` | 远端执行 / 允许列表含 claude、codex，按 program 启动 | 真可执行入口；Pi 默认名单替换，旧后端命令是否保留需按配置控制。 |
| `Dockerfile:110,118,134,138,143,168`；`docker-compose.yml:90,152` | 部署 / 安装并验证 Claude、持久化凭据目录 | 替换默认镜像依赖为 agentd/Node；旧后端卷兼容保留，勿自动删除用户凭据。 |
| `.env.example:3`；`.env.server.example:41,49`；`scripts/docker-smoke.ps1:24,26` | 配置/烟测 / Anthropic 与 Claude readiness | 替换示例和验收字段。 |
| `src-tauri/tauri.conf.json:61` | 包元数据 / Claude Code 集成描述 | 替换产品描述。 |

## 前端与误命中裁定

| 文件:行号 | 模块 / 做什么 | Pi 后处理 |
| --- | --- | --- |
| `src/tauri.ts:1130,1501,1999,2147,2362,2477` | 前端桥 / 真实 invoke、类型及浏览器 mock | 替换后端专属接口/文案及 mock；保持 chat 事件。 |
| `src/stores/providers.ts:140,197,290`；`src/components/ProviderDock.vue:270,384,854` | 供应商 / 登录启动轮询取消、状态、订阅 UI | 替换 Pi 登录和模型列表，旧后端选项按配置显示。 |
| `src/components/AddProviderModal.vue:219,325`；`src/components/UsageBoard.vue:18,252` | 供应商 / env 参数与 Claude 账本说明 | 替换新模型配置和 usage 来源；历史用量保留。 |
| `src/components/EnvDoctor.vue:85,268,390`；`src/App.vue:190,194` | doctor UI / 安装、更新、autopilot 提示 | 替换默认检测及安装对象。 |
| `src/components/ClaudeMdPanel.vue:76,171`；`src/components/Sidebar.vue:216`；`src/stores/app.ts:24,364` | 上下文编辑 / CLAUDE.md 面板与导航 | 替换为兼容上下文编辑器，保留旧文件读取；Sidebar `:273` 是风格注释误命中。 |
| `src/components/ExpertCenter.vue:136`；`src/components/ExpertTeam.vue:81,180` | 专家 / 应用与导出旧人格 | 替换装载操作，加入默认关闭配置。 |
| `src/components/chat/ChatComposer.vue:513,588,1325`；`src/stores/chat.ts:332,506,572` | 对话 / 模式、预热、旧 CLI 警告 | 替换运行时入口/说明；事件消费者保留。 |
| `src/features/sandbox/components/SandboxStatus.vue:131,241` | 沙箱 UI / 示例与 Claude 沙箱就绪说明 | 替换实际路由状态，不能继续宣称已在容器执行。 |
| `src/stores/automation.ts:323,495`；`src/stores/workflows.ts:248,310`；`src/stores/longtask.ts:9` | 自动化/工作流 / 组装 prompt 并调用 chat | 替换专属指令，执行依赖随 chat 迁移；并非另起 CLI。 |
| `src/components/FeishuSettings.vue:170,264`；`src/components/MediaOps.vue:669`；`H/accounts.rs:10,136` | 飞书/媒体 / chat 间接任务与浏览器账号状态 | 替换文案/Agent 入口；媒体浏览器登录不是模型 OAuth。 |
| `mobile/src/lib/chat.ts:213,271`；`mobile/src/lib/projects.ts:4` | 手机 / 远程 chat 与项目绑定 | 事件兼容保留；Claude 相关注释更新，无本地 CLI 进程。 |
| `mobile/src/lib/auth.ts:225,240`；`mobile/src/lib/hosts.ts:138,205`；`mobile/src/lib/telemetry.ts:51,94` | 手机 / Codex 审查编号注释 | 无关模型登录或 CLI；这些逻辑是 Polaris 主机认证/连接/遥测。 |
| `src/features/interconnect/InterconnectView.vue:66,1049,1426,1569` | 互联 / 前三处为头像来源或审查注释；1569 为执行名单提示 | 注释无关；替换含 claude 的名单文案，与 `H/exec.rs:135` 同步。 |
| `mobile/src/screens/HostsScreen.vue:87,105,143,213,215,230` | 手机主机页 / 全部为 Codex 审查编号注释 | 无关模型 CLI；P6 账号密码流程另行改造，不能误记为模型 readiness。 |
| `H/voice/mod.rs:174,200,569`；`H/voice/asr.rs:136`；`src/components/VoiceSettings.vue:588,593` | 语音 / 专有词纠错与说明 | 无关 CLI；provider Key 借用接口须兼容。 |
| `F/fable/files/thumbs.rs:106`；`H/project.rs:60,66`；`H/server.rs:442` | 子进程误命中 / ffmpeg、shell、df | 无关；不能删除所有 Command::new。 |
| `K/chat/mod.rs:86,93,100`；`K/provider/codex_login.rs:479,487,494` | 子进程误命中 / 打开浏览器 | 无关 CLI 推理；OAuth 浏览器入口随新实现复用。 |
| `S/e2b.rs:130`；`K/integrations/feishu.rs:644,890` | 子进程误命中 / curl、npm、Node 消息桥 | 无关 CLI 推理；保留基础能力。 |
| `F/kb/threat.rs:152` | 安全规则 / 检测 `.claude` 凭据外泄意图 | 保留旧敏感路径识别，并按 Pi 存储补充规则。 |
| `F/fable/index/math.rs:176`；`C/collab/http.rs:4588`；`src-tauri/crates/polaris-protocol/Cargo.toml:5` | 注释/测试 / 审查说明或普通词 | 无关运行时；具体命中见索引。 |
| `src/style.css:51,140,192`；`src/components/chat/TurnItem.vue:127,329,393` | 展示 / Claude/Codex 视觉风格描述 | 无关 CLI；不因换后端改样式。 |
| `src/components/chat/shared.ts:430` | 展示调度 / 空闲加载说明提到 Codex 状态查询 | 无关直接 CLI；登录查询实现归 providers store。 |

## 待确认

- U1：Pi SDK 的 OAuth 提供商覆盖、旧凭据导入/刷新兼容性，须 P1/P4 用实际 SDK 验证；本仓仅能证明旧文件格式（`K/provider/claude_login.rs:243`、`K/provider/codex_login.rs:412`），不推断 SDK 能力。
- U2：旧 Claude JSONL 与 Pi usage 的历史合并/去重规则尚无实现依据（`K/provider/usage.rs:96`）；P4 定数据契约。
- U3：同目录同时存在 AGENTS.md 与 CLAUDE.md 时的优先级、一次性改名的覆盖/回滚策略，规划只给方向（`docs/plans/2026-10-14-pi-refactor.md:40`），P1/P3 定细则。
- U4：sandbox 的 UI 承诺与当前宿主执行不符；P3 须验证真实工具隔离及 E2B 范围（`K/chat/pipeline.rs:703,727,1726`；`S/e2b.rs:13`）。
