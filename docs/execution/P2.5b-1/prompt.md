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
# 任务 P2.5b：聊天入口接通图片附件与思考档位

P2.5 已完成协议与后端（见 `docs/pi-migration/p25-result.md`），阻塞在入口：`ChatSendArgs`（`chat/types.rs`）没有结构化图片与思考档位字段。范围例外已批准。

## 现状
对话附件经 `chat_attach_files` / `chat_attach_image`（`chat/attach.rs`）复制进"会话 uploads 目录"，前端把绝对路径写进 prompt，旧后端的 claude 用 Read 工具读图。

## 要求
1. `ChatSendArgs` 增加可选字段 `image_paths: Option<Vec<String>>`、`thinking_level: Option<String>`（serde 默认值，旧前端不传也能用）。
2. 后端读取 `image_paths`：**只允许**位于该会话 uploads 目录内的文件（规范化后判断，拒绝 `..`、符号链接逃逸、目录外路径）；按 P2.5 的附件上限与 MIME 白名单读取并 base64 编码，经 `TurnRequest` 下发。越界/超限/类型不符返回具名错误，不静默丢弃。`thinking_level` 校验后下发，非法值具名错误。
3. 兼容：路径仍保留在 prompt 文本中（旧后端照常工作）；Pi 后端下同时以结构化图片发送。核查 Pi 内置 `read` 工具能否读取图片（看 d.ts/文档），把结论写进结果——这决定"只有路径没有结构化图片"时 Pi 能否兜底。
4. 前端最小增量：发送消息时把已上传图片附件的路径填入 `image_paths`；如前端已有思考档位选择则一并传，没有就不加 UI。只改 `src/stores/chat.ts` 与 `src/tauri.ts` 的类型/调用处，不改样式与组件结构。
5. 测试：Rust 端路径校验（目录内/外、`..`、符号链接、超限、类型）、结构化图片确实进入 `TurnRequest`；`npx vue-tsc --noEmit` 通过。

## 规则
- 允许修改：`polaris-kernel/src/chat/{types.rs,attach.rs,mod.rs}`、`polaris-kernel/src/chat/backend/`、上述两个前端文件、测试、`docs/`。
- 无新依赖；≤500 行、≤500 字符；新 Rust 代码无 unwrap/expect；上限进配置。
- 必须通过：两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`（基线与沙箱 TCP 项除外）；`npx vue-tsc --noEmit`。不 commit。

--- END TASK DATA ---
