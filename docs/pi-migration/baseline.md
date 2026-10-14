# 重构前基线（2026-10-14，提交 b6d9e38，WSL2 Ubuntu 24.04）

重构过程中，下面这些"已知状态"不算回归；新出现的失败才算。

| 检查 | 命令（在仓库根或 `src-tauri/`） | 结果 |
| --- | --- | --- |
| 前端依赖 | `npm ci --ignore-scripts` | 通过 |
| 前端类型 | `npx vue-tsc --noEmit` | 通过，0 错误 |
| 前端构建 | `npx vite build` | 通过（生成 `dist/`，桌面 crate 编译依赖它） |
| 桌面壳编译 | `cargo check -p polaris-app` | 通过，7 警告 |
| 服务端壳编译 | `cargo check -p polaris-cli --bin polaris-server --features collab-net` | 通过，7 警告 |
| 协作测试 | `cargo test -p polaris-collab --features polaris-collab/collab-host,polaris-collab/collab-net --lib --tests` | 112 通过 |
| 内核测试 | `cargo test -p polaris-kernel --lib --tests` | 118 通过，**4 失败**，2 忽略 |

## 已知失败（重构前就存在）

内核 4 个失败全部是 Windows 路径断言，在 Linux 上运行必然失败，与本次重构无关：

- `doctor::path::tests::claude_dir_from_path_picks_parent_or_npm_prefix`（期望 `C:\Users\mi\.local\bin`）
- `doctor::path::tests::prime_and_prepend_behaviour`
- `doctor::verify::tests::install_root_groups_npm_entries_together`
- `doctor::verify::tests::two_installs_with_different_versions_report_both_conflicts`

这些测试属于"检测 claude CLI 安装"的 doctor 模块，P5 会改为检测 agentd，届时一并重写为跨平台。

## 不能用的检查组合

- `cargo check --workspace --all-targets`：工作区特性合并会把桌面壳与服务端壳的互斥特性同时打开，`server.rs` 报 11 个错误。这是构建方式问题，不是代码缺陷；按上表分壳检查。
- `cargo test -p polaris-collab`（不带特性）：`examples/` 依赖 `collab-host`/`collab-net`，需按上表加特性。

## 环境前提

`build-essential pkg-config libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev cmake clang protobuf-compiler libopus-dev libasound2-dev`，Node 22.19，Rust stable 1.99。WSL 中 `sudo apt` 需显式传代理 `-o Acquire::http::Proxy=...`。

## 补充基线（2026-10-14，P5b 前记录）
`cargo test -p polaris-wiki -p polaris-fable --lib --tests`：103 通过、2 失败、7 忽略。2 个失败为重构前即存在（fable 自基线提交 b6d9e38 起未被改动），均为 Windows 路径断言：
- `fable::inventory::tests::rel_of_is_root_relative_slash_path`
- `kb::access::tests::path_contains_handles_verbatim_prefix_and_case`
P5a 已修复内核的 4 个同类失败（内核现 0 失败）。
