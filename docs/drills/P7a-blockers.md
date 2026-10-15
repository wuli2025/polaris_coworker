# P7a 覆盖与阻塞边界

P7b 后续：打包与 steer/followUp 入口、D1 断言已补；本文件保留 P7a 原始发现记录。真实演练未执行，自定义 CA / D4 仍阻塞，见 [P7b 报告](../pi-migration/p7b-product-gaps.md)。

本文件区分已编写脚本、真实运行结果和未实现内容。沙箱未运行真实模型、Docker、产品回归或 age；任何演练是否通过都需要主控实跑。

| 范围 | 本次可执行部分 | 未完成 / 原因 |
| --- | --- | --- |
| D1 | 生产构建、chat、coding、image、compact、abort、同提供商切模型 | steer/followUp 输出 blocked；跨提供商切换、旧 CLI 性能对照未实现 |
| D2 | 真实进程 kill/restart/历史恢复、熔断、双进程凭据锁 | P7 总表的模型断网、401/429 分类与故障切换尚未编写；P7a brief 的 D2 三项已写但未实跑 |
| D3 | 四档权限、manual allow/deny/expire、路径穿越 API 拒绝、真实 Docker fixture、server 无沙箱时报错 | 停 Docker 后的产品沙箱路由、受保护目录项目链接校验输出 blocked；产品创建/执行沙箱链未接通 |
| D4 | 临时 CA/容器拓扑、权威本机浏览器会话通知/审批/吊销、匿名邮箱响应、频控、真实 age 备份恢复脚本 | 账号密码登录完整成网、relay 实际被客户端使用、立即跨主机读写、远端吊销传播、离线高风险/已有会话、清单篡改/回滚未实现，步骤输出 blocked |
| D6 | 所有 crate 独立测试 + 各 JS/Python 测试清单 N 轮、三个实际 build、行数检查 | 未执行；默认范围包含既有超长文件，可能真实 fail，不缩小检查冒充全绿 |

## 确认的产品接口限制

1. **D1 steer/followUp**：`agentd/src/protocol/schema.ts` 和 kernel Pi protocol 有命令，但 `src-tauri/src/apihub/invoke.rs`、`chat_services.rs`、`engines.rs` 没有对应服务端分发入口。
   当前脚本不能从服务端 HTTP/IPC 控制正在运行的 session。没有向 agentd stdin 偷写命令，也没有改产品注册表。
   主控若决定补入口，应定义 conversation/reqId 到 session 的绑定、busy/done 竞争及响应契约，然后补真实步骤。
2. **D3 server 沙箱**：`src-tauri/src/wiring.rs::wire_engine_bridges` 明确仅在 desktop 下注册 ManagedSandbox；server 不注册 SandboxExecutor。
   因而 server-unavailable 可验证缺执行器时 fail closed，却不能验证 Docker 先可用后停机的行为。
   `polaris-sandbox/src/policy.rs` 的 validate_source/validate_container 在产品 desktop 路径内；直接在脚本复刻这个判断不能证明产品拒绝受保护项目链接。
3. **D4 自签 CA 信任**：`polaris-collab/src/collab/authority/trust.rs` 用默认 ureq::get；`interconnect/browser/authority.rs` 用默认 AgentBuilder。
   当前 Cargo 清单只为 ureq 加 json；本地 ureq 2.12.1 的默认 TLS 使用 webpki roots，未找到注入 CA 的产品配置。
   容器安装系统 CA 或 curl --cacert 成功都不能直接推导 Rust 客户端已信任同一 CA。脚本没有禁用 TLS 校验，也没有把跨容器 HTTP 冒充本机环回例外。

## D4 实现缺口，不伪称产品缺接口

`/api/interconnect/login|status|retry|security/*|devices/revoke` **已经存在**，注册在 collab HTTP router；不是 invoke 的命令。
脚本利用这些路由完成权威本机浏览器会话策略演练。其身份是 `web_*` 的 session device，不是 iroh 主机 NodeId。

由于自签 CA 委托链阻塞，本次没有继续实现成员本机准入、真实 host enrollment、peer 连接和文件读写适配。
也没有实现委托 authority 停机后的已有会话断言，以及可控 TLS 代理投递损坏/旧版本签名清单。
这些是**本次脚本仍缺的流程**；不声称产品完全没有相应能力。主控应在信任路径可运行后补齐 adapter 和断言。
`notifications/approvals/revoke` 的 evidence.scope 会明确标为 authority HTTP session devices，不计作跨主机结果。

本地拓扑根据 deploy/singapore 角色配置生成，未直接执行生产部署的 prepare/configure 流程：后者要求公开域名、受审镜像 digest 和 ACME。
本次使用本地唯一 tag、生成证书、随机 loopback 端口；relay 由 Caddy TLS 终止，关闭 QUIC 地址发现。
这不是生产 UDP/QUIC 网络等价性演练；`relay` 步骤仍 blocked。

## 其它验证限制

- 缩小上下文窗口已有 settingsConfig.models[].contextWindow 路径，脚本使用它，不把“没有小窗口开关”错误列为阻塞。
  compaction 是否发生通过本次 session 的持久 JSONL 条目验证，未从无压缩事件的 frontend translator 猜测。
- 自动权限中的 `auto-current` 越界操作会请求批准；脚本拒绝并检查文件无变化。它不是系统级硬沙箱。
- 图像步骤验证附件入会话和模型有回复，不验证视觉回答的语义准确度。
- D4 SMTP 使用本容器假端口，不发送真实邮件；R11 必须两边都为 200 且 JSON 相同，不能以“两边同为配置错误”通过。
- D4 backup 真实 age 尚未运行。沙箱 Python 测试用替身替换加密函数，只验证接口参数、归档完整性及恢复动作。
- `--keep` 不保留活动容器、凭据或含密数据库，仅留脱敏排查材料。异常 SIGKILL/断电需要主控清理自己那一次资源。
- D5 实机工作按用户决定推迟，不在 P7a 假报为完成。

本次报告不是模块最终完成声明；Claude 重跑、Pi 独立审查后决定。
