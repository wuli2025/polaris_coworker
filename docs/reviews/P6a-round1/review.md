## 审查结果

**审查模式：** packet（无工具）

---

## 结论：CHANGES_REQUIRED

### 范围与缺失材料

**已审查：**
- `P6a-collab.patch`（polaris-collab 核心库）
- `P6a-host.patch`（宿主装配与 mesh）

**关键缺失材料：**
- `interconnect` 模块完整实现源码未提供（路由已注册但核心逻辑在引用处不可见）
- `trust.rs`、`assertions.rs`、`sessions.rs` 等拆分模块的完整实现未提供
- 签名吊销清单的具体字段结构和验证逻辑未提供
- `policy_commands.rs`、`security.rs` 等新增子模块实现未提供

---

## 问题

### 严重程度：HIGH

#### 1. 吊销清单版本回滚攻击（重放旧清单）未防护

**位置：** `P6a-collab.patch` 中引用 `interconnect` 模块，但实现未提供

**问题：** Brief 要求「版本单调递增」，但 patch 中未见版本号比较逻辑。
- `mesh.rs` 第 1309 行 `devices::enrolled` 和 `revoked` 调用后没有版本号验证
- 成员主机侧接收新清单时，未见 `if new_version <= cached_version { reject }` 的逻辑

**可复现影响：** 攻击者重放旧清单可让已吊销设备重新有效

**最小修复建议：** 在 `interconnect` 模块中实现：
```rust
if incoming_version <= self.cached_version { return Err("版本不能回滚"); }
```

---

#### 2. 新设备可自我批准（绕过高风险操作审批）

**位置：** `P6a-collab.patch` 第 1263-1264 行注册路由：
```rust
.route("/api/mesh/risk/authorize", post(security_authorize))
.route("/api/mesh/risk/decide", post(security_decide))
```

**问题：** 
- `security_decide` 接口只检查 `session_proven`（第 62 行），未验证**批准者不是当前请求者**
- 新设备发起高风险操作后，可直接调用 `decide` 批准自己的请求

**可复现影响：** 新设备在窗口期内通过自批准绕过高风险操作限制

**最小修复建议：** `decide` 接口须验证批准者 device_id 与操作发起者不同，且批准者不在窗口期内

---

#### 3. Relay 列表下发无签名验证（MITM 风险）

**位置：** `P6a-collab.patch` 第 403 行：
```rust
super::interconnect::apply_received_relays(&v)?;
```

**问题：**
- `apply_received_relays` 直接应用权威返回的 relay 列表，无签名验证
- 权威响应可能被中间人篡改 relay URL

**可复现影响：** 流量被重定向至恶意 relay

**最小修复建议：** relay 列表应与断言同签，或至少与 meshKey 绑定

---

#### 4. TOFU 流程未实现指纹校验

**位置：** `P6a-collab.patch` 第 272-290 行 `fetch_public_key`：
```rust
fn fetch_public_key(url: &str) -> Result<String, String> {
    let ep = format!("{}/api/account/pubkey", url.trim_end_matches('/'));
    // 直接接受权威返回的公钥，无指纹比对
```

**问题：**
- Brief 要求「随包权威配置（URL + 预置公钥指纹），指纹不符拒绝」
- 代码中 `ensure_pinned` / `pin_explicit` 直接接受权威返回的公钥，未与预置指纹比对
- `trust.rs` 实现未提供，但 patch 第 164 行可见其导出

**可复现影响：** 中间人攻击：攻击者伪造权威，返回自己的公钥

**最小修复建议：** 在 `fetch_public_key` 或 `ensure_pinned` 中增加指纹比对逻辑

---

### 严重程度：MEDIUM

#### 5. 吊销清单缓存损坏时行为未定义

**位置：** `interconnect` 模块（实现未提供）

**问题：** Brief 要求「清单缓存被删除/损坏时回落本地模式不崩溃」，但 patch 中未见此逻辑

**最小修复建议：** 提供缓存完整性检查或签名验证，损坏时清除并触发重新拉取

---

#### 6. 断言与 device_id 绑定断言缺失

**位置：** `P6a-collab.patch` 第 422-498 行 `login_with_assertion_ex`：
```rust
pub fn login_with_assertion_ex(
    assertion: &str,
    device_id: &str,  // 直接传入，未与断言内容绑定验证
    local_console: bool,
) -> Result<(User, String), String> {
```

**问题：** Brief 要求「断言绑定 device_id 与 NodeId」，但 `device_id` 参数只是传入值，未验证其是否与断言中的声明一致

**最小修复建议：** 断言中应包含 device_id，成员主机验签时比对

---

#### 7. 已失效的冷静期代码残留

**位置：** `P6a-collab.patch` 第 686 行：
```rust
pub const COOLDOWN_SECS: i64 = 24 * 3600;  // 仍保留常量
```

**问题：** 策略 B 取消冷静期，但 `COOLDOWN_SECS`、`in_cooldown()`、`effective_fs()` 仍存在（只是返回全权）。遗留代码可能误导维护者

**最小修复建议：** 删除或注释掉冷静期相关死代码

---

#### 8. 环境变量优先级实现未验证

**位置：** `P6a-host.patch` 第 173-183 行 `default_authority`：
```rust
fn default_authority() -> String {
    match crate::collab::interconnect::load() {
        Ok(config) => config.authority.url.map_or_else(String::new, |url| url),
        Err(_) => String::new(),
    }
}
```

**问题：** Brief 要求优先级「环境变量 > 用户配置 > 随包默认」，但 `default_authority` 只调用 `load()`，未见环境变量 `POLARIS_ACCOUNT_AUTHORITY_URL` 的显式检查

---

### 严重程度：LOW

#### 9. 登录响应空 relay 列表的处理不一致

**位置：** `P6a-collab.patch` 第 403 行：
```rust
super::interconnect::apply_received_relays(&v)?;
```

**问题：** 未见空列表时的 fallback 逻辑（Brief 要求「空列表按 fallback 决定是否用 n0」）

---

## 建议验证步骤

以下步骤**仅基于推断**，未运行测试：

1. **版本回滚验证：** 用旧版本号构造伪造清单，验证成员主机是否拒绝
2. **自批准验证：** 新设备发起高风险操作后立即调用 `decide`，验证是否被拦截
3. **Relay MITM 验证：** 在权威响应 relay 列表处注入恶意 URL，验证客户端是否接受
4. **指纹校验验证：** 伪造权威返回错误公钥，验证 `ensure_pinned` 是否拒绝
5. **断言绑定验证：** 用断言搭配错误 device_id 登录，验证是否被识别

---

**备注：** 由于 `interconnect` 模块实现源码缺失，无法完成对吊销清单签名验证、版本单调性、批准流程等核心安全逻辑的审查。
