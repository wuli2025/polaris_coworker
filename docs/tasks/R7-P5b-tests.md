# 任务 R7：P5b 补路径穿越回归测试
见 `docs/reviews/P5b-round1-triage.md` #2。在 `src-tauri/crates/polaris-wiki/src/wiki/compile/` 的测试中为 `progress::page_path` 补用例：`a/../b`、`概念/../../etc/passwd`、`概念/嵌套/../穿透.md`、`a\..\b`（Windows 分隔符），断言均被拒绝；不改生产代码。
允许修改：`src-tauri/crates/polaris-wiki/src/wiki/compile/` 下测试文件、`docs/`。必须通过：`cargo test -p polaris-wiki --offline`；两壳 `cargo check --offline`（0 警告）。不 commit。
