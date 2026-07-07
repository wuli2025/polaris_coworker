# Source Registry — Polaris 团队协作架构调研 (2026-07-01)

## A. 编码 Agent 协作产品
- Devin/Cognition 定价 https://devin.ai/pricing/ · Knowledge https://docs.devin.ai/product-guides/knowledge · VPC https://docs.devin.ai/enterprise/vpc/overview · DeepWiki https://ai.miraheze.org/wiki/DeepWiki
- Factory 企业部署 https://docs.factory.ai/enterprise · Droid Computers https://factory.ai/news/droid-computers
- Cursor Teams https://cursor.com/business/teams · IAM https://cursor.com/docs/enterprise/identity-and-access-management · Cloud Agents https://cursor.com/docs/cloud-agent · 自托管 https://cursor.com/blog/self-hosted-cloud-agents
- Claude Code Enterprise https://claude.com/product/claude-code/enterprise · 部署 https://code.claude.com/docs/en/third-party-integrations
- GitHub Copilot Spaces https://docs.github.com/en/copilot/concepts/context/spaces
- Amp https://sourcegraph.com/amp · Windsurf 共享 https://www.iamraghuveer.com/posts/windsurf-team-shared-memory/

## B. 企业知识库 Agent + 权限
- Glean permissions-aware https://www.glean.com/perspectives/security-permissions-aware-ai · 索引ACL https://developers.glean.com/api-info/indexing/documents/permissions
- Manus Projects https://manus.im/docs/features/projects · Team https://manus.im/team
- 飞书知识库权限 https://www.feishu.cn/hc/zh-CN/articles/821998241087 · Base高级权限 https://www.feishu.cn/hc/zh-CN/articles/588604550568 · 知识问答 https://www.feishu.cn/hc/zh-CN/articles/854453754409
- Notion 权限与AI https://www.notion.com/help/sharing-and-permissions · M365 Copilot 架构 https://learn.microsoft.com/en-us/copilot/microsoft-365/microsoft-365-copilot-architecture · Slack AI https://slack.com/help/articles/47421816860947 · Rovo https://support.atlassian.com/rovo/docs/rovo-agent-permissions-and-governance/

## C. 自托管知识库架构 + 本地优先同步
- Outline https://docs.getoutline.com · Docmost https://noted.lol/docmost/ · Trilium sync https://docs.triliumnotes.org/developer-guide/concepts/sync
- libSQL/Turso https://docs.turso.tech/features/embedded-replicas/introduction · libSQL https://github.com/tursodatabase/libsql · cr-sqlite https://vlcn.io/docs/cr-sqlite/intro
- Litestream https://litestream.io/how-it-works/ · PowerSync/Electric/Zero https://trybuildpilot.com/648-electric-sql-vs-powersync-vs-zero-2026 · local-first https://www.inkandswitch.com/essay/local-first/ · SQLite WAL https://sqlite.org/wal.html

## D. 安全与低运维部署
- Tailscale Serve https://tailscale.com/kb/1312/serve · ACLs https://tailscale.com/docs/features/access-control/acls · Tailnet Lock https://tailscale.com/kb/1226/tailnet-lock/
- Headscale https://github.com/juanfont/headscale · CF Tunnel token https://developers.cloudflare.com/tunnel/advanced/tunnel-tokens/ · CF Access https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/self-hosted-public-app/
- Pocket-ID https://github.com/pocket-id/pocket-id · distroless/scratch https://safeguard.sh/resources/blog/scratch-vs-distroless-minimal-images · npm供应链 https://www.cisa.gov/news-events/alerts/2025/09/23/... · Next.js SSRF CVE-2026-44578 https://hadrian.io

## E. 细粒度授权 + permission-aware RAG
- RBAC/ABAC/ReBAC https://www.permit.io/blog/rbac-vs-abac-vs-rebac · OpenFGA https://openfga.dev/docs/configuration-language · Rust crate https://docs.rs/openfga-client · SpiceDB https://authzed.com/docs · 比较 https://inferadb.com/dispatch/authorization-infrastructure-compared/
- permission-aware RAG https://www.rheininsights.com/blog/en/... · 向量层访问控制 https://tianpan.co/blog/2026-05-04-permission-aware-retrieval-enterprise-rag-access-control
- Azure security trimming https://learn.microsoft.com/en-us/azure/search/search-security-trimming-for-azure-search · 查询时ACL https://learn.microsoft.com/en-us/azure/search/search-query-access-control-rbac-enforcement
- Pinecone访问控制 https://www.pinecone.io/learn/rag-access-control/ · 预/后过滤 https://apxml.com/courses/advanced-vector-search-llms/... · SQLite RLS https://docs.sqlitecloud.io/docs/rls · FTS5 JOIN https://sqlite.org/forum/info/509bdbe534f58f20 · Postgres RLS https://www.postgresql.org/docs/current/ddl-rowsecurity.html
