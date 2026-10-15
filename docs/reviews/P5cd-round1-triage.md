# P5c/P5d 第 1 轮 Pi 审查处置（Claude）

原始报告：`docs/reviews/P5cd-fe-round1/`、`docs/reviews/P5cd-be-round1/`（均含测试）。

## 前端包
| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 严重 | 前端透传 projectId 可越界读文件 | **有证据驳回** | 后端 `claude_md/context.rs:94-97` 要求 projectId 属于活跃项目（白名单，不拼路径）；文件名经 `filename()` 限定为 `AGENTS.md`/`CLAUDE.md`（`:25-27`）；`target()` 拒绝任何符号链接（含悬挂）并要求规范化后在项目内（`:39-53`）；`pinned()` 校验选择未过期（`:69-75`）。 |
| 2 中 | `report.node` 为 null 时崩溃 | **驳回** | 后端 `doctor/types.rs:39` 中 `node: ToolStatus` 非 Option，不会返回 null。 |
| 3 中 | 专家开关服务端无鉴权 | **有证据驳回** | 同后端包 #2。 |
| 4 低 | 类型守卫过宽，字符串可通过 | **驳回** | `typeof value.unpricedRequests !== 'number'` 对字符串为真，会返回"价格完整性未知"，Pi 推理反了。 |
| 5 低 | 切换项目时未保存修改丢失 | **采纳** | 切换项目前若有未保存修改，确认后才切换，取消则回滚选择。 |

## 后端包
| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 | 写入不截断 | **Pi 已自行撤回** | `context.rs` 已 `truncate(true)`。 |
| 2 阻塞 | 专家开关无鉴权，任何人可改 | **有证据驳回** | 服务端 `/api/invoke` 的 `required_role` 对所有命令返回 3（owner），`apihub/auth.rs:180-181` 与 `apihub/invoke.rs:96` 拒绝非 owner；桌面 Tauri 命令仅本机 owner 可调。 |
| 3 中 | `ExpertConfig.prompt` 无长度上限 | **采纳（低）** | 增加可配置上限（默认 32 KiB）与校验，超限具名错误；比较前规范化尾部空白。 |
| 4 中 | `restore_experts` 失败阻止后端就绪 | **采纳（中）** | agentd 暂不可达时不得阻止后端启动：状态标为"未同步"，在后续 ping 恢复时重试同步，`agent_backend_status` 暴露同步状态。 |
| 5 低 | `has_price` 不可逆 | **驳回，补测试** | 聚合按每次查询重新计算，`has_price` 表示"该时间窗内此模型的请求全部有价"，语义正确；补边界测试固化该语义。 |
