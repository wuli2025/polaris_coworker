# R4：P3b 审查意见修复执行报告

2026-10-14。仅实现 C2、C3、#2、#5、#7、#9；驳回项未改代码。
未 commit/push、未新增依赖。保留任务开始时未跟踪的 `docs/execution/R4-1/` 与 `tests/`，未修改其中内容。
本报告仅记录执行结果，由 Claude 重跑、Pi 独立审查，Claude 决定模块是否完成。

## 改动文件与行为

Rust 路径前缀为 `src-tauri/crates/`。

| 文件 | 行为 |
| --- | --- |
| `polaris-sandbox/src/policy.rs` | 删除 forbidden_name 及全树 read_dir；保留 canonicalize/保护路径规范化、双向路径前缀、目录类型校验；创建与路由均显式拒绝 HOME 及祖先；补 path_for_docker 的 Windows 扩展前缀、盘符与分隔符说明 |
| `polaris-sandbox/src/policy_tests.rs` | 普通名称可作为源路径组件和项目内容；新增保护路径本身/上下层、HOME、根目录、实际旧挂载拒绝；0 与 2048 文件项目在不可枚举权限下仍通过创建和路由校验 |
| `polaris-sandbox/src/lifecycle.rs` | 新增假 Docker 端到端测试：普通项目进入 ps/run、精确项目 RW 与 KB RO 挂载、inspect 路由校验；保护路径上下层/HOME/根目录在任何 Docker 调用前拒绝；生产生命周期逻辑不变 |
| `polaris-kernel/src/chat/backend/expert_selection.rs` | 错误包含选择 kind 与 id；保留 EXPERT_LOADER_UNAVAILABLE，实际加载失败用 EXPERT_LOAD_FAILED；不传播可能包含提示词的原始加载器错误；测试两种选择及完整错误链不含提示词 |
| `polaris-kernel/src/chat/backend/pi/extension_tests.rs` | 两轮使用不同 request id、复用同 session；移除 2 秒/5ms 的预约轮询，等待 done 与协议确认；模型、权限、图片及空闲重配置断言保留 |
| `polaris-kernel/tests/fixtures/fake-agentd.mjs` | 写完请求日志并 fsync/close 后才派发请求、发出响应；协议确认可作为读取记录前的确定性信号 |
| `src/features/sandbox/project.ts` | 导出 projectDirectoryHint：POSIX 与 Windows 盘符绝对路径格式提示；最终有效性由后端决定 |
| `src/features/sandbox/components/SandboxProject.vue` | computed 随输入变化更新提示；aria-live 与 aria-describedby/aria-invalid 连接输入；格式提示不绕过后端校验 |
| `src/features/sandbox/project.test.ts` | 格式覆盖相对路径、盘符相对路径、POSIX/双分隔符 Windows 路径，验证后端拒绝传播；修正 beforeEach 返回 mock 被当作清理回调的问题 |
| `docs/feature-tree.md`、本报告 | 入口、依赖、默认值、配置切换、迁移、验证与边界记录 |

C3 的代码证据：`Route::drain` 在发布 done 前移除 session 预约；准备线程的 request id 清理由另一个线程稍后执行。
旧测试两轮都用 request id `wire`，即使 session 已释放，仍可能遇到 AgentdDuplicateRequest。
第二轮现用 `wire-second`，session_id 仍是 `wire`；没有增加 timeout 来掩盖时序问题。
`collect` 等待终态，显式 configure 响应确认空闲重配置，第二轮后 models 响应确认 fixture 已完成之前记录。
没有改动生产调度或协议。

## 配置、默认值与迁移

没有新依赖或生产配置项；公开 Config/ExpertLoader 接口兼容。
`POLARIS_SANDBOX_CONFIG` 未设置时仍为项目 `/workspace` RW、KB `/kb` RO，4 GiB/2 CPU、capAdd=[]、workdirChange="reject"。
`protectedPaths` 默认 []，仅追加到兜底保护路径；必须是绝对路径字符串数组，未知字段与非法类型仍拒绝。
切换示例：保存以下 JSON 为 `/absolute/path/sandbox.json`，设置 `POLARIS_SANDBOX_CONFIG=/absolute/path/sandbox.json`。

```json
{
  "protectedPaths": ["/absolute/private-model-credentials"],
  "workdirChange": "rebuild"
}
```

其余字段由既有 serde 默认值补齐；schema 与运行时默认一致性测试保留。
现在允许普通业务项目中的 data、sessions、*.key 等名称；要保护具体位置，应追加其绝对路径，而不是依赖文件名。
源目录与受保护路径互为前缀均拒绝；源与保护位置经规范化，符号链接别名不能绕过。
HOME 本身及祖先禁止挂载；HOME 下未与保护路径重叠的普通项目仍可挂载。
校验工作量取决于挂载/保护路径数量与路径深度，不取决于项目文件数；没有目录树枚举。
历史 P3b 的“敏感名字扫描”描述由本报告取代。
旧宽挂载仍需显式 `sandbox_start({projectDir:"/absolute/project",rebuild:true})` 停止并改名保留后重建，未修改迁移策略。

专家字段默认不携带；示例 `expertSelection={kind:"expert",id:"qa"}`，团队为 `{kind:"team",id:"review"}`。
空 id、未知 kind/类型仍失败；错误仅公开选择元数据，未向用户暴露模板内容。
前端目录提示默认空路径要求选择；支持 `/home/me/project`、`C:/Projects/app` 及 Windows 反斜杠路径。
提示只验证格式，不能判断目标平台、目录存在或保护规则；例如 `/` 格式有效，但后端仍拒绝挂载。

## 实际命令与结果

除明确说明外，Cargo cwd=`src-tauri`，前端 cwd=仓库根；均为本机实际执行。

| 命令 | 真实结果 | 日志 |
| --- | --- | --- |
| `cargo check -p polaris-app --offline` | 退出 0；既有 kernel 6 项 warning | `/tmp/r4-check-app.log` |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0；既有 warning | `/tmp/r4-check-server.log` |
| `cargo test -p polaris-sandbox --offline` | 17 通过，doc-tests 0，退出 0 | `/tmp/r4-sandbox.log` |
| 根目录 `cargo test -p polaris-sandbox --offline --manifest-path src-tauri/Cargo.toml` | 加强实际挂载错误码断言后复跑，17 通过，退出 0 | `/tmp/r4-sandbox-final.log` |
| `cargo test -p polaris-kernel --lib --tests --offline` | 171 通过、10 失败、2 忽略，退出 101；lib 失败使集成测试未继续执行 | `/tmp/r4-kernel-full.log` |
| `cargo test -p polaris-kernel --lib --offline` 连续 10 次 | 目标 C3 测试每次通过；每次整体均 171 通过、10 项相同例外失败、2 忽略，退出 101 | `/tmp/r4-kernel-lib-01.log` 至 `-10.log` |
| kernel lib/tests 命令追加下列十个完整名称的 `--skip` | 171 单测 + 4 集成通过，2 忽略、10 排除，退出 0 | `/tmp/r4-kernel-filtered.log` |
| `npx vue-tsc --noEmit` | 退出 0 | `/tmp/r4-vue-tsc.log` |
| `npx vitest run` | 最终 8 文件、25 测试通过，退出 0 | `/tmp/r4-vitest-final.log` |
| `git diff --check`；修改文件物理行数/行长、新增 Rust unwrap/expect 扫描 | 通过；每个本次自有文件 ≤500 行、每行 ≤500 字符；无新增 unwrap()/expect() | 终端检查 |

首次 Vitest 为 24 通过、1 失败（`/tmp/r4-vitest.log`）：既有 beforeEach 箭头表达式返回 mock，被 Vitest 作为 cleanup 调用。
新增 mock 拒绝场景暴露该问题；改为不返回值的语句块后全量 25 项通过。

连续全量 lib 结果（不使用 skip，不限单个测试，不设置 test-threads）：

| 次数 | C3 目标 | 整体通过/失败/忽略 | 退出码 |
| --- | --- | --- | --- |
| 1 | 通过 | 171 / 10 / 2 | 101 |
| 2 | 通过 | 171 / 10 / 2 | 101 |
| 3 | 通过 | 171 / 10 / 2 | 101 |
| 4 | 通过 | 171 / 10 / 2 | 101 |
| 5 | 通过 | 171 / 10 / 2 | 101 |
| 6 | 通过 | 171 / 10 / 2 | 101 |
| 7 | 通过 | 171 / 10 / 2 | 101 |
| 8 | 通过 | 171 / 10 / 2 | 101 |
| 9 | 通过 | 171 / 10 / 2 | 101 |
| 10 | 通过 | 171 / 10 / 2 | 101 |

目标完整名：`chat::backend::pi::extension_tests::model_profile_image_and_idle_reconfiguration_are_on_the_wire`。
按当前任务 brief 的“至少 10 次”执行；未把分流文档的 20 次描述宣称为已执行。

四项既有基线失败：

```text
doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix
doctor::path::tests::prime_and_prepend_behaviour
doctor::verify::tests::install_root_groups_npm_entries_together
doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts
```

六项本环境 TCP 限制失败（EPERM 或本地端口无法绑定）：

```text
integrations::codex_proxy::tests::e2e_anthropic_passthrough_over_real_tcp
integrations::codex_proxy::tests::e2e_openai_route_over_real_tcp
provider::claude_login::claude_oauth_tests::loopback_run_end_to_end_local
provider::claude_login::claude_oauth_tests::loopback_run_rejects_wrong_state
provider::store::local_route_tests::openai_provider_routes_via_local_router
provider::store::local_route_tests::route_all_switch_reroutes_anthropic_provider
```

过滤命令为 `cargo test -p polaris-kernel --lib --tests --offline --` 后逐项追加 `--skip <上述完整名称>`。
未扩大例外范围，也未把整体退出 101 声称为全量通过。

## 未完成、阻塞与未运行验证

本轮指定修复没有新增范围阻塞；四项基线及六项 TCP 测试仍按任务许可例外列出。
真实 Docker/Windows 实机、浏览器点击、真实模型和 agentd 自身测试未运行；本轮挂载端到端为假 Docker 命令边界测试。
禁止目录枚举回归为 Unix 专用，使用非特权用户（本次 uid=1000）与 0111 权限；root 绕过 DAC 时该测试明确失败，不能用其结果声称已验证禁止枚举。
未运行 rustfmt；保持既有文件风格。没有提交或推送。
既有服务端未装配 SandboxPolicy 的边界不属本轮修复；未改两壳接线，也不把 cargo check 通过解释为服务端沙箱功能验收。
最终验收由 Claude / Pi 决定。
