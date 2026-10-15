# R8 执行报告：部分实现，BLOCKED

> 本文保留第一次执行记录。范围例外批复后的实现和最新验证见 [第二次执行报告](r8-review-fixes-run2.md)。

日期：2026-10-15。先读取 `docs/reviews/P5cd-round1-triage.md`，仅处理采纳项和指定补测试项。
未 commit / push，未添加依赖，未修改驳回项的生产逻辑。保留工作区中 collab、mesh 和 docs/execution 等并行用户改动。

## 改动文件和行为

| 文件 | 行为 |
| --- | --- |
| `src/features/runtime/components/ContextFileEditor.vue` | 项目下拉改为受控选择；change 时先恢复当前选择，再调用确认切换接口，取消不丢失文本 |
| `src/features/runtime/useContextFileEditor.ts` | 提取编辑状态，提供 open/save/switchProject；确认后自动读取目标项目，忙时拒绝并发切换，读取失败清空旧保存目标 |
| `tests/frontend/runtimeContextEditor.test.ts` | 4 个行为用例：取消及保存原目标、确认一次及新目标保存、干净切换及重复选择、忙时拒绝及读取失败 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/experts.rs` | prompt 可配置字节上限与具名错误；原始长度校验后规范化尾部空白，比较前统一规范化 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/experts_tests.rs` | 移入原有 3 个测试，新增 5 个配置/字节边界/持久读取/等价配置不 apply/daemon 超限不落盘测试 |
| `src-tauri/crates/polaris-kernel/src/provider/usage_costs.rs` | 仅新增测试：同一模型混合请求的三种顺序均未知，重新查询仅有价窗口时恢复完整价格标记 |
| `docs/feature-tree.md` | 追加入口、依赖、配置、迁移、验证结果和范围阻塞 |
| `docs/pi-migration/r8-review-fixes.md` | 本报告 |

## 配置与兼容

- 环境变量 `POLARIS_EXPERT_PROMPT_MAX_BYTES` 默认 32768 字节；正 usize，零、负数、非整数、溢出及非 UTF-8 环境值报 `ExpertPromptLimitInvalid`。
- 示例：`POLARIS_EXPERT_PROMPT_MAX_BYTES=65536 ./polaris-server`；`unset POLARIS_EXPERT_PROMPT_MAX_BYTES` 恢复默认。环境值在专家配置操作边界读取。
- 持久配置及 daemon 实际 prompt 均检查原始 UTF-8 字节数，超过上限报 `ExpertPromptTooLong`，错误包含实际字节数与上限。多字节文本按字节计数；空白不能绕过长度校验。
- 比较和保存采用去除尾部空白后的配置；保留开头空白、内部换行。已有超限配置需缩短正文或提高显式上限；JSON 格式不变。
- 费用聚合生产代码没有修改。

## 实际执行命令及结果

Rust 命令工作目录为 `src-tauri/`，前端命令工作目录为仓库根。

| 命令 | 实际结果 | 日志 |
| --- | --- | --- |
| `npx vitest run tests/frontend/runtimeContextEditor.test.ts` | 退出 0；4 测试通过 | 工具输出 |
| `cargo test -p polaris-kernel --lib --offline chat::backend::pi::experts` | 退出 0；当时 6 测试通过；后新增 2 个 daemon 行为测试由后续全量运行覆盖 | 工具输出 |
| `npx vue-tsc --noEmit` | 退出 0，无输出 | `/tmp/r8-vue-tsc.log` |
| `npx vitest run` | 退出 0；15 文件、65 测试通过 | `/tmp/r8-vitest.log` |
| `cargo check --offline` | 退出 0；桌面壳零警告 | `/tmp/r8-desktop-check.log` |
| `cargo check -p polaris-cli --bin polaris-server --offline` | 退出 0；4 条范围外 mesh 警告，不能算零警告通过 | `/tmp/r8-server-check.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 101；沿用 P5d 服务壳组合，当前工作区 mesh/orchestration 编译错误 | `/tmp/r8-server-collab-check.log` |
| `cargo test -p polaris-kernel --lib --tests --offline` | 执行两次，均退出 101；最终 213 通过、6 失败、2 ignored，全部 8 个专家测试及 3 个费用测试通过 | `/tmp/r8-kernel-tests.log`（最后一次） |
| `cargo test -p polaris-kernel --offline --test backend_config --test provider_switch --test catalog_protocol --test credential_lock_interop` | 退出 0；8 个集成测试通过，补足全量命令因 lib 失败未执行的部分 | `/tmp/r8-kernel-integration.log` |
| `git diff --check -- src/features/runtime src-tauri/crates/polaris-kernel/src/chat/backend/pi/experts.rs src-tauri/crates/polaris-kernel/src/provider/usage_costs.rs` | 退出 0 | 工具输出 |

内核失败名及原因：

- `integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp`：bind PermissionDenied / Operation not permitted。
- `integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp`：同上。
- `provider::claude_login::tests::loopback_run_end_to_end_local`：PermissionDenied / Operation not permitted。
- `provider::claude_login::tests::loopback_run_rejects_wrong_state`：同上。
- `provider::store::tests::local_route_tests::openai_provider_routes_via_local_router`：无法绑定 8765–8789 本地端口。
- `provider::store::tests::local_route_tests::route_all_switch_reroutes_anthropic_provider`：同上。

服务壳失败：`src-tauri/src/mesh/orchestration.rs:37` 引用 `crate::hosting::bus_emit`，但 `src/lib.rs:8` 将 hosting 限定为 desktop。文件属于范围外并行工作，未修改。
不带 collab-net 的 4 条警告来自 `src/mesh.rs`：473/1386 不可达语句、329/1348 未使用变量；未通过屏蔽警告或越界修改来宣称成功。

## 未完成与阻塞

- 后端 #4 未实现。现有 `PiBackend::new` 仅创建惰性适配器，`sync_experts`/`expert_state` 在首轮或读取时恢复；周期 ping 在 `pi/supervisor.rs`。
- 实现不可达时允许初始化、同步状态及恢复后自动重试，需要在 `pi/mod.rs` 和 `pi/supervisor.rs` 接入行为。brief 允许该目录的 experts*.rs 和状态类型，没有授权这两个文件的行为修改。
- 已通过问题工具请求这两个文件的最小修改授权，报告时未收到答复。未擅自扩展范围；未添加没有实际同步链路支撑的状态字段。
- #4 的“不可达→后端可用且未同步→恢复同步”测试未编写、未运行。浏览器手工/E2E、agentd 全量测试未运行（本次没有 agentd 改动）。
- 必须通过的 kernel 全量和零警告双壳检查未满足，不能宣称 R8 验收成功。由 Claude 重跑、Pi 独立审查并决定完成状态。
