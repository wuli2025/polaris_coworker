# 任务 P5b：间接调用方迁移（wiki 编译、无头调用命名、远程执行名单）

先读：`docs/pi-migration/call-sites.md`"间接 Agent 调用与宿主入口"一节；`polaris-wiki/src/wiki/compile.rs`；`polaris-kernel/src/headless.rs`（P2 已改走 AgentBackend）；`chat/backend/`（TurnRequest、toolProfile、权限档位、沙箱选项）；`src-tauri/src/exec.rs`。

## 现状（主控已核实）
- fable 的知识库补链/去重（`kb/enrich.rs`）、回声记忆（`fable/echo.rs`）、本体抽取（`fable/ontology.rs`）、检索扩写（`fable/retrieve/ai.rs`）、文件归类与画像（`fable/files/llm.rs`、`profile.rs`）都经 `headless::run_claude_readonly*`，P2 起已走新后端——**只剩命名遗留**。
- **wiki 编译 `compile.rs:120-224` 仍自己 `Command::new("claude")`**，以写权限运行、自行解析 stream-json、隔离模式下调用 `scope_child_claude`。
- `exec.rs:135` 远程执行允许名单写死 `"claude", "codex"`。

## 要求
1. **wiki 编译走 AgentBackend**：
   - 用会话级工作目录 = wiki 根目录、写权限仅限该目录（权限档位 auto-current，工具档位按需配置，档位名进配置），沙箱设置沿用用户选择。
   - 进度：把后端事件（tool.started/completed、text.delta）翻译成现有 `kb:compile` 进度事件，前端不改；写页面的工具调用要能识别为"写入了哪个页面"（按 Pi 的 write/edit 工具参数中的路径，限 wiki 目录内）。
   - 保留现有维护锁、写入约束与失败收尾语义；超时/取消走后端的 abort。
   - 旧 claude 路径仅在 `agent.backend=claude-cli` 时保留（守卫），不在 Pi 后端下启动 claude。
   - 拆分 `compile.rs`（如触及职责超过 500 行）。
   - 测试：用假后端（复用 P2 的假 agentd 或 AgentBackend 测试替身）验证进度翻译、目录外写入被拒、取消与超时收尾、旧后端守卫。
2. **命名**：`run_claude_readonly` / `run_claude_readonly_timeout` 更名为后端中立的名字（例如 `run_agent_readonly*`），旧名保留为 `#[deprecated]` 转发一个版本；fable 内调用点改用新名；只改名与注释，不改行为。
3. **远程执行名单**：`exec.rs` 的允许程序名单进配置（默认值与现状一致以免破坏既有用户，但从代码常量中移出；Pi 后端下默认不再需要 `claude`，在配置注释说明）；测试两组配置。

## 规则
- 允许修改：`src-tauri/crates/polaris-wiki/`、`src-tauri/crates/polaris-kernel/src/headless.rs`、`src-tauri/crates/polaris-kernel/src/chat/backend/`（仅为 wiki 所需的公开接口）、`src-tauri/crates/polaris-fable/`（仅改名调用点）、`src-tauri/src/exec.rs` 及其配置装配、测试、`docs/`。前端不改。
- 无新依赖；每文件 ≤500 行、每行 ≤500 字符（既有超长文件只拆本次触及的职责，未触及部分列入技术债）；新 Rust 代码无 unwrap/expect。
- 编译警告不得高于当前（两壳均为 0）。
- 必须通过：两壳 `cargo check --offline`（警告 0）；`cargo test -p polaris-kernel -p polaris-wiki -p polaris-fable --lib --tests --offline`（沙箱 TCP 项除外并列出）；`npx vue-tsc --noEmit`。不 commit。
- 结束时输出：wiki 编译新旧行为差异、改动文件与行数、命令结果。

## 附：fable 的 2 个既有失败测试
`fable::inventory::tests::rel_of_is_root_relative_slash_path`、`kb::access::tests::path_contains_handles_verbatim_prefix_and_case` 在重构前即失败（Windows 路径断言，见 `docs/pi-migration/baseline.md` 补充基线）。改为跨平台（按平台构造期望或 `cfg`），不改被测逻辑的 Windows 行为；允许修改对应两个测试所在文件的测试部分。修复后 fable 应 0 失败。
