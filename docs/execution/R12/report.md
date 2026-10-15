# R12：设备名称纵深防御执行报告

要求的三种构建、Rust 全量测试和 Vitest 均已通过；无任务实现阻塞。本报告供 Claude 重跑、Pi 独立审查；不代表最终验收。未 commit / push。

## 入口与行为

- 公开字符策略：`collab::device_label::validate_characters(&str) -> Result<(), String>`。
- 拒绝 Unicode 控制字符和 `<`、`>`、`"`、`'`、反引号；中文、emoji、空格、`& + / . : @ # _ -` 和中文括号仍可使用。
- 浏览器 `validate_label` 同时保护登录请求与 `browser.deviceName` / `browser.platform` 配置；仍拒绝空白名称和超过 40 个字符的值。
- 设备网 `mesh::{enroll,rename,announce}`、白名单 `identity::{add_device,ensure_device_for}`、两种邀请兑换入口、联邦邀请入伙及 `grants::{update,grant_by_code,auto_grant_self}` 共用字符策略。
- 校验发生在保存、换设备密钥、消耗邀请和已有记录快捷返回之前；非法输入不能修改已存设备名。
- 遥测 HTTP 在截断前校验完整输入，合法输入仍截断至 48 个字符。
- 改名仍对 trim 后名称限 1–40 字；邀请兑换仍限 1–80 字。原本无长度上限的底层登记/心跳/契约接口未增加长度上限；空心跳仍不覆盖名称。
- 前端测试递归扫描 `src/features/interconnect/**/*.vue` 和 `src/features/runtime/**/*.vue` 的原文。
- 唯一白名单是精确路径 `InterconnectView.vue` 内一次 `v-html="allianceAvatar"`，且必须存在 `./alliance-avatar.svg?raw` 静态导入；其他表达式、文件、重复用法和不完整指令均失败。

## 配置、依赖与迁移

- 无新依赖、无 DB schema 或协议迁移；复用现有 Rust 标准库/serde_json、Vitest/Vite 的 raw glob。
- 字符策略是固定安全约束，无关闭开关。各入口保留已有长度参数及默认规则。
- 浏览器默认值仍为 `deviceName: "Browser session"`、`platform: "web"`；配置来源和类型校验沿用 `interconnect::resolve/load`。
- 切换示例：在现有 `POLARIS_INTERCONNECT_CONFIG` 指向的 JSON 内设置 `{"browser":{"deviceName":"书房电脑-01","platform":"网页"}}`；删除覆盖字段恢复默认。
- 配置字段须为字符串；非法字符、空白和超过 40 字会报错；未新增参数。
- 不清洗历史设备名；已有数据库和主机密钥加密镜像恢复内容保持原样。后续新增/更新/重新上报名含禁用字符时须更名。前端继续使用 Vue 插值转义显示历史值。
- `account_store` 的加密镜像恢复已检查：属于历史数据恢复，不是用户输入设备名的在线接口，本次不更改恢复语义。
- `identity.rs` 原有 889 行，按邀请票据、分享码、设备白名单拆为三个子文件；所有原公开路径由 `pub use` 保留，既有测试仍在父文件中。

## 改动文件

以下 Rust 路径均相对 `src-tauri/crates/polaris-collab/src/collab/`：

| 文件 | 行为/职责 |
| --- | --- |
| `device_label.rs`、`mod.rs` | 定义并导出共享字符校验 |
| `device_label/tests.rs` | 配置/Unicode/边界、所有写入口拒绝、存储及邀请无副作用测试 |
| `interconnect/browser/mod.rs` | 登录和配置标签接入共享校验 |
| `mesh.rs` | 入网、改名、心跳名称校验 |
| `identity.rs` | 保留原公开接口，分拆已有实现以满足行数约束 |
| `identity/tickets.rs` | 迁移票据代码，兑换前校验设备名 |
| `identity/devices.rs` | 迁移白名单代码，直接新增和确保设备入口校验 |
| `identity/sharing.rs` | 原分享码代码按职责迁移，行为不变 |
| `authority/sessions.rs` | 联邦邀请入伙前校验，避免忽略后续登记错误绕过校验 |
| `grants.rs`、`grants/automatic.rs` | 契约名称写入与已有契约快捷返回前校验 |
| `http/telemetry_routes.rs` | 完整名称校验先于 48 字截断 |
| `http/browser_tests.rs` | HTTP 登录非法名拒绝/中文名成功，遥测截断与尾部非法字符测试 |

其他文件：

- `tests/frontend/rawHtmlPolicy.test.ts`：递归源文件扫描和扫描器自身 9 项回归用例。
- `docs/feature-tree.md`：互联入口关联本报告的校验、配置、迁移与验证信息。
- 本报告：真实执行结果及边界说明。

## 实际验证

Rust 命令在 `src-tauri/`，前端命令在仓库根执行。日志在 `/tmp/r12-*.log`。

| 命令 | 结果 |
| --- | --- |
| `cargo check -p polaris-app --offline` | 退出 0，0 警告；`/tmp/r12-check-desktop.log` |
| `timeout 60s cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0，0 警告；`/tmp/r12-check-server-net.log` |
| `timeout 60s cargo check -p polaris-cli --bin polaris-server --offline` | 退出 0，0 警告；`/tmp/r12-check-server.log` |
| `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline` | 退出 0；165 通过、0 失败、0 ignored，0 警告；`/tmp/r12-collab-tests-final.log` |
| `npx vitest run` | 退出 0；20 文件、96 测试通过；`/tmp/r12-vitest.log` |
| `npx vitest run tests/frontend/rawHtmlPolicy.test.ts` | 退出 0；1 文件、9 测试通过 |
| `git diff --check` | 退出 0 |

过程记录：

- Rust 初始等待共享包缓存/构建锁；两个服务检查成功。首轮 Rust 全量测试被 `timeout 60s` 以 124 终止，当时刚开始编译；已去掉该超时重新运行完整命令。
- 为排除锁等待曾复制 Cargo registry 到 `/tmp/r12-cargo-home/`，复制成功，未用它构建；原缓存锁随后释放。清理时自动审批拒绝 `rm -rf`（禁止强制删除形式），改用 `rm -r -- /tmp/r12-cargo-home` 成功清理。未删除共享锁或终止其他任务。
- 对新 Rust 文件执行 `rustfmt --edition 2021 --config skip_children=true` 失败，退出 1：当前 stable toolchain 未安装 rustfmt；未安装新组件。
- Python 静态审计通过：17 个改动/新增文件均 ≤500 行、每行 ≤500 字符（最大 500 行/445 字符），新增 Rust 无 `unwrap()` / `expect()`；既有父文件测试及随机码函数中的旧调用未更改。
- `vue-tsc`、Vite build、在线端到端测试未运行（本 brief 未要求）。
- Rust 两项既有网络测试由 `POLARIS_NET_TEST=1` 门控，本次未设置，测试函数提前返回；不将该默认测试结果视为真实网络验证。
