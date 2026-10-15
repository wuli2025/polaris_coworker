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
# 任务 P7b：演练发现的产品缺口

来源：`docs/drills/P7a-blockers.md`"确认的产品接口限制"。

## 0. 回归：agentd 生产打包失败（阻塞演练，最先修）
- 现象：`node scripts/pack-agentd.mjs <out>` 在最后的契约自检步骤失败。根因：F1 让 `agentd/test/sdk-contract.test.ts` 引入了 `./synchronization.ts`，但 `scripts/pack-agentd.mjs:20` 只复制 `test/sdk-contract.test.ts`、`:46` 只复制编译后的 `sdk-contract.test.js`，辅助文件缺失导致导入失败。
- 要求：打包时一并带上契约测试依赖的辅助模块（按导入关系收集，不要写死文件名列表），或让契约自检脚本自包含；自检通过后仍删除测试目录、不把测试文件留在产物里。
- 加回归保护：`scripts/` 下增加针对打包脚本的测试（或把一次最小打包纳入 agentd 测试），确保以后测试辅助文件变化不会悄悄打坏打包。
- 允许修改：`scripts/pack-agentd.mjs`、`scripts/runtime/`、`agentd/test/`。

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
- 允许修改：`scripts/pack-agentd.mjs`、`scripts/runtime/`、`agentd/test/`、`src-tauri/src/apihub/`、`src-tauri/src/` 中命令注册、`src-tauri/crates/polaris-kernel/src/chat/`、`src-tauri/crates/polaris-collab/`、`src/components/chat/`（按上述净增限制）与新文件、`src/stores/chat.ts`（既有超长，净增 ≤10 行）、`drills/`、测试、`docs/`。
- 不新增依赖（若 ureq 现有特性无法注入 CA，先在结果中说明并给出不新增依赖的方案或理由）；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；三种构建 0 警告。
- 必须通过：三种 `cargo check --offline`；`cargo test -p polaris-kernel -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`；`npx vue-tsc --noEmit`；`npx vitest run`。不 commit。

--- END TASK DATA ---
