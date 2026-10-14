# 审查 brief：P1 polaris-agentd（Pi SDK 常驻服务，含第 1 轮修复）

## 目标
Node 22 / TypeScript 进程，经 stdio JSONL 与 Rust 壳通信，托管多个 Pi `AgentSession`（@earendil-works/pi-coding-agent 1.0.4，全家锁定）。请求：ping / models.list / session.open / prompt / steer / followUp / abort / close / config.apply。事件：turn.started / text.delta / reasoning.delta / tool.started / tool.completed / usage / turn.completed(status) / error(code,message)。

## 验收条件
- 配置不硬编码：schema + 默认文件 + 运行时 config.apply，非法配置给字段路径。
- 跨会话并发、同会话串行；ping 永不被阻塞。
- 超时语义：turnIdleTimeoutMs（无 SDK 事件判卡死）+ turnMaxMs（0=不限）。
- 默认只读工具（权限扩展 P3 才做）；重试与压缩默认开启。
- 密钥不进日志与事件；模型错误分五类固定中文提示，不转发提供商原文。
- 优雅退出：stdin 关闭/SIGTERM 时中止所有会话并 flush。

## Claude 实测结果（沙箱外）
- `npm run typecheck` 通过；`npm test` 44/44 通过，连续 3 次运行均通过（含子进程端到端测试：真实启动 main.ts 完成一轮带工具调用的对话）。
- 行数检查：33 个文件全部 ≤500 行，单行 ≤500 字符。

## 请重点审查
会话注册表的竞态（open/close/abort/idle 回收/config.apply 交错）；命令调度在关闭期间的行为；超时与中止只产生一个终态；Redactor 是否有漏网（例如密钥被拆分、编码后出现）；JSONL 读写在背压与超长行下的行为；协议 schema 是否能挡住非法输入到达 SDK。
