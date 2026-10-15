# 任务 R12：设备名称纵深防御（P6d 审查推断风险 #1、#2）
见 `docs/reviews/P6d-round1-triage.md`。
- 后端：`polaris-collab` 浏览器会话设备的 `validate_label`（以及其他接受用户输入设备名的入口，一并检查）在现有控制字符过滤基础上拒绝 `<`、`>`、`"`、`'`、反引号；保留中文与常见符号；超长截断/拒绝规则不变；测试覆盖特殊字符被拒、中文名通过。
- 前端：新增 vitest 回归测试，扫描 `src/features/interconnect/` 与 `src/features/runtime/` 的 `.vue` 文件，除白名单（`InterconnectView.vue` 中渲染静态头像的 `allianceAvatar`）外禁止出现 `v-html`。
允许修改：`src-tauri/crates/polaris-collab/`、`tests/frontend/` 或 `src/features/interconnect/` 下测试文件、`docs/`。无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；三种构建 0 警告。
必须通过：三种 `cargo check --offline`；`cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests --offline`；`npx vitest run`。不 commit。
