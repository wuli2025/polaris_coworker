# R7：P5b 路径穿越回归执行报告

任务来源：[P5b 第 1 轮处置 #2](../reviews/P5b-round1-triage.md)。

## 改动文件与行为

- `src-tauri/crates/polaris-wiki/src/wiki/compile/tests.rs`：新增
  `page_path_rejects_combined_parent_directory_traversal`，复用临时 wiki 根目录，
  逐个调用 `progress::page_path` 检查以下输入：
  `a/../b`、`概念/../../etc/passwd`、`概念/嵌套/../穿透.md`、`a\..\b`。
  每项必须返回错误且消息以 `WikiWriteOutsideRoot:` 开头，失败时显示具体路径。
  用例不加平台门控，覆盖 Windows 风格分隔符；包括折叠后仍位于根内的穿越输入。
- `docs/feature-tree.md`：补充测试入口、依赖、配置/迁移及验证信息。
- `docs/pi-migration/r7-result.md`：记录本次执行结果与未运行项。

不改生产代码；无新增依赖、配置或迁移。既有配置保持原默认值、类型校验和切换方式，
详见 [P5b 配置与迁移报告](p5b-indirect-callers.md)。
保留执行前已有的未跟踪目录 `docs/execution/R7-1/`，未提交、未推送。
执行期间出现的 `src/features/runtime/`、`src/types/agentRuntime.ts` 未跟踪改动亦未触碰。

## 实际验证

2026-10-14，以下 Cargo 命令工作目录均为 `src-tauri/`。
两壳命令沿用 [P5b 验证清单](p5b-files-and-validation.md) 的特性组合。

| 实际命令 | 真实结果 |
| --- | --- |
| `cargo test -p polaris-wiki --offline` | 退出 0；7 通过、0 失败、0 忽略、0 过滤；文档测试 0 项；0 警告 |
| `cargo check -p polaris-app --offline` | 退出 0；0 警告 |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0；0 警告 |
| `git diff --check`（仓库根目录） | 退出 0，无输出 |
| `python3` 文件行数/行宽断言（仓库根目录） | 退出 0；三个自有文件均 ≤500 物理行，每行 ≤500 字符 |

未运行 Windows/macOS 原生测试、真实模型编译或前端端到端测试；
Windows 风格路径输入已在当前 Linux 环境执行。
实现无剩余项，无阻塞。本报告仅为执行记录，由 Claude 重跑、Pi 独立审查，最终由 Claude 决定模块是否完成。
