# 北极星团队协作架构调研 — 结论摘要

> 完整交付物: `C:\Users\mi\Desktop\北极星-团队协作架构调研报告-v1.html`(57KB,10节,5对比表,90+来源)
> 2026-07-01 · Full report · 5 研究线并行 · Stakes High

## 一句话结论
团队协作 = 三件事各选对技术:①共享库放哪 ②谁能看哪部分 ③怎么安全连上。北极星零件已齐,不必推倒重来。

## 核心建议(低攻击面·低运维)
- **共享**:把现有 Docker/axum server 版升级成团队唯一权威 `fable.db` 中枢,放服务器**本地盘(绝不放 SMB/NFS,WAL 会损坏)**。桌面/网页/平板都连它。= Outline/Docmost/Trilium 团队版的做法,运维最省。
- **连接**:Tailscale(托管)+ `tailscale serve` 暴露 HTTPS,**不开公网端口、不用 Funnel**;自动 TLS 证书;前置 Pocket-ID(无密码 OIDC/通行密钥)做真认证——不靠网络位置。规避 2025-26 那波未授权自托管 CVE。不选 Cloudflare Tunnel(边缘解密+token 被滥用)。不自建 Headscale(运维反而高)。
- **权限**:SQLite 一张 `section_grants` 授权表 + `user_groups`,检索时按登录身份解析可见分区,**查询命中之后、进大模型之前**做预过滤(`WHERE section_id IN {allowed}`)。FTS5 走 CTE-then-JOIN(直接 JOIN 慢 380×),向量走 allowed 位集预过滤(后过滤会少给/零结果)。**绝不让大模型判权限**。

## 关键洞察
- "让 AI 只用用户有权文档"全行业成败只在一点:**查询时按身份实时校验**(Glean 双重校验+delta-sync;飞书知识问答"千人千面")。索引时打标签不够,权限会变。
- "细粒度知识库权限"在市场罕见:编码 Agent 里只 Devin + Copilot Spaces 真做到按人分权。北极星做扎实即超越大多数编码 Agent。
- 更现代 ≠ 更多组件。最安全省心的形态恰是单二进制+单 SQLite+私有网 = 北极星现状,别误上 Postgres+Redis+K8s。
- 授权模型:默认 RBAC → 需继承加 ReBAC(SQLite 递归 CTE)→ 真复杂才 OpenFGA(Rust crate 成熟;SpiceDB 无官方 Rust SDK)。cr-sqlite 已停更、Oso Rust crate 早期,都别当长期依赖。

## 路线图
阶段0(本周):库确认本地盘 + Litestream 备份 + distroless 非 root 只读根。
阶段1(2-3周):Pocket-ID 身份 + Tailscale Serve 接入。
阶段2(3-4周,核心):授权表 + 检索层收口 `retrieve(user,query)` + 全检索路径查询时过滤。
阶段3(2周):成员权限管理 UI + 权限体检(防过度共享)。
阶段4(按需):libSQL 内嵌副本(离线优先)/ OpenFGA / Postgres。

## 服务器需求
2 vCPU / 4-8GB / 50GB+ SSD 覆盖 10-50 人。扩容看内容量非用户数。NAS $0 / Hetzner ~€4 / Fly.io ~$8-25。

详见桌面 HTML。
