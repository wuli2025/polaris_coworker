# P6a/P6a-2 第 1 轮 Pi 审查处置（Claude）

原始报告：`docs/reviews/P6a-round1/`（含测试）。Pi 多处称"实现未提供"，实际 `polaris-collab/src/collab/interconnect/` 新模块已在包内；主控逐条直接核对代码。

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 高 | 吊销清单可回滚重放 | **有证据驳回** | `interconnect/revocations.rs:69` 新清单版本或签发时间早于缓存即拒绝；同版本重放无害；`:57-60` 校验账号与条目版本不超过清单版本。 |
| 2 高 | 新设备可自批高风险操作 | **有证据驳回** | `interconnect/risk.rs` 批准时 SQL 要求请求方设备序号大于批准方（`d.sequence>?6`），同一设备不可能比自己新，新设备也不能替旧设备批准；`http/security_routes.rs:75` 另校验断言中的 device_id 与会话设备一致。 |
| 3 高 | relay 列表无签名，可被中间人改写 | **降为低，不改** | 权威地址强制 HTTPS（环回除外，`src/mesh.rs:236-244`），篡改需先攻破 TLS；且 iroh relay 只转发端到端加密的 QUIC 流量，无法读取或伪造内容，最坏是拒绝服务。记入 feature-tree 作为已知边界。 |
| 4 高 | 未做预置指纹校验 | **有证据驳回** | `interconnect/gate.rs:44`、`revocations.rs:51,132` 在配置了指纹时 `verify_key`；`interconnect/mod.rs:189` 明确"未配置指纹不做首次信任"。 |
| 5 中 | 吊销清单缓存损坏行为未定义 | **采纳（低）** | `revocations.rs:128-139` 缓存验签失败返回错误（高风险操作因此被拒，安全侧正确）；补：损坏时清除缓存并在下次心跳重拉，补测试。 |
| 6 中 | device_id 未与断言绑定校验 | **有证据驳回** | `http/security_routes.rs:75` 比对断言 `claims.device_id` 与会话设备。 |
| 7 中 | 冷静期代码残留 | **采纳（低）** | 删除 `grants/contract.rs:23` `in_cooldown` 等已失效逻辑与常量，避免误导。 |
| 8 中 | 环境变量优先级未实现 | **有证据驳回** | `interconnect/mod.rs:80` 优先读取 `POLARIS_ACCOUNT_AUTHORITY_URL`，`:140-157` 解析配置覆盖链。 |
| 9 低 | 空 relay 列表处理不一致 | **不处理** | 低，P6b 前端显示状态时一并核对。 |

# R8（P5c/P5d 第 2 轮）处置
| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 中 | `BackendStatus::default()` 使 `experts_synced=None`，与 `new()` 的 `Some(false)` 语义不一致 | **采纳（低）** | 生产代码统一用显式构造；`Default` 仅在测试可用或默认值改为 `Some(false)`。 |
| 2 低 | 回滚失败降为未同步存疑 | **驳回** | 回滚失败时 agentd 实际配置可能是新值而持久化为旧值，二者不一致，标"未同步"正确。 |
| 3 低 | 字节/字符混用 | **驳回** | Rust `str::len()` 与 `trim_end().len()` 均为字节数，Pi 理解有误；上限以字节计，在配置注释写明。 |
结论：P5c/P5d 达 2 轮上限，修完 #1 后标记完成。
