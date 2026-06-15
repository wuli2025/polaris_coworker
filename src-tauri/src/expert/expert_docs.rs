//! 专家 CLAUDE.md 文档 — 编译期内嵌模板 + 运行时变量替换
//!
//! 目前只有 GENERIC.md 一个模板，运行时通过变量替换生成具体专家的 CLAUDE.md 正文。
//!
//! 变量: {{NAME}} · {{ID}} · {{ROLE}} · {{DESCRIPTION}} · {{KEYWORDS}} ·
//!       {{CAPABILITIES}} · {{TRIGGER_SIGNALS}} · {{COMPLEMENTS}} ·
//!       {{EXCLUSIVE_WITH}} · {{COST_TIER}} · {{TIMESTAMP}}

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// 读取专家 CLAUDE.md 模板正文。
/// `claude_md_ref` 是 ExpertCard.claude_md_ref，如 `experts/orchestration/chief-strategist.md`。
///
/// 策略:
/// 1. 优先读运行时文件（允许开发期热更新模板）
/// 2. 若文件含 {{}} 占位符，做变量替换
/// 3. 若文件不存在，用 GENERIC.md 做变量替换生成
pub fn get_expert_doc(claude_md_ref: &str) -> Option<String> {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .join("src")
        .join("templates")
        .join(claude_md_ref);

    if base.exists() {
        if let Ok(content) = std::fs::read_to_string(&base) {
            if content.contains("{{") {
                return substitute_template(&content, claude_md_ref);
            }
            return Some(content);
        }
    }

    let template = GENERIC_TEMPLATE;
    substitute_template(template, claude_md_ref)
}

/// 用专家元数据对 GENERIC.md 模板做变量替换，
/// 供 expert_apply 在写入 CLAUDE.md 前填充完整内容。
#[allow(dead_code)]
pub fn build_expert_doc(
    ref_path: &str,
    name: &str,
    role: &str,
    description: &str,
    keywords: &[String],
    capabilities: &[String],
    trigger_signals: &[String],
    complements: &str,
    exclusive_with: &[String],
    cost_tier: u8,
) -> Option<String> {
    let template = GENERIC_TEMPLATE;
    let timestamp = current_date();

    let parts: Vec<&str> = ref_path
        .trim_start_matches("experts/")
        .trim_end_matches(".md")
        .split('/')
        .collect();
    let id = parts.last().unwrap_or(&"unknown").to_string();
    let group = parts.get(parts.len().saturating_sub(2)).unwrap_or(&"unknown");

    let mut result = template.to_string();
    result = result.replace("{{NAME}}", name);
    result = result.replace("{{ID}}", &id);
    result = result.replace("{{GROUP}}", group);
    result = result.replace("{{ROLE}}", role);
    result = result.replace("{{DESCRIPTION}}", description);
    result = result.replace("{{KEYWORDS}}", &keywords.join("、"));
    result = result.replace("{{CAPABILITIES}}", &capabilities.iter().map(|s| format!("- **{}**", s)).collect::<Vec<_>>().join("\n"));
    result = result.replace("{{TRIGGER_SIGNALS}}", &trigger_signals.iter().map(|s| format!("- **{}**", s)).collect::<Vec<_>>().join("\n"));
    result = result.replace("{{COMPLEMENTS}}", complements);
    result = result.replace("{{EXCLUSIVE_WITH}}", &exclusive_with.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("、"));
    result = result.replace("{{COST_TIER}}", &cost_tier.to_string());
    result = result.replace("{{TIMESTAMP}}", &timestamp);
    Some(result)
}

fn substitute_template(content: &str, ref_path: &str) -> Option<String> {
    let timestamp = current_date();

    let parts: Vec<&str> = ref_path
        .trim_start_matches("experts/")
        .trim_end_matches(".md")
        .split('/')
        .collect();
    let id = parts.last().unwrap_or(&"unknown").to_string();
    let group = parts.get(parts.len().saturating_sub(2)).unwrap_or(&"unknown");

    let mut vars: std::collections::HashMap<&str, String> = HashMap::new();
    vars.insert("{{NAME}}", id_to_display_name(&id));
    vars.insert("{{ID}}", id.to_string());
    vars.insert("{{GROUP}}", group.to_string());
    vars.insert("{{ROLE}}", "专家".to_string());
    vars.insert("{{DESCRIPTION}}", "专业领域专家".to_string());
    vars.insert("{{KEYWORDS}}", String::new());
    vars.insert("{{CAPABILITIES}}", String::new());
    vars.insert("{{TRIGGER_SIGNALS}}", String::new());
    vars.insert("{{COMPLEMENTS}}", String::new());
    vars.insert("{{EXCLUSIVE_WITH}}", "无".to_string());
    vars.insert("{{COST_TIER}}", "2".to_string());
    vars.insert("{{TIMESTAMP}}", timestamp);

    let mut result = content.to_string();
    for (key, val) in vars {
        result = result.replace(key, &val);
    }
    Some(result)
}

fn current_date() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| String::new())
}

fn id_to_display_name(id: &str) -> String {
    let display: HashMap<&str, &str> = HashMap::from([
        ("chief-strategist", "首席战略师"),
        ("multi-agent-coordinator", "多Agent协调员"),
        ("knowledge-synthesizer", "知识综合器"),
        ("strategy-planner", "OKR/战略规划"),
        ("context-manager", "上下文管理器"),
        ("backend-architect", "后端架构师"),
        ("frontend-architect", "前端架构师"),
        ("architecture-advisor", "系统架构权衡"),
        ("api-contract-designer", "API契约设计"),
        ("cloud-architect", "云架构师"),
        ("kubernetes-architect", "云原生/K8s架构师"),
        ("microservices-architect", "微服务架构师"),
        ("graphql-architect", "GraphQL架构师"),
        ("event-sourcing-architect", "事件溯源架构师"),
        ("platform-engineer-arch", "平台工程师"),
        ("python-pro", "Python专家"),
        ("typescript-pro", "TypeScript/Node专家"),
        ("golang-pro", "Go专家"),
        ("rust-pro", "Rust专家"),
        ("java-pro", "Java专家"),
        ("cpp-pro", "C/C++专家"),
        ("csharp-pro", "C#专家"),
        ("sql-pro", "SQL专家"),
        ("ios-developer", "Swift/iOS专家"),
        ("kotlin-specialist", "Kotlin/Android专家"),
        ("blockchain-developer", "Solidity/区块链"),
        ("embedded-systems", "嵌入式/IoT"),
        ("react-specialist", "React专家"),
        ("vue-expert", "Vue专家"),
        ("nextjs-developer", "Next.js专家"),
        ("flutter-expert", "Flutter专家"),
        ("accessibility-tester", "无障碍审计员"),
        ("mobile-developer", "移动端专家"),
        ("devops-engineer", "DevOps工程师"),
        ("deployment-engineer", "CICD部署工程师"),
        ("terraform-specialist", "Terraform/IaC专家"),
        ("docker-expert", "Docker专家"),
        ("sre-engineer", "SRE/可观测性"),
        ("network-engineer", "网络工程师"),
        ("incident-responder", "事故响应官"),
        ("ops-engineer", "运维工程师"),
        ("platform-engineer-devops", "平台工程师"),
        ("data-scientist", "数据科学家"),
        ("data-engineer", "数据工程师"),
        ("data-analyst", "数据分析师"),
        ("database-architect", "数据库架构师"),
        ("database-optimizer", "数据库优化师"),
        ("vector-db-engineer", "向量数据库工程师"),
        ("data-contract-engineer", "数据契约/质量"),
        ("dataviz-storyteller", "数据可视化叙事"),
        ("ai-engineer", "AI工程师"),
        ("ml-engineer", "ML工程师"),
        ("mlops-engineer", "MLOps工程师"),
        ("llm-architect", "LLM架构师"),
        ("nlp-engineer", "NLP工程师"),
        ("prompt-engineer", "提示词工程师"),
        ("rl-engineer", "强化学习工程师"),
        ("security-auditor", "安全审计员"),
        ("penetration-tester", "渗透测试员"),
        ("threat-modeling-expert", "威胁建模专家"),
        ("appsec-coder", "应用安全工程师"),
        ("compliance-privacy", "合规/隐私"),
        ("license-counsel", "法务/许可证"),
        ("privacy-engineer", "密码学/数据隐私"),
        ("code-reviewer", "代码评审员"),
        ("test-automator", "测试自动化工程师"),
        ("qa-expert", "QA专家"),
        ("performance-engineer", "性能工程师"),
        ("debugger", "调试专家"),
        ("refactoring-specialist", "重构专家"),
        ("tech-debt-strategist", "技术债治理"),
        ("payment-integration", "支付集成专家"),
        ("game-developer", "游戏开发者"),
        ("legacy-modernizer", "遗留系统现代化"),
        ("browser-automation", "浏览器自动化"),
        ("fintech-engineer", "金融科技工程"),
        ("technical-writer-pro", "技术写作者"),
        ("docs-architect", "文档架构师"),
        ("api-documenter", "API文档师"),
        ("tutorial-engineer", "教程工程师"),
        ("mermaid-expert", "图表专家"),
        ("technical-writer", "技术写作"),
        ("product-manager", "产品经理"),
        ("delivery-manager", "项目交付管理"),
        ("scrum-master", "Scrum Master"),
        ("business-analyst", "业务分析师"),
        ("ux-researcher", "用户研究"),
        ("growth-experimenter", "增长实验设计"),
        ("financial-modeler", "财务建模"),
        ("pricing-strategist", "定价策略"),
        ("deep-research", "深度研究"),
        ("competitive-analyst", "竞品分析"),
        ("market-researcher", "市场研究"),
        ("trend-analyst", "趋势分析"),
        ("scientific-researcher", "科学文献研究"),
        ("osint-analyst", "OSINT情报分析"),
        ("research-analyst", "研究分析师"),
        ("content-marketer", "内容营销策略"),
        ("seo-specialist", "SEO专家"),
        ("growth-hacker", "增长黑客"),
        ("social-media-manager", "社媒运营"),
        ("brand-storyteller", "叙事官/品牌故事"),
        ("visual-designer", "视觉设计"),
        ("pitch-coach", "路演/演讲教练"),
        ("copywriter", "文案/口播"),
    ]);

    display.get(id).map(|s| s.to_string()).unwrap_or_else(|| {
        id.split('-')
            .map(|word| {
                let mut w = word.to_string();
                if let Some(first) = w.get(0..1) {
                    w = format!("{}{}", first.to_uppercase(), &w[1..]);
                }
                w
            })
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// GENERIC.md 模板（编译期内嵌）
const GENERIC_TEMPLATE: &str = include_str!("../templates/experts/GENERIC.md");
