use super::*;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct StoredProvider {
    pub(super) id: String,
    pub(super) name: String,
    #[serde(default)]
    pub(super) note: String,
    #[serde(default)]
    pub(super) website_url: String,
    #[serde(default)]
    pub(super) token_field: String,
    /// 上游协议: ""(默认, Anthropic 兼容·claude 直连) | "openai"(OpenAI chat/completions,
    /// 切换时经本地路由翻译转发)。老 store 无此字段 → serde 默认 "" → 行为不变。
    #[serde(default)]
    pub(super) protocol: String,
    #[serde(default)]
    pub(super) settings_config: Value,
}

// 旧版结构 (上一轮), 仅用于一次性迁移
#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct LegacyCustom {
    pub(super) id: String,
    pub(super) name: String,
    #[serde(default)]
    pub(super) base_url: String,
    #[serde(default)]
    pub(super) auth_token: String,
    #[serde(default)]
    pub(super) token_field: String,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct LegacyKey {
    #[serde(default)]
    pub(super) auth_token: String,
    #[serde(default)]
    pub(super) token_field: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Store {
    #[serde(default)]
    pub(super) current_id: String,
    /// true = 联动(切换写 ~/.claude/settings.json, 终端 CLI 跟着变);
    /// false(默认) = 隔离(只作用于 Polaris 自己 spawn 的 claude, 走进程 env)。
    /// 老 store 没有此字段 → serde 默认 false → 升级即自动隔离, 串台就此止住。
    #[serde(default)]
    pub(super) link_global: bool,
    /// 本地路由总开关(cc-switch 式代理模式)。true = Anthropic 兼容供应商也统一走
    /// 127.0.0.1 本地路由透传(key 由路由实时注入, 改 key 即刻生效、不进子进程 env);
    /// false(默认) = 直连(base/key 直接注入环境)。GPT/Codex 等非 Anthropic 协议
    /// 不受此开关影响 —— 它们必须经路由翻译, 恒走路由。
    #[serde(default)]
    pub(super) route_local: bool,
    #[serde(default)]
    pub(super) items: Vec<StoredProvider>,
    // legacy（迁移后清空, 不再写出）
    #[serde(default, skip_serializing)]
    pub(super) custom: Vec<LegacyCustom>,
    #[serde(default, skip_serializing)]
    pub(super) keys: HashMap<String, LegacyKey>,
}

pub(crate) static STORE: Lazy<RwLock<Store>> = Lazy::new(|| RwLock::new(Store::default()));
pub(super) static STORE_PATH: Lazy<RwLock<PathBuf>> = Lazy::new(|| RwLock::new(PathBuf::new()));
/// 串行化对 settings.json / providers.json 的「读-改-写」。
/// Tauri 命令可并发跑在线程池上, 两个 provider_switch 同时进来若不串行化, 会交错写同一份
/// settings.json → 撕裂成半截。此锁保证整条 RMW 原子, 与 atomic_write 一起根治配置损坏。
pub(super) static IO_LOCK: Lazy<parking_lot::Mutex<()>> = Lazy::new(|| parking_lot::Mutex::new(()));

pub fn shared_path() -> Option<PathBuf> {
    let p = STORE_PATH.read().clone();
    (!p.as_os_str().is_empty()).then_some(p)
}
