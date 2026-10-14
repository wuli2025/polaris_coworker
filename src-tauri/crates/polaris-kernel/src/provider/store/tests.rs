use super::*;
#[cfg(test)]
mod volc_ark_tests {
    use super::*;

    #[test]
    fn ark_host_takes_configured_host_but_never_the_local_router() {
        // 正常:照抄用户配的主机名(国内 / 海外两套站点都要认)
        assert_eq!(
            ark_host("https://ark.cn-beijing.volces.com/api/coding", "agentplan"),
            "ark.cn-beijing.volces.com"
        );
        assert_eq!(
            ark_host("https://ark.ap-southeast.bytepluses.com/api/coding", "byteplus"),
            "ark.ap-southeast.bytepluses.com"
        );
        // 开过「本地路由」(或旧配置残留)时 base_url 指向回环 —— 照抄会拼出
        // https://127.0.0.1:port/api/v3/...(在 http 端口上说 TLS), 生图/转写双双打空炮。
        for b in [
            "http://127.0.0.1:8791/p/agentplan",
            "http://localhost:8791/p/agentplan",
            "http://0.0.0.0:8791/p/agentplan",
        ] {
            assert_eq!(
                ark_host(b, "agentplan"),
                "ark.cn-beijing.volces.com",
                "回环地址必须回落到真站点: {b}"
            );
        }
        // 空 / 含变量 → 按 id 回默认站点
        assert_eq!(ark_host("", "agentplan"), "ark.cn-beijing.volces.com");
        assert_eq!(ark_host("", "byteplus"), "ark.ap-southeast.bytepluses.com");
        assert_eq!(
            ark_host("https://${ARK_HOST}/api", "agentplan"),
            "ark.cn-beijing.volces.com"
        );
    }

    #[test]
    fn image_endpoint_uses_v3_not_the_chat_plan_prefix() {
        // base_url 的 `/api/coding` 是**聊天套餐**前缀, 生图必须走 /api/v3 —— 照抄就是 404。
        let host = ark_host("https://ark.cn-beijing.volces.com/api/coding", "agentplan");
        let ep = format!("https://{host}/api/v3/images/generations");
        assert!(!ep.contains("/api/coding"), "不该把聊天套餐前缀带进生图地址: {ep}");
        assert!(ep.ends_with("/api/v3/images/generations"), "{ep}");
    }

    #[test]
    fn local_route_placeholder_token_is_not_a_real_key() {
        // 占位串被当成真 key 借出去的话, 打上游必 401, 而且会把整条借用链堵死
        // (火山优先 → MiniMax 永远轮不到)。这里钉住常量, 防它跟 local_route_config 走散。
        assert_eq!(LOCAL_ROUTE_TOKEN, "polaris-local-router");
        let cfg = local_route_config(8791, "agentplan", "m");
        assert_eq!(
            cfg_env_str(&cfg, DEFAULT_TOKEN_FIELD),
            LOCAL_ROUTE_TOKEN,
            "本地路由写进去的占位串必须与借 key 侧判定的常量是同一个"
        );
    }
}

#[cfg(test)]
mod key_mask_tests {
    use super::*;

    fn save(id: &str, token_field: Option<&str>, cfg: Value) {
        provider_save(ProviderInput {
            id: Some(id.to_string()),
            name: format!("掩码测试 {id}"),
            note: String::new(),
            website_url: String::new(),
            token_field: token_field.map(str::to_string),
            protocol: None,
            settings_config: cfg,
        })
        .expect("保存应成功");
    }

    fn view_of(id: &str) -> ProviderView {
        provider_list()
            .expect("列表应成功")
            .providers
            .into_iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("{id} 应在列表里"))
    }

    fn stored_token(id: &str, field: &str) -> String {
        let store = STORE.read();
        let it = store
            .items
            .iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("{id} 应已存库"));
        cfg_env_str(&it.settings_config, field)
    }

    /// provider_list 出口不得漏出明文 key —— 免口令的 NAS/Docker 壳把这条命令整个开放着。
    #[test]
    fn 列表出口的密钥全是掩码() {
        let id = "t-mask-list";
        save(
            id,
            None,
            json!({"env": {
                "ANTHROPIC_BASE_URL": "https://a.example.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-super-secret-1234",
                "ANTHROPIC_MODEL": "M-1",
            }}),
        );

        let v = view_of(id);
        assert_eq!(v.auth_token, "••••••••1234", "authToken 必须打码");
        assert_eq!(
            cfg_env_str(&v.settings_config, DEFAULT_TOKEN_FIELD),
            "••••••••1234",
            "settingsConfig.env 里的同一把 key 也必须打码(前端就是从这儿回填的)"
        );
        assert!(v.has_key, "打码不能把「配了 key」这个事实一起抹掉");
        // 非密钥字段原样可见 —— 打码不是把整份配置糊掉。
        assert_eq!(cfg_env_str(&v.settings_config, "ANTHROPIC_MODEL"), "M-1");
        assert_eq!(v.base_url, "https://a.example.com");
        // 序列化后整份 JSON 里不该再出现明文。
        let txt = serde_json::to_string(&provider_list().unwrap()).unwrap();
        assert!(
            !txt.contains("sk-super-secret-1234"),
            "provider_list 的报文里出现了明文 key"
        );

        provider_delete(id.to_string()).unwrap();
    }

    /// 编辑别的字段再保存(掩码原样回传)不得把 key 抹成一串圆点 —— provider_save 是整条覆盖写。
    #[test]
    fn 掩码原样回传时保持原_key_不变() {
        let id = "t-mask-roundtrip";
        save(
            id,
            None,
            json!({"env": {
                "ANTHROPIC_BASE_URL": "https://a.example.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-keep-me-abcd",
            }}),
        );
        // 前端拿到的就是打了码的那份;只改模型名后整份回传。
        let mut cfg = view_of(id).settings_config;
        cfg["env"]["ANTHROPIC_MODEL"] = json!("M-2");
        save(id, None, cfg);

        assert_eq!(
            stored_token(id, DEFAULT_TOKEN_FIELD),
            "sk-keep-me-abcd",
            "掩码回传必须还原成原 key"
        );
        assert_eq!(stored_token(id, "ANTHROPIC_MODEL"), "M-2", "其余改动照常生效");

        provider_delete(id.to_string()).unwrap();
    }

    /// 用户重填 / 清空这两条路要照旧生效 —— 掩码回填不能把它们堵死。
    #[test]
    fn 重填换_key_清空删_key() {
        let id = "t-mask-replace";
        save(id, None, json!({"env": {"ANTHROPIC_AUTH_TOKEN": "sk-old-0000"}}));

        // 重填:圆点被整串换成新 key
        save(id, None, json!({"env": {"ANTHROPIC_AUTH_TOKEN": "sk-new-9999"}}));
        assert_eq!(stored_token(id, DEFAULT_TOKEN_FIELD), "sk-new-9999");

        // 清空:前端删掉该 env 字段 → 不走回填, key 真的没了
        save(id, None, json!({"env": {"ANTHROPIC_BASE_URL": "https://a.example.com"}}));
        assert_eq!(stored_token(id, DEFAULT_TOKEN_FIELD), "", "清空必须真的清掉");
        assert!(!view_of(id).has_key);

        provider_delete(id.to_string()).unwrap();
    }

    /// 在弹窗里把 AUTH_TOKEN 切成 API_KEY:前端把掩码搬到新字段名, 同名取不到旧值,
    /// 得按「掩码相同」认领过去, 否则一次换字段就把 key 弄丢。
    #[test]
    fn 切换_token_字段时掩码仍能认领旧值() {
        let id = "t-mask-field-switch";
        save(id, None, json!({"env": {"ANTHROPIC_AUTH_TOKEN": "sk-switch-7777"}}));
        let masked = view_of(id).auth_token;
        assert_eq!(masked, "••••••••7777");

        save(
            id,
            Some(API_KEY_FIELD),
            json!({"env": {"ANTHROPIC_API_KEY": masked}}),
        );
        assert_eq!(
            stored_token(id, API_KEY_FIELD),
            "sk-switch-7777",
            "换字段名后原 key 应搬过去"
        );
        assert_eq!(stored_token(id, DEFAULT_TOKEN_FIELD), "", "旧字段应已让出");

        provider_delete(id.to_string()).unwrap();
    }

    /// 认领不到旧值(新建供应商却传了掩码, 或掩码对不上任何旧值)时宁可留空 ——
    /// 把一串圆点当 key 存进去, 拿去打上游只会 401, 而且用户看不出哪儿错了。
    #[test]
    fn 认领不到旧值的掩码一律剔除而非存进去() {
        let id = "t-mask-orphan";
        save(id, None, json!({"env": {"ANTHROPIC_AUTH_TOKEN": "••••••••0000"}}));
        assert_eq!(stored_token(id, DEFAULT_TOKEN_FIELD), "");
        provider_delete(id.to_string()).unwrap();
    }

    #[test]
    fn 掩码本身的边界() {
        assert_eq!(mask_secret(""), "", "空值仍是空 —— 没配 key 与配了不给看要能区分");
        assert_eq!(mask_secret("   "), "");
        assert_eq!(mask_secret("ab"), "••", "短值不留尾巴(留了等于全给)");
        assert_eq!(mask_secret("abcd"), "••••");
        assert_eq!(mask_secret("abcde"), "••••••••bcde");
        // 本地路由占位串不是秘密, 打码会让「按内容判占位」的借 key 逻辑认不出它。
        assert_eq!(mask_secret(LOCAL_ROUTE_TOKEN), LOCAL_ROUTE_TOKEN);
        assert!(is_masked("••••••••1234"));
        assert!(!is_masked("sk-1234"), "真 key 不含 U+2022, 不能被误判成掩码");
        assert!(!is_masked(LOCAL_ROUTE_TOKEN));
    }
}

#[cfg(test)]
mod local_route_tests {
    use super::*;

    fn save_openai(id: &str, cfg: Value) {
        provider_save(ProviderInput {
            id: Some(id.to_string()),
            name: format!("测试 {id}"),
            note: String::new(),
            website_url: String::new(),
            token_field: None,
            protocol: Some("openai".into()),
            settings_config: cfg,
        })
        .expect("保存应成功");
    }

    /// OpenAI 协议供应商: route_target 解析出上游三元组(尾斜杠归一 + 模型钉选),
    /// cfg_for_view 生成指向本地路由 /p/{id} 的配置 —— 这是本地转发热切换的两半。
    #[test]
    fn openai_provider_routes_via_local_router() {
        save_openai(
            "t-openai-route",
            json!({"env": {
                "ANTHROPIC_BASE_URL": "https://gw.example.com/v1/",
                "ANTHROPIC_AUTH_TOKEN": "sk-test",
                "ANTHROPIC_MODEL": "gpt-5.5-mini",
            }}),
        );

        match route_target("t-openai-route").expect("应解析成功") {
            RouteTarget::OpenAiCompat {
                base_url,
                api_key,
                model,
            } => {
                assert_eq!(base_url, "https://gw.example.com/v1");
                assert_eq!(api_key, "sk-test");
                assert_eq!(model, "gpt-5.5-mini");
            }
            _ => panic!("应是 OpenAiCompat"),
        }

        let store = STORE.read().clone();
        let views = build_views(&store);
        let v = views
            .iter()
            .find(|v| v.id == "t-openai-route")
            .expect("视图应在");
        assert_eq!(v.protocol, "openai");
        let cfg = cfg_for_view(v, false).expect("cfg 应生成(顺带拉起本地路由)");
        let base = cfg_env_str(&cfg, "ANTHROPIC_BASE_URL");
        assert!(
            base.starts_with("http://127.0.0.1:"),
            "应指向本地路由: {base}"
        );
        assert!(base.ends_with("/p/t-openai-route"), "应带 /p/{{id}} 前缀: {base}");
        for k in MODEL_ENV_KEYS {
            assert_eq!(cfg_env_str(&cfg, k), "gpt-5.5-mini", "{k} 应钉到该家模型");
        }

        provider_delete("t-openai-route".to_string()).unwrap();
    }

    #[test]
    fn route_target_fallbacks_and_errors() {
        // 空 id / codex → ChatGPT(兼容旧版裸端口 base_url)
        assert!(matches!(route_target(""), Ok(RouteTarget::Chatgpt)));
        assert!(matches!(route_target("codex"), Ok(RouteTarget::Chatgpt)));
        assert!(route_target("no-such-provider-xyz").is_err());

        // 没配 key 的 openai 供应商 → 拒绝路由
        save_openai(
            "t-openai-nokey",
            json!({"env": {"ANTHROPIC_BASE_URL": "https://gw.example.com"}}),
        );
        assert!(route_target("t-openai-nokey").is_err());

        // Anthropic 兼容供应商 → 透传目标(路由总开关下会被指到 /p/{id}, 目标始终可解析)
        provider_save(ProviderInput {
            id: Some("t-anthropic-direct".into()),
            name: "直连".into(),
            note: String::new(),
            website_url: String::new(),
            token_field: None,
            protocol: None,
            settings_config: json!({"env": {
                "ANTHROPIC_BASE_URL": "https://a.example.com",
                "ANTHROPIC_AUTH_TOKEN": "k",
            }}),
        })
        .unwrap();
        assert!(matches!(
            route_target("t-anthropic-direct"),
            Ok(RouteTarget::AnthropicCompat { .. })
        ));

        provider_delete("t-openai-nokey".to_string()).unwrap();
        provider_delete("t-anthropic-direct".to_string()).unwrap();
    }

    /// 本地路由总开关: Anthropic 兼容供应商 ①route_target 解析成 AnthropicCompat 透传目标;
    /// ②开关开 → cfg 改指 /p/{id}、真 key 摘出换占位、模型钉选保留; ③开关关 → 原配置直连。
    #[test]
    fn route_all_switch_reroutes_anthropic_provider() {
        provider_save(ProviderInput {
            id: Some("t-anthropic-routed".into()),
            name: "透传测试".into(),
            note: String::new(),
            website_url: String::new(),
            token_field: None,
            protocol: None,
            settings_config: json!({"env": {
                "ANTHROPIC_BASE_URL": "https://api.example-relay.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "sk-real-key",
                "ANTHROPIC_MODEL": "SomeModel-X",
            }}),
        })
        .expect("保存应成功");

        match route_target("t-anthropic-routed").expect("应解析成功") {
            RouteTarget::AnthropicCompat {
                base_url,
                api_key,
                token_field,
            } => {
                assert_eq!(base_url, "https://api.example-relay.com/anthropic");
                assert_eq!(api_key, "sk-real-key");
                assert_eq!(token_field, DEFAULT_TOKEN_FIELD);
            }
            _ => panic!("应是 AnthropicCompat"),
        }

        let store = STORE.read().clone();
        let views = build_views(&store);
        let v = views
            .iter()
            .find(|v| v.id == "t-anthropic-routed")
            .expect("视图应在");

        // 开关开: 走本地路由, key 不落 env
        let routed = cfg_for_view(v, true).expect("路由 cfg 应生成");
        let base = cfg_env_str(&routed, "ANTHROPIC_BASE_URL");
        assert!(base.starts_with("http://127.0.0.1:"), "应指向本地路由: {base}");
        assert!(base.ends_with("/p/t-anthropic-routed"));
        assert_eq!(
            cfg_env_str(&routed, "ANTHROPIC_AUTH_TOKEN"),
            "polaris-local-router",
            "真 key 不应进 env"
        );
        assert_eq!(
            cfg_env_str(&routed, "ANTHROPIC_MODEL"),
            "SomeModel-X",
            "模型钉选应原样保留"
        );

        // 开关关: 原配置直连
        let direct = cfg_for_view(v, false).expect("直连 cfg 应生成");
        assert_eq!(
            cfg_env_str(&direct, "ANTHROPIC_BASE_URL"),
            "https://api.example-relay.com/anthropic"
        );
        assert_eq!(cfg_env_str(&direct, "ANTHROPIC_AUTH_TOKEN"), "sk-real-key");

        provider_delete("t-anthropic-routed".to_string()).unwrap();
    }
}

#[cfg(test)]
mod per_conv_scope_tests {
    use super::*;
    use std::collections::HashMap;
    use std::process::Command;

    /// 收集一条 Command 上「显式设置/移除」的 env 覆写: key → Some(值) 表示设了某值,
    /// key → None 表示被 env_remove(阻止从父进程继承)。inherited(没动过的)不出现在这里。
    fn cmd_env_overrides(cmd: &Command) -> HashMap<String, Option<String>> {
        cmd.get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|s| s.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    /// 核心隔离机制: 对话显式选「Claude 官方」时, 即便父进程(全局当前供应商)残留着
    /// ANTHROPIC_BASE_URL/TOKEN, 也会被本条命令逐键 env_remove 顶掉 —— 这条 claude
    /// 因此走官方端点, 与全局开关、与其它并发对话彻底解耦。这正是「每对话隔离」的根。
    #[test]
    fn forced_official_clears_inherited_global_env() {
        // 模拟「全局当前 = 某第三方」在父进程 env 里留下的痕迹
        std::env::set_var(
            "ANTHROPIC_BASE_URL",
            "https://global-thirdparty.example/anthropic",
        );
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "global-token");

        let mut cmd = Command::new("claude");
        scope_child_claude_by_id(&mut cmd, Some("claude-official"));
        let ov = cmd_env_overrides(&cmd);

        // 受管键必须被显式移除(值为 None), 而不是放任继承全局那家
        for k in [
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_MODEL",
        ] {
            assert_eq!(
                ov.get(k),
                Some(&None),
                "受管键 {k} 应被 env_remove 顶掉继承值"
            );
        }
        // 官方档不套私有 CLAUDE_CONFIG_DIR(OAuth 凭据在共享 ~/.claude)
        assert!(
            !ov.contains_key("CLAUDE_CONFIG_DIR"),
            "官方不应改 CLAUDE_CONFIG_DIR"
        );
    }

    /// Auto 档(None/""/"auto"): 不做逐命令注入。全局当前为官方时, scope_child_claude
    /// 提前返回 → 命令上零 env 覆写(纯继承父进程), 行为与旧版完全一致。
    #[test]
    fn auto_is_passthrough_when_global_official() {
        {
            let mut s = STORE.write();
            s.link_global = false;
            s.current_id = "claude-official".to_string();
        }
        for id in [None, Some(""), Some("auto")] {
            let mut cmd = Command::new("claude");
            scope_child_claude_by_id(&mut cmd, id);
            assert!(
                cmd_env_overrides(&cmd).is_empty(),
                "Auto 档({id:?}) 不应在命令上留下任何 env 覆写"
            );
        }
    }
}
