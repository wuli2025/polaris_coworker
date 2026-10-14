use super::*;

// ───────────────────────── 开关 ─────────────────────────

/// 常驻 agent 总开关: 默认开; POLARIS_PERSISTENT_AGENT=0 回落旧的每消息 spawn 路径
/// (旧路径完整保留在 pipeline.rs, 排障/对比随时可切)。
pub(crate) fn persistent_enabled() -> bool {
    std::env::var("POLARIS_PERSISTENT_AGENT")
        .map(|v| v.trim() != "0")
        .unwrap_or(true)
}

// ───────────────────────── PromptPlan ─────────────────────────

/// prompt 分段: 首轮/续轮的拆分依据。
/// - `Session`: 会话级不变(KB 铁律/风格/CLAUDE.md/KB 概览/记忆地图/对话历史) ——
///   claude 进程内自持上下文, 续轮重发只是烧 token + 稀释注意力, 剔除;
/// - `Keyed`: 意图门控块(skill/脚本公约/下载公约等), 本轮触发且**本会话还没发过**
///   才发(按 key 去重) —— 首轮没触发、第 5 轮才触发的公约不能因"续轮剔除"而永远缺席;
/// - `Turn`: 每轮都变(KB 召回增量/手改产物提醒/专家块/用户问题), 续轮照发。
pub(crate) enum Seg {
    Session(String),
    Keyed(String, String),
    Turn(String),
}

/// 有序分段列表: 旧路径拼接顺序原样保留(full_text = 逐段连接), 常驻续轮按
/// 分段类别取舍(continuation_text)。
#[derive(Default)]
pub(crate) struct PromptPlan {
    segs: Vec<Seg>,
}

impl PromptPlan {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn session(&mut self, s: String) {
        self.segs.push(Seg::Session(s));
    }
    pub(crate) fn keyed(&mut self, key: impl Into<String>, s: String) {
        self.segs.push(Seg::Keyed(key.into(), s));
    }
    pub(crate) fn turn(&mut self, s: String) {
        self.segs.push(Seg::Turn(s));
    }

    /// 全量文本: 旧路径每轮用它; 常驻路径首轮(含进程重建后第一轮)用它。
    pub(crate) fn full_text(&self) -> String {
        let mut out = String::new();
        for seg in &self.segs {
            match seg {
                Seg::Session(s) | Seg::Turn(s) => out.push_str(s),
                Seg::Keyed(_, s) => out.push_str(s),
            }
        }
        out
    }

    /// 续轮文本: 剔除会话级不变段与已发过的门控块, 相对顺序保持不变。
    pub(crate) fn continuation_text(&self, sent: &HashSet<String>) -> String {
        let mut out = String::new();
        for seg in &self.segs {
            match seg {
                Seg::Session(_) => {}
                Seg::Keyed(k, s) => {
                    if !sent.contains(k) {
                        out.push_str(s);
                    }
                }
                Seg::Turn(s) => out.push_str(s),
            }
        }
        out
    }

    /// 本轮出现的全部门控 key(写入成功后并入会话的 sent_keys)。
    pub(crate) fn keys(&self) -> Vec<String> {
        self.segs
            .iter()
            .filter_map(|s| match s {
                Seg::Keyed(k, _) => Some(k.clone()),
                _ => None,
            })
            .collect()
    }
}

// ───────────────────────── 指纹 ─────────────────────────

/// 会话配置指纹: 这些量全部烙在 claude 进程的**启动参数/环境**里(--permission-mode/
/// --allowedTools/--model/--add-dir/供应商 env), 常驻进程无法中途改 → 任一变化只能
/// 杀旧重спавн。creative 也入指纹: 它决定 KB 铁律/回答风格这两段会话级前置的取向,
/// 中途翻转时靠重建让新前置以"首条"身份生效(创作/非创作切换很少见, 偶付一次 6.4s 可接受)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fingerprint {
    pub perm: String,
    pub provider_id: String,
    pub work_full: bool,
    pub with_task: bool,
    pub model: String,
    pub add_dirs: Vec<String>,
    pub creative: bool,
    /// 本项目绑定的工作目录(claude cwd)。cwd 是 spawn 时钉死、进程中途改不了的, 故必须入
    /// 指纹: 用户切换项目工作目录后, 旧常驻进程还在老 cwd 里跑, 判变才会杀旧重建。空=默认 cwd。
    pub cwd: String,
}

// ───────────────────────── PoolCore (纯记账, 可测) ─────────────────────────

