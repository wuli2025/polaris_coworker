# P6a-2 执行报告与 P6b 接口

本报告描述执行结果，最终由 Claude 重跑、Pi 独立审查后判定验收。保留工作区已有 R8/P6a 改动，无 commit/push，无新依赖，未修改前端。具体文件、命令与环境限制见 [验证清单](p6a2-validation.md)。

## 两个回归

- `BackendStatus` 派生 `Default`；wiki 的测试替身只指定 backend/healthy，其余使用默认值。现有后端 `new("pi")` 的专家同步状态初始化不变。
- mesh 的连接责任移到 `mesh/connections.rs`，有网/无网构建分别编译实际连接与明确的不可用分支，不用新增 `allow` 消除不可达或未使用变量警告。无网分支仍保留可见的 degraded 状态。

## 设备、会话与吊销

- 设备目录仍以 iroh NodeId 定位机器，新增独立、不含设备密钥哈希的 `dev_*` 登记 ID。正常重登保留 ID/接入时间/次序；吊销后重新登记生成新 ID，旧 ID 的吊销记录永久保留。同一 NodeId 不能跨账号覆盖登记。
- `/api/mesh/assert` 返回的 PA1 断言新增 `device_id`、`node_id`，TTL 保持 300 秒，沿用原时钟偏差规则。设备密钥仍仅持久化 SHA-256 哈希。
- 本机会话 `sessions.device_id` 存签名中的设备 ID；`bound_sessions` 将其映射到 UID/NodeId。现有设备契约闸得到 NodeId，成员资格与手工权限依旧由本机决定。
- `login_assertion` 的历史请求字段 `deviceId` 可为空、为签名的 NodeId 或设备 ID；其他值拒绝。邀请加入和本机控制台也走绑定检查。桌面入网后用设备密钥取得绑定断言，替换临时的旧格式本机会话。
- 旧断言没有两个绑定字段时保留既有准入方式，打印 `[auth] legacy_unbound_assertion` 并写 `auth.legacy_unbound_assertion` 审计。仅缺其中一个字段时拒绝。
- 权威按账号签发 `PR1` 清单：UID、版本、签发时间、吊销设备 ID/NodeId/时间/版本；附带当前设备接入时间/次序和首设备 ID，供离线高风险策略判断。
- 吊销与版本递增处于同一个 SQLite 写事务。接收端验证固定域前缀和钉住的 Ed25519 公钥，拒绝篡改、未签名文档、跨账号清单、版本/时间回退及异常未来时间。
- 验签成功后，缓存、匹配会话删除及对应本机契约 revoked 在一个事务内落地，随后递增现有 `SESSION_REVOKE_GEN`。每个会话请求在短缓存前查本地绑定/吊销状态；不访问网络。
- 清单是累计吊销历史，不因重登删除旧代际记录；同一清单重复拉取不会再次覆盖用户在同步后作出的显式本地恢复决定。
- mesh 启动及 collab HTTP router 启动均启动同一幂等安全同步任务。它与 mesh 生命周期共用入口，单独定时运行，避免慢隧道/挂盘阻塞清单拉取。各本机成员账号并行拉取，默认 30 秒一轮。

## 离线边界

普通请求始终使用最后一份有效清单。网络错误、清单缺失或验签失败不会以空清单覆盖缓存，也不会中断尚未吊销的既有会话；失败会打印告警并写审计。

高风险操作要求清单签发时间距今不超过默认 600 秒，缺少或过期时返回“需要联网确认”。清单仍新鲜时，已有可信设备、窗口已结束设备和单设备账号可以本地通过；窗口内新设备还必须拿到权威签名的本次批准，离线不能绕过批准。

在正常在线拉取成功的条件下，吊销在下一轮同步中生效。每次拉取超时不超过 `min(poll, 10s)`，失败保留旧清单并告警；不能对网络不可达、进程停止或调度停滞承诺 30 秒墙钟上限。

旧无绑定断言和升级前的旧会话继续兼容，无法提供本模块的密码学设备归属与定向吊销保证。升级后重新登录可得到绑定会话；本次没有强制清空所有旧会话，保留离线可用性。机器级管理员凭据、本机未入网模式仍沿用既有管理边界。

## 策略 B

- 取消同账号设备的自动只读/执行冷静期。旧 cooldown 数据迁移清零；兼容字段仍序列化，`inCooldown` 为 false。显式只读、禁执行、禁自动挂载和 revoked 不变。
- 新设备登记事务向全部既有有效设备写入持久通知，含设备 ID、NodeId、设备名、平台、时间与吊销动作。事务提交唤醒长轮询，桌面通过 `interconnect:security` 事件发布；报到响应也带未确认通知。断线重连可重复送达，前端按 deviceId 去重并显式 ack。
- 窗口内的新设备执行高风险操作时创建 pending 请求，业务 mutation 尚不执行。请求绑定账号、设备代际、目标主机、操作类别与完整参数摘要。
- 只有接入次序早于申请设备且未吊销的同账号设备可以批准/拒绝；禁止自批、新设备互批、跨账号批准、过期批准和撤销后的批准者继续授权。
- 首台设备作为初始可信设备；账号当前仅一台有效设备时免确认。普通重登不重置窗口，吊销后重新登记重新计算窗口。
- pending 返回 `approval_pending:<id>`。P6b 展示待批准状态，批准后重试完全相同的操作；批准只消费一次。超时/拒绝返回 `approval_expired_or_denied:<id>`，下一次明确重试可创建新请求。
- 不保存密码/邮箱变更的明文待执行载荷，不在后台自动重放业务请求。批准与业务执行之间若出错，已消费批准不会复用。
- `mesh_device_remove` 先通过权威吊销，再撤本机契约并拆盘。批准挂起/超时时不会先做本地破坏操作；权威已成功而本地撤销失败时返回 partial，允许重试。
- 高风险清单入口来自 `risk_inventory.rs`，HTTP middleware 与 invoke 直接使用该清单；Tauri/fs share/unmount/peer/mesh 变更包装器使用相同操作 ID。内部 mesh 自动拆盘使用专用内部接口，避免吊销清理本身被批准闸阻止。
- 基础 API 的显式无效/吊销会话不再回退为免登录 owner。

## 配置与切换

配置沿用 `POLARIS_INTERCONNECT_CONFIG` → 用户 JSON 与随包默认值合并。现有只含 authority/relay 的配置无需修改。`security` 拒绝未知字段、字符串冒充数字、负数、零和超界值。

| 字段 | 默认 | 范围 |
| --- | --- | --- |
| `revocationPollSecs` | 30 秒 | 1–3600 |
| `revocationMaxAgeSecs` | 600 秒 | 1–86400 |
| `newDeviceWindowHours` | 24 小时 | 1–720 |
| `approvalTimeoutSecs` | 300 秒 | 1–3600 |

示例：更频繁同步、缩短离线高风险有效期，同时保留 24 小时新设备窗口：

```json
{
  "security": {
    "revocationPollSecs": 15,
    "revocationMaxAgeSecs": 300,
    "newDeviceWindowHours": 24,
    "approvalTimeoutSecs": 180
  }
}
```

```sh
export POLARIS_INTERCONNECT_CONFIG=/absolute/path/interconnect.json
```

默认值/类型契约分别在 `interconnect/defaults.json`、`schema.json` 和 `mod.rs::resolve`。权威和成员主机应部署一致的窗口策略；沿用既有权威 URL/完整公钥指纹与 relay 配置。

## P6b 所需接口

| 接口 | 凭据/输入 | 结果或用途 |
| --- | --- | --- |
| `GET /api/mesh/revocations?uid=…` | 签名文档可公开获取；调用方必须验钉住的公钥 | `{document:"PR1.…"}`，不能信任裸 JSON 吊销数组 |
| `GET /api/mesh/security/inbox` | Bearer meshKey | `notifications[]`、`approvals[]` |
| `GET /api/mesh/security/watch?cursor=…` | Bearer meshKey | 长轮询，返回收件箱及新 cursor；断线后可重新获取 |
| `POST /api/mesh/security/ack` | Bearer meshKey；`{deviceId}` | 仅确认当前设备收件箱的该条接入通知 |
| `POST /api/mesh/risk/decide` | Bearer meshKey；`{requestId,approve:boolean}` | 既有设备批准或拒绝 |
| `POST /api/mesh/risk/authorize` | Bearer 主机 meshKey；`actorNodeId,operation,digest,nonce`，可附 `actorAssertion` | 内部授权适配；返回一次性消费后签发的 `PRISK1` permit，含账号/actor/host/digest/nonce/过期时间 |
| `POST /api/mesh/revoke` | Bearer meshKey；`{nodeId}` | 先过批准闸，再吊销；权威业务拒绝沿用 HTTP 400/error |
| 高风险成员 HTTP / invoke | 原有会话凭据；跨账号时可附 `x-polaris-device-assertion` | 新设备须带其账号新签的绑定断言（仍检查 5 分钟 TTL），避免让目标主机冒充另一账号的设备 |
| `/api/account/reset`、`/api/collab/email/reset` | 有 mesh 设备的账号额外要求其 Bearer meshKey，仍需原有邮箱验证码 | 新设备改密也必须批准；未登记设备的老账号保留恢复流程 |
| Tauri `mesh_security_inbox` | 无参数 | 通过本机保存的设备密钥拉取 |
| Tauri `mesh_security_ack` | `deviceId` | 确认通知 |
| Tauri `mesh_risk_decide` | `requestId,approve` | 批准/拒绝 |
| Tauri `mesh_device_remove` | `nodeId` | 通知内“一键吊销”；权威 + 本机契约 |
| 桌面事件 `interconnect:security` | 通知/批准项 | 展示、去重；最终动作以接口返回为准 |

HTTP 高风险成员操作被挂起/拒绝时返回 403/error；Tauri 返回同一错误字符串。跨账号新设备审批要求成员主机已入网，且客户端提供新鲜绑定断言；缺凭据时拒绝，不能降级成只靠请求体设备名。

## 数据迁移

`db::migrate` 调用独立 `interconnect/security.sql`，新增设备代际、首设备、权威版本/吊销历史、已验清单/吊销缓存、会话绑定、通知和批准表及索引。无需新依赖或外部迁移命令。

旧目录设备回填独立随机 ID；旧 revoked 行回填吊销历史，其吊销时间只能记录升级迁移时间，不能还原旧系统未保存的真实吊销时间。已存在的旧会话保持原格式；新绑定会话用外键级联删除，缓存版本随有效清单落地递增。

部署顺序：先升级权威，再升级成员主机，再重新登录需要设备保证的客户端。旧权威没有签名清单时成员保留缓存并告警；高风险操作缺新鲜清单将拒绝。前端展示/交互留给 P6b。
