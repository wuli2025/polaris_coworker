# R-release-v3 执行报告

状态：代码改动已落地；发布验收 BLOCKED（沙箱端口/Docker 权限及基线 E2E 失败）。本报告不是模块验收结论，由 Claude 重跑、Pi 审查后决定。

## 行为、入口与依赖

- 默认后端继续为 Pi；Windows/macOS 使用 fetch-runtimes → pack-agentd，Docker 使用 agent 编译阶段。没有新增依赖。
- MiniMax 默认值集中在 `provider/store/bundled.rs`：id=minimax，端点=https://api.minimaxi.com/anthropic，模型=MiniMax-M3；Pi 翻译为 anthropic-messages。
- `option_env!("POLARIS_BUNDLED_MINIMAX_KEY")` 在 Rust 编译时读取 key；未提供或仅空白时禁用内置凭据，保留开发环境原有初始化行为。
- init 在 current_id 为空或已经是 minimax 时播种无密钥元数据并持久化；非空的其他选择，包括 claude-official，保持不变。已有 minimax 设置不覆盖。
- selection/views 的空选择回落到 minimax；这不把已有明确的 claude-official 选择迁移走。
- key 通过 Rust 启动 agentd 的私有环境注入；agentd 启动即从环境删除，避免继承给后续工具子进程。FileCredentials 读取用户凭据优先，缺失时用内存内置凭据，不把内置 key 写入 providers.json。
- 列表视图不装载内置 key，不输出尾号，使用 hasKey + note="内置"；导出 store/config 不含内置 key。daemon 注册原值及编码变体到 Redactor，日志和协议输出统一脱敏。
- Codex 未显式配置模型时默认 gpt-6-astra；用户显式 models/env 模型优先。仓库源码未找到 gpt-5.3-codex-spark 字面量，问题来自 SDK 目录顺序，故在翻译边界固定默认。
- 模型错误保留 errorMessage 中的供应商原因，移除认证头、Bearer、常见 key 模式，随后由协议输出层移除已登记凭据；错误分类仍依据结构化 diagnostics。
- Docker 删除整个可选 Claude Code 下载/安装段。doctor 的 legacy 安装、更新、探测入口关闭，报告只走 Pi；旧 ClaudeCli 实现仍保留，可显式选择，但不分发 Claude Code。
- MinGit 保留：`doctor/pi_shell.rs` 的 resolve/configure 直接使用 `bundled::git_bash()`（原文件约 104、136 行），是 Pi Windows shell 的依赖，不仅服务 Claude CLI。fetch-runtimes 注释已更新。
- 版本更新为 3.0.0：应用、Tauri、Cargo 主包和 lock、agentd 和 npm lock；其他内部 crate 原为 0.1.0，保持原版本。第三方 tinybench/其他依赖的 2.9.0 不改。
- 默认中继配置未修改；mobile/ 未修改；没有 commit/push。保留任务开始时已有未跟踪的 docs/execution/R-release-v3/、docs/tasks/R-release-v3.md、src-tauri/runtime。

## 配置与切换示例

- 发布 CI：Windows/macOS 构建步骤使用 `POLARIS_BUNDLED_MINIMAX_KEY: ${{ secrets.MINIMAX_API_KEY }}`；两个工作流在 Tauri 构建前对 agentd 执行 npm ci，并将 Node 固定到 agentd engines 要求的 22.19.0。
- Docker：build-push-action 的 secret-envs 将同一环境变量传给 BuildKit；Rust 编译 RUN 使用 secret mount 的 env 注入。key 不作为普通 ARG/ENV、文件或 RUN 字面量保存；最终阶段只复制二进制。
- 本地开发：不设该变量即可编译；值类型是环境字符串，trim 后为空即禁用。FileCredentials 的显式 bundled 参数具有 TypeScript 类型约束并拒绝空 id/key。
- 本地发布示例（key 已由外部安全环境提供，不写入文件）：`cargo build --release -p polaris-app`（src-tauri 内）。
- Docker 示例：`docker build --secret id=POLARIS_BUNDLED_MINIMAX_KEY,env=POLARIS_BUNDLED_MINIMAX_KEY --build-arg POLARIS_BUILD_VERSION=3.0.0 .`。
- 用户切换：供应商坞选择其他 provider；或编辑 MiniMax 填写自己的 key。用户 key 优先；删除用户凭据后恢复内置回落。
- 没有拿到或使用真实发布 key；CI secret 是否已设置及上游模型实际授权需发布环境验证。

## 改动文件

| 文件 | 改动 |
| --- | --- |
| `.github/workflows/release.yml` | 注入编译 secret；agentd npm ci |
| `.github/workflows/mac-build.yml` | 同上 |
| `.github/workflows/docker-image.yml` | secret 环境及 BuildKit secret-envs |
| `Dockerfile` | secret mount 编译；移除 Claude Code 分发 |
| `package.json`、`package-lock.json` | 应用 3.0.0 |
| `agentd/package.json`、`agentd/package-lock.json` | agentd 3.0.0 |
| `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` | polaris-app 3.0.0 |
| `src-tauri/tauri.conf.json` | Tauri 3.0.0 |
| `scripts/fetch-runtimes.mjs` | 保留 Git Bash 并说明 Pi 用途 |
| `src/features/runtime/copy.ts` | Pi 随包、内置凭据及用户优先说明 |
| `src-tauri/crates/polaris-kernel/build.rs` | 更新旧 gift 兼容常量的说明；实际 key 不生成到源文件 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/process.rs` | 子进程内置凭据环境注入 |
| `src-tauri/crates/polaris-kernel/src/doctor/pi.rs` | 禁用 legacy 安装/探测；内置凭据就绪识别 |
| `src-tauri/crates/polaris-kernel/src/provider/store.rs` | bundled 模块公开入口 |
| `src-tauri/crates/polaris-kernel/src/provider/store/bundled.rs` | 默认常量、编译 key、播种、列表标记及行为测试 |
| `src-tauri/crates/polaris-kernel/src/provider/store/persistence.rs` | init 调用无 key 播种 |
| `src-tauri/crates/polaris-kernel/src/provider/store/gifts.rs` | 移除旧 MiniMax 明文落盘播种代码 |
| `src-tauri/crates/polaris-kernel/src/provider/store/presets.rs` | 引用集中默认值 |
| `src-tauri/crates/polaris-kernel/src/provider/store/views.rs` | 内置标记、空选择回落 |
| `src-tauri/crates/polaris-kernel/src/provider/selection.rs` | 空选择回落 |
| `src-tauri/crates/polaris-kernel/src/provider/pi_translate.rs` | Codex 默认模型 |
| `src-tauri/crates/polaris-kernel/src/provider/pi_translate_tests.rs` | Codex 默认与用户模型优先测试 |
| `agentd/src/main.ts` | 读取/删除凭据环境，注册脱敏，注入内存回落 |
| `agentd/src/models/file-credentials.ts` | 用户优先的只读内置回落；列表无 key |
| `agentd/src/events/model-error.ts`、`mapper.ts` | 保留脱敏原因 |
| `agentd/test/bundled-provider.test.ts` | SDK 认证、无落盘、列表/日志脱敏、用户优先、原因展示 |
| `agentd/test/model-errors.test.ts` | 更新分类稳定、原因可见及认证头脱敏断言 |
| `docs/feature-tree.md` | 模块入口链接（保持 500 行） |
| `docs/tasks/R-release-v3-report.md` | 本报告 |

## 实际验证

日志目录：`/tmp/polaris-r-release-v3/`（本机临时目录，不提交）。

| 实际命令 | 结果 |
| --- | --- |
| `cargo check --offline -p polaris-app`（src-tauri） | 最终通过。首轮新增 presets 导入遗漏已修复；第二轮缺 dist，真实构建前端后重跑通过。见 cargo-check-final.log |
| `cargo test --offline -p polaris-kernel --lib provider`（src-tauri） | 41 通过、4 失败。新增 MiniMax/Codex 测试通过；失败是两个 OAuth loopback 的 EPERM、两个本地路由无法绑定端口。见 cargo-provider-final.log |
| `npx vue-tsc --noEmit` | 通过，vue-tsc.log 无错误 |
| `npx vitest run` | 默认配置加载器写只读 node_modules/.vite-temp 失败（EROFS） |
| `npx vitest run --configLoader runner` | 24 文件、118 测试全部通过，含 agentd 生产包构建/安装验证。首次重跑暴露新增测试错误 import，已修复后全量通过 |
| `npx vite build --configLoader runner` | 通过，真实生成 dist；有已有大 chunk 提示 |
| agentd `npm run typecheck && npm test` | 类型检查通过；全量 33 个测试文件中 30 通过、3 失败，见 agentd-test-final.log |
| agentd `node --import tsx test/bundled-provider.test.ts` | 新增内置凭据/日志/错误脱敏行为测试通过 |
| agentd `node --import tsx test/network.test.ts` | 1 通过、1 失败，listen EPERM 127.0.0.1 |
| agentd `node --import tsx test/web.test.ts` | 本地 HTTP 测试 listen EPERM；其他不监听端口的测试通过 |
| agentd `node --import tsx test/e2e.test.ts` | daemon 在 ping 前 exit=0，stderr 为空 |
| `git archive HEAD agentd` 解出临时基线，并用同一依赖执行上述 E2E | 同样提前 exit=0，证明在未改源码基线上可复现；根因未在本模块修复 |
| `docker info` | Docker socket permission denied，无法构建/检查 history |
| `node --check scripts/fetch-runtimes.mjs` | 通过 |
| `git diff --check` | 通过 |
| Python 文件长度审计 | 新增和改动源文件均 ≤500 行、每行 ≤500 字符；仅历史 lock 文件超行数且只改版本元数据 |

辅助命令/失败说明：运行过 rg、git status/diff/show、cat/sed/tail、wc、df；初次执行辅助脚本使用 python（系统不存在，改用 python3）。曾在仓库根误跑 npm run typecheck 和 cargo check（分别缺脚本/Cargo.toml），已切到正确目录重跑；一次辅助编辑用错相对路径后已用绝对路径修正。尝试 `node --test --test-isolation=none` 时本机 Node 22.19 不支持该选项，改为直接运行对应测试文件获取详情。

## 未完成项与 Claude 重跑要求

- BLOCKED：在允许本地监听端口的环境重跑全部 Rust provider 与 agentd 测试；E2E 基线提前退出另需定位。本次没有把这些失败算作通过。
- BLOCKED：Windows/macOS 实际安装包构建、干净机器首次启动及升级已有用户配置验证未运行（当前 Linux 沙箱）。
- BLOCKED：Docker 构建和 `docker history --no-trunc` 的实际密钥残留核验未运行（无 socket 权限）。配置使用 BuildKit secret，但不以静态检查代替产物验收。
- 未运行真实 MiniMax/Codex 计费请求、真实 secret 发布及上游授权验证；无真实密钥输入，也未执行 CI 发布。
- 没有新增依赖、修改中继或 mobile；没有提交或推送。历史 ClaudeCli 源码和不可达的旧 UI 分支保留；doctor 统一返回 Pi 报告，不再触发该 UI 的安装提示。
