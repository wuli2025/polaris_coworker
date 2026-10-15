# 审查 brief：P7b 系列（演练发现的产品修复）

审查者为 openai-codex/gpt-6-astra，与执行者同属 OpenAI 家族，独立性较弱，请尽量挑刺。

## 范围
- 包 1 `P7b-diff.patch`（a941730）：agentd 打包缺辅助文件、对话中途 steer/followUp 入口（kernel live_input、apihub invoke/auth、前端 LiveInputActions）。含测试。
- 包 2 `P7b23-diff.patch`（f5f29a6 + f3157ff + ab3226e）：订阅提供商切换、提供商离线保存、agentd 经 undici EnvHttpProxyAgent 走代理、doctor 网络探测、私有 CA（`authority.extraCaPem`，http_client）、mesh 统一 CA 客户端、Dockerfile 修复。含测试。已排除锁文件（仅新增同版本直接依赖 undici 8.10.2、webpki-roots 0.26.11）。
- 包 3 `P7b23-splits.patch`：authority.rs → membership.rs/tests.rs/host_tests.rs、mesh.rs → device_commands.rs/tests.rs 的拆分。**不是纯搬移**：其中 ureq 直接调用改为统一 CA 客户端，测试路径前缀调整。请重点核对搬移过程中有无遗漏或语义变化。

## Claude 实测（沙箱外）
三种 cargo check 均 0 警告；kernel/wiki/fable、collab 测试通过；agentd typecheck + test 通过；vitest、vue-tsc、vite build 通过。
真实演练：D1 7/8（switch 缺第二模型）、D2 3/3、D3 9 通过 2 blocked（server 未接沙箱执行器，属设计）。代理修复后，真实模型可经 Clash 访问。

## 请重点审查
1. steer/followUp：会话与 reqId 的绑定、busy/done 竞争、跨用户能否向他人会话插话（鉴权）。
2. 代理：凭据和代理 URL 是否可能出现在日志或错误中；NO_PROXY 语义；调用方替换的 fetch 是否被覆盖。
3. 私有 CA：是否在任何路径回退到默认客户端，或关闭主机名/有效期校验；extraCaPem 来源是否可被非 owner 修改。
4. 提供商离线保存：未校验的配置能否在切换时绕过 SDK 校验；缓存是否可能被投毒。
5. 拆分文件的语义等价性。

输出：按严重程度列出问题，给出文件与行号；没有问题请明确写 PASS。
