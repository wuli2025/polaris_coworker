# 审查 brief：P3 权限闸、真沙箱、联网工具、子代理、技能与专家

两包分开送审：A 包 = agentd 扩展（`P3a-diff.patch`），B 包 = Rust 工具路由/协议/前端授权弹窗（`P3b-diff.patch`）。测试文件已从 diff 排除以控制体积。

## 验收条件
1. 四档权限：deny 只读（只放行只读工具与联网检索）；manual 写/执行前发 `permission.request`，等 `permission.respond`（allow/deny/allowAlways），超时按拒绝；auto-current 工作目录内读写自动放行、执行与目录外写走 manual；auto-all 全放行。`toolProfile` 决定可见工具，权限扩展决定每次调用是否放行，两层都要有。未知工具默认需要批准。
2. 真沙箱：bash/write/edit 经协议交给 Rust，在受控容器执行；沙箱不可用时报错，**禁止静默回落宿主**；cwd 必须在容器可写挂载下。
3. web_fetch：仅 http/https；解析后按 IP 拒绝私网/环回/链路本地，重定向后重判；大小/超时/重定向上限可配。web_search：可配置适配器；无 Key 明确报错。
4. subagent：并发/深度上限；子会话继承父会话权限档位与沙箱设置（不能借子代理绕过权限）；父中止级联中止。
5. 上下文：AGENTS.md 优先，否则 CLAUDE.md；不自动改名。专家默认关，开启时作系统提示附加段。

## Claude 实测（沙箱外）
agentd typecheck 通过、`npm test` 91/91（含子进程端到端与本地 HTTP 假服务器测试）；Rust 两壳 `cargo check` 通过（7 警告同基线）；`cargo test -p polaris-kernel --lib --tests` 171 通过，4 失败均为基线 Windows 路径测试；`vue-tsc` 通过；前端 vitest 4 个测试文件 12 项通过（根 vitest 误收 agentd 的 node:test 文件，属配置问题，另行修复）。改动文件 ≤500 行。

## 已知问题（主控已掌握，不必复述）
- 现有 `sandbox_start` 把 `~/Polaris` 整体挂入容器（含明文 Key 的 `data/providers.json`），P3 拒绝路由到这类挂载，挂载策略改造另列 P3b。
- 前端显式专家/团队选择仍写文件，另列 P3b。

## 请重点审查
权限能否被绕过（工具名伪装、参数中路径穿越绕过"目录内"判定、符号链接、子代理/工具链间接执行、allowAlways 的作用域是否过宽）；SSRF（DNS 重绑定、IPv6/IPv4 映射地址、十进制/八进制 IP 写法、重定向到私网）；沙箱路由能否回落宿主；permission.request 等待期间 abort/close/超时的收尾；协议 v3 两端一致性。
