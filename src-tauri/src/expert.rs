//! 百人专家团模块 — 运行时动态召集 + 可解释路由
//!
//! 思想来源: WorkBuddy「专家团」+ Kimi Agent Swarm「无预定义角色/运行时召人」
//! Polaris 实现: 专家 = 能力候选池(CLAUDE.md)，运行时按触发信号 RRF 召回，
//! 每次召集给出「为什么是你」理由 + 备选。
//!
//! 入口: expert_list() / expert_route() / expert_match_auto() / expert_apply()

mod expert_groups;
mod avatar_map;
mod expert_docs;

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono::Utc;

/// 专家能力卡 — 一张「能力候选池」卡片，不含任何执行顺序/依赖关系。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExpertCard {
    pub id: String,
    pub name: String,
    /// 图标（emoji 或 SVG path）
    pub icon: String,
    /// 角色定位一句话
    pub role: String,
    /// 详细描述（会嵌入主体 CLAUDE.md）
    pub description: String,
    /// ★为什么选它：命中即解释路由原因（词/短语列表）
    pub trigger_signals: Vec<String>,
    /// ★补哪一维：防同质团队
    pub complements: String,
    /// 关键词（喂 FTS5 trigram 检索）
    pub keywords: Vec<String>,
    /// 能力权限列表
    pub capabilities: Vec<String>,
    /// CLAUDE.md 模板路径（编译期内嵌）
    pub claude_md_ref: String,
    /// 推荐模型 hint
    pub model_hint: String,
    /// 成本档: 1=便宜路由/初筛, 2=中档专业活, 3=贵档深度推理
    pub cost_tier: u8,
    /// 互斥列表（同质专家同进会增加协调成本）
    pub exclusive_with: Vec<String>,
    /// 来源仓库
    pub source: String,
    /// 许可
    pub license: String,
    /// 专家分组
    pub group: String,
}

/// 路由结果 — 包含推荐理由
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExpertMatch {
    /// 专家卡片引用
    pub expert: ExpertCard,
    /// 命中信号（子任务里出现的触发词）
    pub hit_signals: Vec<String>,
    /// 相似度分（0.0 ~ 1.0）
    pub similarity: f32,
    /// 补的维度
    pub complements: String,
    /// 是否是主选（false=备选）
    pub is_primary: bool,
}

/// 单专家活跃状态
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExpertAgentStatus {
    pub expert_id: String,
    pub name: String,
    pub status: String, // "idle" | "working" | "done"
    pub last_active: String,
}

/// 对话模式
#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ChatMode {
    /// 单 agent（无专家加成，最便宜）
    SingleAgent,
    /// 单专家（从花名册选一个）
    SingleExpert,
    /// 专家团（战略师领衔，按需组阵）
    ExpertTeam,
    /// 智能匹配（一句话描述需求，自动路由到最合适专家）
    AutoMatch,
}

impl Default for ChatMode {
    fn default() -> Self {
        // 默认自动匹配专家
        ChatMode::AutoMatch
    }
}

/// 路由请求
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRequest {
    pub query: String,
    /// 最多返回多少个（默认 5，含主选+备选）
    pub limit: Option<usize>,
    /// 指定分组过滤（如 "系统架构"）
    pub group_filter: Option<String>,
}

/// ───────────────────────── 100 专家花名册 ─────────────────────────

fn all_experts() -> Vec<ExpertCard> {
    expert_groups::build_experts()
}

/// 专家团状态表: project_id -> Vec<ExpertAgentStatus>
/// 线程安全
static EXPERT_TEAMS: once_cell::sync::Lazy<Arc<Mutex<HashMap<String, Vec<ExpertAgentStatus>>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// 判断任务是否需要多专家（并行/分工/组队检测）
pub fn detect_multi_expert_task(task: &str) -> bool {
    let t = task.to_lowercase();

    // 并行关键词
    let parallel_kw = ["并行", "同时", "分别", "各自", "拆成", "分工", "团队", "组队", "多人", "多步"];
    for kw in &parallel_kw {
        if t.contains(*kw) {
            return true;
        }
    }

    // 列表式任务: 3+ 子任务以换行/bullet 分割
    let lines: Vec<_> = task
        .split(|c| c == '\n' || c == '\r')
        .filter(|l| !l.trim().is_empty())
        .collect();
    // 统计看起来像子任务项的行(以 bullet/数字/顿号开头)
    let bullet_count = lines.iter().filter(|l| {
        let l = l.trim();
        l.starts_with('-') || l.starts_with('*') || l.starts_with('·')
            || l.starts_with('●') || l.starts_with('○')
            || (l.len() > 1 && l.chars().next().unwrap().is_numeric())
            || l.starts_with('1') || l.starts_with('2') || l.starts_with('3')
            || l.starts_with('①') || l.starts_with('②') || l.starts_with('③')
    }).count();

    bullet_count >= 3 || lines.len() >= 3
}

/// 召集专家团
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_team_spawn(
    project_id: String,
    task_description: String,
) -> Vec<ExpertMatch> {
    let matches = expert_route(RouteRequest {
        query: task_description.clone(),
        limit: Some(5),
        group_filter: None,
    });

    // 初始化项目团队状态
    {
        let mut teams = EXPERT_TEAMS.lock().unwrap();
        if !teams.contains_key(&project_id) {
            let initial: Vec<ExpertAgentStatus> = matches
                .iter()
                .map(|m| ExpertAgentStatus {
                    expert_id: m.expert.id.clone(),
                    name: m.expert.name.clone(),
                    status: "idle".into(),
                    last_active: Utc::now().to_rfc3339(),
                })
                .collect();
            teams.insert(project_id.clone(), initial);
        }
    }

    // 标记主选/备选: 前2名为主选(is_primary=true)，其余备选
    let mut result = Vec::new();
    for (i, m) in matches.into_iter().enumerate() {
        let mut m = m;
        m.is_primary = i < 2;
        result.push(m);
    }
    result
}

/// 查询项目当前专家团状态
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_agents_status(project_id: String) -> Vec<ExpertAgentStatus> {
    let teams = EXPERT_TEAMS.lock().unwrap();
    teams.get(&project_id).cloned().unwrap_or_default()
}

/// 全量专家列表
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_list() -> Vec<ExpertCard> {
    all_experts()
}

/// 按分组获取专家
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_list_by_group(group: String) -> Vec<ExpertCard> {
    all_experts()
        .into_iter()
        .filter(|e| e.group == group)
        .collect()
}

/// 全部分组列表
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_groups() -> Vec<ExpertGroup> {
    vec![
        ExpertGroup { id: "orchestration".into(), name: "编排/统帅".into(), icon: "🧭".into(), count: 5 },
        ExpertGroup { id: "system_arch".into(), name: "系统架构".into(), icon: "🏛".into(), count: 10 },
        ExpertGroup { id: "language".into(), name: "语言专精".into(), icon: "⌨".into(), count: 12 },
        ExpertGroup { id: "frontend".into(), name: "前端/移动".into(), icon: "📱".into(), count: 6 },
        ExpertGroup { id: "devops".into(), name: "DevOps/基础设施".into(), icon: "⚙".into(), count: 9 },
        ExpertGroup { id: "data".into(), name: "数据".into(), icon: "📊".into(), count: 8 },
        ExpertGroup { id: "ai_ml".into(), name: "AI/机器学习".into(), icon: "🧠".into(), count: 7 },
        ExpertGroup { id: "security".into(), name: "安全/合规".into(), icon: "🛡".into(), count: 7 },
        ExpertGroup { id: "quality".into(), name: "质量/治理".into(), icon: "🔬".into(), count: 7 },
        ExpertGroup { id: "specialty".into(), name: "专项技术".into(), icon: "🧩".into(), count: 6 },
        ExpertGroup { id: "docs".into(), name: "文档/技术写作".into(), icon: "📝".into(), count: 5 },
        ExpertGroup { id: "product".into(), name: "产品/项目/战略".into(), icon: "📐".into(), count: 8 },
        ExpertGroup { id: "research".into(), name: "研究/分析".into(), icon: "🔎".into(), count: 7 },
        ExpertGroup { id: "marketing".into(), name: "营销/内容".into(), icon: "📣".into(), count: 8 },
    ]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertGroup {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub count: usize,
}

/// 专家路由 — RRF 召回 + 信号命中
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_route(req: RouteRequest) -> Vec<ExpertMatch> {
    let limit = req.limit.unwrap_or(5);
    let query_lower = req.query.to_lowercase();
    let experts = all_experts();

    // 两路召回
    let signal_scores = signal_match_score(&query_lower, &experts);
    let keyword_scores = keyword_match_score(&query_lower, &experts);

    // RRF 融合（k=60，业界标准参数）
    let rrf_scores = rrf_fuse(&signal_scores, &keyword_scores, 60.0);

    // 排序取 top N
    let mut sorted: Vec<_> = rrf_scores.iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Less));

    sorted
        .into_iter()
        .take(limit)
        .map(|(id, score)| {
            // find the expert by id
            let expert = experts.iter().find(|e| &e.id == id).unwrap();
            let hit_signals = find_hit_signals(expert, &query_lower);
            let is_primary = *score > 0.6;
            ExpertMatch {
                expert: expert.clone(),
                hit_signals,
                similarity: *score,
                complements: expert.complements.to_string(),
                is_primary,
            }
        })
        .collect()
}

/// 单专家路由（指定 id）
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_get(id: String) -> Option<ExpertCard> {
    all_experts().into_iter().find(|e| e.id == id)
}

/// 智能匹配 — 根据用户描述自动路由最合适专家
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_match_auto(query: String) -> Vec<ExpertMatch> {
    expert_route(RouteRequest {
        query,
        limit: Some(3),
        group_filter: None,
    })
}

/// 返回专家头像的 base64 Data URL（供前端 <img src=""> 直接使用）
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_avatar(id: String) -> Option<String> {
    let filename = avatar_map::avatar_filename(&id)?;
    // 头像 PNG 位于 src-tauri/src/templates/experts/avatars/
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // src-tauri
        .unwrap()
        .join("src")
        .join("templates")
        .join("experts")
        .join("avatars")
        .join(filename);
    let png_bytes = std::fs::read(base).ok()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Some(format!("data:image/png;base64,{}", b64))
}

/// 把某专家的 CLAUDE.md 模板应用到指定项目：写入该项目 CLAUDE.md + 记录 persona_id。
/// `overwrite=false` 且已有非占位内容时拒绝覆盖（交前端二次确认后再 true）。
///
/// 专家团预设（如 "team-general"）会写成战略师领衔的编排型 CLAUDE.md，
/// 与 persona_apply 走同一条写 CLAUDE.md 链路；区别是 expert_apply 读模板文件，
/// persona_apply 用编译期内嵌的 preset body。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn expert_apply(
    project_id: String,
    expert_id: String,
    overwrite: bool,
) -> Result<(), String> {
    // 查找专家
    let expert = all_experts()
        .into_iter()
        .find(|e| e.id == expert_id)
        .ok_or_else(|| format!("未知专家: {}", expert_id))?;

    // 用专家元数据构建完整的 CLAUDE.md 正文
    let body = expert_docs::build_expert_doc(
        &expert.claude_md_ref,
        &expert.name,
        &expert.role,
        &expert.description,
        &expert.keywords,
        &expert.capabilities,
        &expert.trigger_signals,
        &expert.complements,
        &expert.exclusive_with,
        expert.cost_tier,
    )
    .ok_or_else(|| format!("专家模板构建失败: {}", expert.claude_md_ref))?;

    // 项目 CLAUDE.md 路径（复用人格模块的同一路径）
    let path = project_claude_md_path(&project_id).ok_or("无法确定项目路径")?;
    if !overwrite && path.exists() {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        if !existing.trim().is_empty()
            && !existing.contains(crate::claude_md::PLACEHOLDER_MARKER)
        {
            return Err("该项目已有人格内容，确认覆盖请重试。".into());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, body).map_err(|e| e.to_string())?;

    // 记录到项目状态（与 persona_apply 共用同一个状态字段）
    crate::conv::set_project_persona(&project_id, Some(expert_id.clone()), None);
    Ok(())
}

/// 项目 CLAUDE.md 路径（须与 persona::project_claude_md_path 一致）
fn project_claude_md_path(project_id: &str) -> Option<std::path::PathBuf> {
    use directories::UserDirs;
    if !crate::conv::is_safe_project_id(project_id) {
        return None;
    }
    let user = UserDirs::new()?;
    Some(
        user.home_dir()
            .join("Polaris")
            .join("projects")
            .join(project_id)
            .join("CLAUDE.md"),
    )
}

// ───────────────────────── 路由算法 ─────────────────────────

fn signal_match_score(query: &str, experts: &[ExpertCard]) -> HashMap<String, f32> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    for expert in experts {
        let mut hits = 0;
        for signal in &expert.trigger_signals {
            if query.contains(&signal.to_lowercase()) {
                hits += 1;
            }
        }
        if hits > 0 {
            let raw = hits as f32 / expert.trigger_signals.len() as f32;
            scores.insert(expert.id.clone(), raw);
        }
    }
    scores
}

fn keyword_match_score(query: &str, experts: &[ExpertCard]) -> HashMap<String, f32> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    for expert in experts {
        let mut hits = 0;
        for kw in &expert.keywords {
            if query.contains(&kw.to_lowercase()) {
                hits += 1;
            }
        }
        if hits > 0 {
            scores.insert(expert.id.clone(), hits as f32 * 0.3);
        }
    }
    scores
}

/// RRF (RecipRank Fusion) — 两路分数融合
fn rrf_fuse(signal_scores: &HashMap<String, f32>, keyword_scores: &HashMap<String, f32>, k: f32) -> HashMap<String, f32> {
    let mut combined: HashMap<String, f32> = HashMap::new();

    // 信号路排名
    let mut signal_rank: Vec<_> = signal_scores.iter().collect();
    signal_rank.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Less));
    for (rank, (id, score)) in signal_rank.iter().enumerate() {
        let rrf = 1.0 / (k + (rank + 1) as f32);
        let e = combined.entry((*id).clone()).or_insert(0.0);
        *e += rrf * *score;
    }

    // 关键词路排名
    let mut kw_rank: Vec<_> = keyword_scores.iter().collect();
    kw_rank.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Less));
    for (rank, (id, score)) in kw_rank.iter().enumerate() {
        let rrf = 1.0 / (k + (rank + 1) as f32);
        let e = combined.entry((*id).clone()).or_insert(0.0);
        *e += rrf * *score;
    }

    combined
}

fn find_hit_signals(expert: &ExpertCard, query: &str) -> Vec<String> {
    expert
        .trigger_signals
        .iter()
        .filter(|s| query.contains(&s.to_lowercase()))
        .cloned()
        .collect()
}

// ───────────────────────── Tauri commands ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_experts_count() {
        let count = all_experts().len();
        assert!(count >= 100, "专家数量应 >= 100，实际 {}", count);
    }

    #[test]
    fn routing_returns_results() {
        let results = expert_route(RouteRequest {
            query: "帮我做一个带支付的 SaaS 落地页，要好看，并发上线".into(),
            limit: Some(5),
            group_filter: None,
        });
        assert!(!results.is_empty(), "路由应返回结果");
        for r in &results {
            assert!(!r.expert.trigger_signals.is_empty(), "{} 缺 trigger_signals", r.expert.id);
        }
    }

    #[test]
    fn auto_match_returns_primary() {
        let results = expert_match_auto("帮我做个支付功能".into());
        assert!(!results.is_empty());
        // 主选应该有较高的相似度
        let primary = &results[0];
        assert!(primary.similarity > 0.0);
    }

    #[test]
    fn all_experts_have_unique_ids() {
        let ids: Vec<_> = all_experts().iter().map(|e| e.id.clone()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "专家 id 不应重复");
    }

    #[test]
    fn expert_groups_complete() {
        let groups = expert_groups();
        let total: usize = groups.iter().map(|g| g.count).sum();
        assert_eq!(total, all_experts().len(), "分组计数应等于专家总数");
    }
}
