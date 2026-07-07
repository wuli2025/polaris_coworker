---
task_id: plan
created: 2026-07-01
mode: lead + 5 subagents
output_type: Full report (中文 HTML, 放桌面)
stakes: high
freshness: 2024-2026 最新; 注意国内拉镜像/合规
geography: 中国 + 全球 SaaS 生态
---

# Research Plan — Polaris 团队协作架构调研

## Research question
主流 AI Agent 协作平台/产品如何实现团队协作?需要什么服务器、什么云原生/现代化部署方式?
什么架构最适合 Polaris(桌面 + Docker/NAS + server 三端、本地优先的知识库应用),要求:
- 低攻击面、低运维成本
- "一个项目/一个公司共享一个知识库"的团队共享
- 细粒度权限(某些用户只能看知识库的某些部分,RBAC/ABAC/ReBAC)

## Audience
Polaris 创始人/主程(技术决策者,懂 Rust/Tauri/SQLite/Docker)

## Output
Full report(中文 HTML,放 Windows 桌面)

## Stakes
High — 直接决定 Polaris 下一阶段协作功能架构与发版

## Threads
- A: 编码 Agent 协作产品(Devin / Factory DROID / Cursor 团队版 / Claude Code 团队)协作机制 + 服务器/部署模型
- B: 通用 Agent + 企业知识库协作(Manus / 飞书多维表+知识库 / Notion AI / Glean)的共享与权限
- C: 可自托管团队知识库架构(多租户、本地优先+同步 CRDT/Yjs、Outline/AFFiNE/AppFlowy 对标)+ 服务器需求
- D: 安全与低运维部署(攻击面收敛、零信任、Tailscale/WireGuard、单二进制、SQLite 多用户 vs Postgres RLS)
- E: 细粒度权限模型(Zanzibar / OpenFGA / SpiceDB ReBAC、RAG permission-aware 检索、文档/分块级访问控制)

## 何时停止
每条结论多源交叉;每产品的协作/服务器/权限模型有明确依据 URL;找不到写"未找到",绝不编造。
