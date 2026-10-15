状态：**BLOCKED，未声明 P7b 完成**。

- 已修复打包依赖收集，新增真实生产打包回归。
- 已实现 HTTP/Tauri 插话与排队、请求归属检查、前端按钮及 D1 真实断言。
- 三种离线 Cargo check 均通过、0 警告；vue-tsc 通过；Vitest **118 项通过**；chat 定向测试 **103 项通过**。
- 必测 Rust 全量失败：collab 176 通过；kernel 221 通过、6 失败、2 ignored。失败涉及环境禁止本地端口绑定。
- **CA/D4 未实现**：ureq 未公开默认信任根；保留默认根并追加 CA 的直接方案需要新增 Cargo 直接依赖，未越过禁止新增依赖的约束。
- 真实 D1/D4 演练未运行。未 commit/push，保留用户改动。

[逐文件改动与阻塞说明](/home/mi/work/polaris-pi/docs/pi-migration/p7b-product-gaps.md) · [实际命令和验证结果](/home/mi/work/polaris-pi/docs/pi-migration/p7b-validation.md)。`docs/feature-tree.md` 已更新，交由 Claude/Pi 复核。