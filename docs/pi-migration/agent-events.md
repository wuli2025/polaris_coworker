# Agent → 前端事件契约

基线：2026-10-14，HEAD `0be46d7474d4ec29f764cf253036eb79fda482b8`；仅静态读取。
路径缩写：`K/`=`src-tauri/crates/polaris-kernel/src/`，`F/`=`src-tauri/crates/polaris-fable/src/`，`W/`=`src-tauri/crates/polaris-wiki/src/`，`H/`=`src-tauri/src/`。
范围：对话 Agent 流，以及迁移 headless/wiki/doctor 时必须继续提供的外围事件。不是全应用事件总表（文件同步、媒体渲染、设备遥测不属于 Agent 流）。

## 传输封装与关联键

| 层 | 已有行为 | 证据 |
| --- | --- | --- |
| Rust 对话 emitter | topic 固定为 `chat:stream`，payload 为 `ChatStreamEvent` | `K/chat/pipeline.rs:1718` |
| Rust JSON 序列化 | camelCase；仅 `reqId`、`kind` 必填；`text`、`tool`、`conversationId` 的 None 省略而非 null | `K/chat/types.rs:106` |
| Tauri 前端桥 | `listen` 将 Tauri event 的 payload 交回业务回调 | `src/tauri.ts:445` |
| 无桌面 runtime | `AppHandle.emit` 广播 Event；WS 帧只有 `{topic,payload}`，内部 audience 不在帧内 | `src-tauri/crates/polaris-runtime/src/host.rs:14,29,77,95` |
| 桌面→远端 | 单向监听 `chat:stream`→内嵌主机 bus；不能假设所有 Tauri 事件都向远端桥接 | `H/hosting.rs:324` |
| HTTP WS | 鉴权后订阅 bus；基础事件流要求 owner | `H/apihub.rs:2560,2562` |
| 手机 | 解析 `{topic,payload}`，按 topic 分发 | `mobile/src/lib/net.ts:315,345` |
| 请求 | `chat_send` 返回 reqId，后台可能已在发事件；用户消息先持久化 | `K/chat/pipeline.rs:150,171,178` |
| 桌面关联 | 按 conversationId 路由；无 conversationId 的事件丢弃；先注册监听再发送 | `src/stores/chat.ts:124,504,507` |
| 手机关联 | conversationId 存在且非当前会话才丢弃；缺失时会落当前会话 | `mobile/src/lib/chat.ts:213` |

## `chat:stream` 六种实际事件

所有行均携带 `reqId:string`、`kind:string`，以及可选 `conversationId:string`。下表给出附加字段；省略字段不能凭前端类型推断为后端已实现。

| kind | 附加字段 / 语义 | Rust 发出位置 | 前端消费位置 |
| --- | --- | --- | --- |
| `meta` | `text:string`，十进制估算输入 token 数；不是实际计费用量 | `K/chat/pipeline.rs:680,682` | `src/stores/chat.ts:567` parseInt 写 tokensByConv；手机无分支（`mobile/src/lib/chat.ts:215`） |
| `delta` | `text:string`，追加正文片段；`tool` 省略 | `K/chat/pipeline.rs:1321,1374,1423,1597,1677` | `src/stores/chat.ts:511`；`mobile/src/lib/chat.ts:215`；各自 40ms 合批 |
| `tool` | `tool:string` 工具名（缺失上游名为 unknown）；`text?:string` 输入摘要；没有结果字段 | `K/chat/pipeline.rs:1610,1617,1971` | `src/stores/chat.ts:518` 保存 toolDetail；`mobile/src/lib/chat.ts:222` 只展示工具名 |
| `artifact` | `text:string`，标准化正斜杠文件路径；应用文件夹尾随 `/`；`tool` 省略 | `K/chat/pipeline.rs:1484,1644` | `src/stores/chat.ts:526` 去重并可自动预览；`mobile/src/lib/chat.ts:229` 收集产物 |
| `error` | `text:string`，错误或 `[stderr] ...` 诊断；没有 Rust severity/retryable | `K/chat/pipeline.rs:199,892,932,1031,1698`；`K/chat/session_pool.rs:815,858,956` | `src/stores/chat.ts:571` 分警告/错误；`mobile/src/lib/chat.ts:245` 所有 error 都清运行态 |
| `done` | 不带 text/tool；结束本轮，成功/失败不由该事件区分 | `K/chat/pipeline.rs:209,1520`；常驻通过共享 TurnStream 收尾 | `src/stores/chat.ts:586,631` 清请求、结束工位、唤醒批次；`mobile/src/lib/chat.ts:249` 清运行态 |

`K/chat/types.rs:110` 注释只列四种，不能据此漏掉 meta/artifact；Rust 实际发出点和前端联合类型一致（`src/tauri.ts:1086`）。

## 各来源与顺序要求

| 来源 | 当前转换方式 | 证据 / Pi 必须兼容的行为 |
| --- | --- | --- |
| stream-json 部分消息 | `stream_event.event.content_block_delta.delta.type=text_delta`→delta，Rust 约 30ms 合批 | `K/chat/pipeline.rs:1299,1552`；Pi text.delta 也应追加，不重复推完整最终文本 |
| 整块 assistant | text 记账；若本消息已有部分流则不再次显示；tool_use→tool | `K/chat/pipeline.rs:1577,1595,1610` |
| result | 缺 assistant 正文才补记账；从未部分流才补显示；error 子类型另发 error | `K/chat/pipeline.rs:1665,1673,1691` |
| 未识别 JSON 类型 | system、user 回显及其他类型仅 flush，不转成新 topic | `K/chat/pipeline.rs:1711` |
| 非 JSON 行 | flush 后作为 delta，并纳入正文缓冲 | `K/chat/pipeline.rs:1411`；这是旧 CLI 调试兼容，不是 Pi 协议要求 |
| 不支持生图提示 | 首段 delta 为 Rust 合成说明，也纳入正文 | `K/chat/pipeline.rs:1370` |
| 写文件工具 | Write/Edit/MultiEdit/NotebookEdit 输入路径经格式过滤后实时 artifact | `K/chat/pipeline.rs:1627`；Pi 工具名不同须适配，不可只复制名字判断 |
| 轮末产物扫描 | 前后快照 diff 发现脚本生成文件；应用目录归并；落库前剔除不存在产物 | `K/chat/pipeline.rs:1464,1497` |
| 正常收尾 | flush→产物整理→assistant 落库→done→清取消标记 | `K/chat/pipeline.rs:1445,1507,1520`；常驻调用 `K/chat/session_pool.rs:776` |
| 后台准备异常/panic | error 后 done，均携带同一 reqId / conversationId | `K/chat/pipeline.rs:178,199,209` |
| stderr | 每个非空行带 `[stderr]` 前缀，以 error kind 发出 | `K/chat/pipeline.rs:909,932`；`K/chat/session_pool.rs:839,858` |
| 崩溃/超时 | 异常退出、result error、看门狗各有错误文本；不一定只有一条 error | `K/chat/pipeline.rs:892,1031,1698`；`K/chat/session_pool.rs:815,956` |
| 取消 | `chat_cancel` 杀请求/常驻会话；早期取消有单独快速返回分支 | `K/chat/pipeline.rs:1063,693`；终态完整性须验证，见 U5 |

## 不是当前已实现的契约

- 前端 `callId/phase/status/summary/severity/retryable` 仅为可选扩展；Rust struct 无这些字段（`src/tauri.ts:1092` 对照 `K/chat/types.rs:108`）。
- 当前没有 reasoning、tool.completed、usage、permission-request 专门事件；chat emitter 仅以上六种（`K/chat/pipeline.rs:1535,1718`）。
- `meta` 是 prompt 估值，历史账本的真实 usage 通过查询获取（`K/provider/usage.rs:96`），两者不能混用。
- 专家状态由 `expert_agents_status` 查询内存表，不是子 Agent 生命周期事件（`H/expert.rs:230,266`）。
- 规划中的 `turn.started/text.delta/reasoning.delta/tool.started/tool.completed/usage/turn.completed/error` 是未来 agentd→Rust 协议，不是现有前端 topic（`docs/plans/2026-10-14-pi-refactor.md:36`）。

## headless / wiki 外围事件

这些 topic 由业务模块拥有，Pi 替换推理调用时继续保留。`headless` 本身只回调 `tool/delta`，不会直接 emit（`K/headless.rs:150,157`）。
字段类型：runId/kind/text/message/schemaId/note/report 为字符串；docCount/applied/merged/episodes/kept/dropped/clusters/assigned/count 为整数计数。
依据为各表格行的 Rust struct/JSON 构造；report 的返回类型见 `F/fable/files/llm.rs:220`。这里的 error 有时用 message，不能统一误写成 text。

| topic | payload（`?` 可选） | Rust 类型/发出点 | 前端消费 |
| --- | --- | --- | --- |
| `kb:compile` | `runId,kind,text?,docCount?`；kind=phase/tool/page/delta/error/done；计数在 done | `W/wiki/compile.rs:30,41,167,358` | `src/stores/kb.ts:49` |
| `kb:enrich` | `runId,kind,text?,applied?`；phase/tool/delta/error/done | `F/kb/enrich.rs:11,173,205,286` | `src/stores/kb.ts:171` |
| `kb:dedup` | `runId,kind,text?,merged?`；实际 phase/tool/error/done | `F/kb/enrich.rs:305,434,476,531` | `src/stores/kb.ts:172` |
| `echo:dream` | `kind,text?,episodes?`；phase/delta/done/error；无 runId | `F/fable/echo.rs:125,133,469,660,993` | `src/components/SenseApi.vue:363`；`src/components/Automation.vue:98`；`src/components/chat/BriefingCenter.vue:102` |
| `fable:ontology` | phase:`text`；tick:仅 kind；error:`message`；done:`kept,dropped,schemaId,note`；均无 runId | `F/fable/ontology.rs:346,473,481,490,570` | `src/stores/fileTasks.ts:161`；`src/components/OnboardingWizard.vue:391` |
| `file:cluster_llm` | phase:`text`；tick:仅 kind；error:`message`；done:`clusters,assigned,report` | `F/fable/files/llm.rs:139,228,267`；`F/fable/files/commands.rs:345` | `src/stores/fileTasks.ts:139` |
| `file:title_llm` | phase:`text`；tick:仅 kind；error:`message`；done:`count` | `F/fable/files/llm.rs:927,964,1002`；`F/fable/files/commands.rs:366` | `src/stores/fileTasks.ts:152` |

`ontology_extract` 虽返回 run_id，但事件 JSON 不带它，不能擅自描述为可按 runId 路由（`F/fable/ontology.rs:450,473,582`）。
文件中心独立模型分支与 headless fallback 共享这些事件（`F/fable/files/llm.rs:243,262`），Pi 不应接管业务落库或终态统计。

## 安装器与消息桥外围事件

| topic | payload | Rust 发出点 | 前端消费 |
| --- | --- | --- | --- |
| `env:stream` | `reqId,kind,line?,ok?,message?`；kind=log/error/done，ok/message 描述安装结果 | `K/doctor/types.rs:145`；`K/doctor/install.rs:809` | `src/components/EnvDoctor.vue:85` |
| `env:autopilot` | `running,step,finished,ok,installed:string[],message` | `K/doctor/autopilot.rs:41,75` | `src/App.vue:194` |
| `provider://failover` | `kind:"switched",from,fromName,to,toName,message`，其余字段均为字符串 | `K/provider/store.rs:274,292,298` | `src/App.vue:187` 提示 message |
| `feishu://log` | 字符串 | `K/integrations/feishu.rs:584` | `src/tauri.ts:528`；`src/components/FeishuSettings.vue:154` |
| `feishu://status` | 状态字符串 | `K/integrations/feishu.rs:602` | `src/tauri.ts:530`；`src/components/FeishuSettings.vue:150` |

飞书模型调用走 chat_send，而日志/status 是消息桥自己的生命周期（`K/integrations/feishu.rs:589,836,856`）；不得混为同一个 Agent 事件协议。

## 后续契约测试清单与待确认

以下是待实现验证项，本次没有运行测试或声称验证通过。

1. camelCase、None 省略、reqId/conversationId 稳定，以及桌面/WS 两套封装一致（`K/chat/types.rs:106`；runtime `host.rs:29`）。
2. 部分流+整块+result 不重复显示；tool/终态前 flush；错误仍最终释放运行态（`K/chat/pipeline.rs:1577,1665`；`src/stores/chat.ts:516,586`）。
3. 宿主/常驻/Pi 路径产物与历史持久化一致；取消、超时、agentd 崩溃能完成终态（`K/chat/pipeline.rs:1451`；`K/chat/session_pool.rs:776`）。
4. U5（待确认）：准备阶段收到取消时 `K/chat/pipeline.rs:264,695` 直接返回 Ok，外层 `:198` 只处理 Err，不保证发 done；`:263` 注释依赖前端自行收尾。P2 须验证远端/批次调用者是否同样完成收尾。
5. U6（待确认）：桌面把 `[stderr]` 当警告，手机却把全部 error 当终止（`src/stores/chat.ts:575`；`mobile/src/lib/chat.ts:245`）；迁移时需定兼容/修复边界。
6. U7（待确认）：Pi reasoning/tool completion/权限询问的 UI 承载尚无现有事件实现依据（`K/chat/types.rs:108`）；新增契约须同时覆盖前后端，不能把这些内容塞进普通正文冒充兼容。
