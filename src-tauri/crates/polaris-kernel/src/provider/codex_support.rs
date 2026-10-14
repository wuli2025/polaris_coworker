use super::*;
/// 按官方 codex CLI 格式写 ~/.codex/auth.json, 外部 `codex` CLI 可直接复用
pub(super) fn codex_write_auth_json(
    tokens: &CodexTokenResp,
    refresh_token: &str,
    account_id: Option<&str>,
) -> Result<(), String> {
    let path = codex_auth_path().ok_or_else(|| "无法定位用户主目录".to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 ~/.codex 目录失败: {e}"))?;
    }

    let auth = json!({
        "OPENAI_API_KEY": Value::Null,
        // 现行 codex CLI / 社区插件(llm-openai-via-codex)按 auth_mode 区分 ChatGPT 订阅
        // 与纯 API key 登录; 缺它会被判成 API-key 模式而拒用订阅额度。务必写 "chatgpt"。
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": tokens.id_token.clone().unwrap_or_default(),
            "access_token": tokens.access_token,
            "refresh_token": refresh_token,
            "account_id": account_id.unwrap_or_default(),
        },
        "last_refresh": codex_rfc3339_now(),
    });

    let content =
        serde_json::to_string_pretty(&auth).map_err(|e| format!("序列化 auth.json 失败: {e}"))?;
    // auth.json 含 refresh/access/id token:① 原子写防写一半撕裂 → 外部 codex CLI 读到坏 JSON;
    // ② Unix 下收紧到 0600,NAS/Docker 多用户主机上不让同机其他用户读走凭证。
    atomic_write(&path, &content).map_err(|e| format!("写入 ~/.codex/auth.json 失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// 当前 UTC 时间的 RFC3339 字符串 (codex CLI 解析 last_refresh 用), 不引 chrono
pub(crate) fn codex_rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant 的 civil_from_days 算法
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// 打开系统默认浏览器到指定 URL (跨平台)
pub(crate) fn codex_open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // rundll32 不解析 &,URL 原样透传(cmd start 会在 & 处截断 —— OAuth 授权 URL
        // 含多个 & 参数,截断后授权页直接报错)。CREATE_NO_WINDOW(0x0800_0000): 别闪黑窗。
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
