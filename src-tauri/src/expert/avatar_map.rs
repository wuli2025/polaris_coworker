//! 专家头像映射表 — expert ID → PNG 文件名
//!
//! 规则：头像 PNG 以专家的中文名（name 字段）命名，存于 templates/experts/avatars/。
//! 有些头像文件名与 name 不完全一致（简写了 "/" 或换了词），此映射表处理这些差异。

/// 专家 ID → PNG 文件名（仅文件名，含 .png 后缀）
pub const AVATAR_MAP: &[(&str, &str)] = &[
    // ── 编排/统帅 (5) ──
    ("chief-strategist", "首席战略师.png"),
    ("multi-agent-coordinator", "多Agent协调员.png"),
    ("knowledge-synthesizer", "知识综合器.png"),
    ("strategy-planner", "OKR战略规划.png"),         // 头像名简写
    ("context-manager", "上下文管理器.png"),

    // ── 系统架构 (10) ──
    ("backend-architect", "后端架构师.png"),
    ("frontend-architect", "前端架构师.png"),
    ("architecture-advisor", "系统架构权衡.png"),
    ("api-contract-designer", "API契约设计.png"),
    ("cloud-architect", "云架构师.png"),
    ("kubernetes-architect", "云原生K8s架构师.png"),  // 头像名无斜杠
    ("microservices-architect", "微服务架构师.png"),
    ("graphql-architect", "GraphQL架构师.png"),
    ("event-sourcing-architect", "事件溯源架构师.png"),
    ("platform-engineer-arch", "云原生专家.png"),    // 头像复用

    // ── 语言专精 (12) ──
    ("python-pro", "Python专家.png"),
    ("typescript-pro", "TypeScript专家.png"),        // 头像名无 /Node
    ("golang-pro", "Go专家.png"),
    ("rust-pro", "Rust专家.png"),
    ("java-pro", "Java专家.png"),
    ("cpp-pro", "C++专家.png"),                     // 头像名 C++
    ("csharp-pro", "C#专家.png"),
    ("sql-pro", "SQL专家.png"),
    ("ios-developer", "Swift专家.png"),             // 头像无 iOS
    ("kotlin-specialist", "Kotlin专家.png"),       // 头像无 Android
    ("blockchain-developer", "Solidity专家.png"),   // 头像无 区块链
    ("embedded-systems", "嵌入式专家.png"),        // 头像无 IoT

    // ── 前端/移动 (6) ──
    ("react-specialist", "React专家.png"),
    ("vue-expert", "Vue专家.png"),
    ("nextjs-developer", "Next.js专家.png"),
    ("flutter-expert", "Flutter专家.png"),
    ("accessibility-tester", "无障碍审计员.png"),
    ("mobile-developer", "移动端专家.png"),

    // ── DevOps/基础设施 (9) ──
    ("devops-engineer", "DevOps工程师.png"),
    ("deployment-engineer", "CICD部署工程师.png"),
    ("terraform-specialist", "Terraform专家.png"),  // 头像无 IaC
    ("docker-expert", "Docker专家.png"),
    ("sre-engineer", "SRE工程师.png"),             // 头像无 /可观测性
    ("network-engineer", "网络工程师.png"),
    ("incident-responder", "事故响应官.png"),
    ("ops-engineer", "运维工程师.png"),
    ("platform-engineer-devops", "平台工程师.png"),

    // ── 数据 (8) ──
    ("data-scientist", "数据科学家.png"),
    ("data-engineer", "数据工程师.png"),
    ("data-analyst", "数据分析师.png"),
    ("database-architect", "数据库架构师.png"),
    ("database-optimizer", "数据库优化师.png"),
    ("vector-db-engineer", "向量数据库工程师.png"),
    ("data-contract-engineer", "数据契约专家.png"), // 头像名无 /质量
    ("dataviz-storyteller", "数据可视化专家.png"),   // 头像无 叙事

    // ── AI/机器学习 (7) ──
    ("ai-engineer", "AI工程师.png"),
    ("ml-engineer", "ML工程师.png"),
    ("mlops-engineer", "MLOps工程师.png"),
    ("llm-architect", "LLM架构师.png"),
    ("nlp-engineer", "NLP工程师.png"),
    ("prompt-engineer", "提示词工程师.png"),
    ("rl-engineer", "强化学习工程师.png"),

    // ── 安全/合规 (7) ──
    ("security-auditor", "安全审计员.png"),
    ("penetration-tester", "渗透测试员.png"),
    ("threat-modeling-expert", "威胁建模专家.png"),
    ("appsec-coder", "应用安全工程师.png"),
    ("compliance-privacy", "合规隐私专家.png"),
    ("license-counsel", "法务许可证专家.png"),
    ("privacy-engineer", "密码学专家.png"),

    // ── 质量/治理 (7) ──
    ("code-reviewer", "代码评审员.png"),
    ("test-automator", "测试自动化工程师.png"),
    ("qa-expert", "QA专家.png"),
    ("performance-engineer", "性能工程师.png"),
    ("debugger", "调试专家.png"),
    ("refactoring-specialist", "重构专家.png"),
    ("tech-debt-strategist", "技术债治理专家.png"),

    // ── 专项技术 (6) ──
    ("payment-integration", "支付集成专家.png"),
    ("game-developer", "游戏开发者.png"),
    ("legacy-modernizer", "遗留系统现代化专家.png"),
    ("browser-automation", "浏览器自动化专家.png"),
    ("fintech-engineer", "金融科技工程师.png"),
    ("technical-writer-pro", "技术写作者.png"),

    // ── 文档/技术写作 (5) ──
    ("docs-architect", "文档架构师.png"),
    ("api-documenter", "API文档师.png"),
    ("tutorial-engineer", "教程工程师.png"),
    ("mermaid-expert", "图表专家.png"),
    ("technical-writer", "技术写作专家.png"),

    // ── 产品/项目/战略 (8) ──
    ("product-manager", "产品经理.png"),
    ("delivery-manager", "交付管理专家.png"),
    ("scrum-master", "Scrum大师.png"),
    ("business-analyst", "业务分析师.png"),
    ("ux-researcher", "用户研究专家.png"),
    ("growth-experimenter", "增长实验设计专家.png"),
    ("financial-modeler", "财务建模专家.png"),
    ("pricing-strategist", "定价策略专家.png"),

    // ── 研究/分析 (7) ──
    ("deep-research", "深度研究专家.png"),
    ("competitive-analyst", "竞品分析专家.png"),
    ("market-researcher", "市场研究专家.png"),
    ("trend-analyst", "趋势分析专家.png"),
    ("scientific-researcher", "科学文献研究专家.png"),
    ("osint-analyst", "OSINT情报分析专家.png"),
    ("research-analyst", "研究分析师.png"),

    // ── 营销/内容 (8) ──
    ("content-marketer", "内容营销策略专家.png"),
    ("seo-specialist", "SEO专家.png"),
    ("growth-hacker", "增长黑客.png"),
    ("social-media-manager", "社媒运营专家.png"),
    ("brand-storyteller", "叙事官品牌故事专家.png"),
    ("visual-designer", "视觉设计专家.png"),
    ("pitch-coach", "路演演讲教练.png"),
    ("copywriter", "文案口播专家.png"),
];

/// 根据专家 ID 查找头像 PNG 文件名（不含路径）
pub fn avatar_filename(id: &str) -> Option<&'static str> {
    AVATAR_MAP
        .iter()
        .find(|(eid, _)| *eid == id)
        .map(|pair| pair.1)
}