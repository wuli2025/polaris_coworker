# 任务 P7b：演练发现的产品缺口

来源：`docs/drills/P7a-blockers.md`"确认的产品接口限制"。

## 1. 运行中对话的插话（steer）与追加（followUp）入口
- 现状：agentd 协议与内核 Pi 后端都支持 steer/followUp，但服务端分发（`src-tauri/src/apihub/`、Tauri 命令）没有入口，前端无法在 AI 回答过程中插话或排队追加。
- 要求：新增 `chat_steer(reqId|conversationId, text)` 与 `chat_follow_up(...)`（Tauri 与服务端 HTTP 两条路径），定义"会话/请求 → agentd session"绑定；运行中才接受，空闲时返回具名错误（或按 followUp 语义转为新一轮，二选一并写明）；与 busy/done 的竞态有明确行为；遵守现有权限（只能操作自己的会话）。
- 前端：对话输入框在 AI 回答中时提供"插话"（steer）与"排队"（followUp）两种发送方式（新逻辑放新文件，`ChatComposer.vue` 净增 ≤15 行）。
- 测试：运行中插话被同一轮吸收、追加在本轮结束后执行、空闲时行为、他人会话被拒；演练脚本 `drills/d1.mjs` 的 steer/followup 步骤改为真实断言（不再 blocked）。

## 2. 自定义 CA 信任（私有部署）
- 现状：`polaris-collab` 的权威客户端（`authority/trust.rs`、`interconnect/browser/authority.rs`）用默认 `ureq`（webpki roots），无法信任私有 CA，NAS/内网自建权威无法使用。
- 要求：互联配置增加 `authority.extraCaPem`（文件路径或 PEM 内容，可选），构造 ureq Agent 时把该 CA 加入信任根（仅追加，不禁用校验）；无配置时行为不变；证书错误给出明确诊断。所有访问权威/relay 配置下发的 HTTP 客户端统一使用该构造函数。
- 测试：用测试 CA 签发的本地 HTTPS 服务，配置 CA 后可连、未配置时拒绝；不出现任何"跳过校验"路径。演练脚本 D4 改用该配置走真实 TLS。

## 规则
- 允许修改：`src-tauri/src/apihub/`、`src-tauri/src/` 中命令注册、`src-tauri/crates/polaris-kernel/src/chat/`、`src-tauri/crates/polaris-collab/`、`src/components/chat/`（按上述净增限制）与新文件、`src/stores/chat.ts`（既有超长，净增 ≤10 行）、`drills/`、测试、`docs/`。
- 不新增依赖（若 ureq 现有特性无法注入 CA，先在结果中说明并给出不新增依赖的方案或理由）；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；三种构建 0 警告。
- 必须通过：三种 `cargo check --offline`；`cargo test -p polaris-kernel -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`；`npx vue-tsc --noEmit`；`npx vitest run`。不 commit。
