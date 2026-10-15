//! Authority selection and preconfigured trust pins.
use super::*;

/// 当前模式。两个环境变量都设时以 Authority 优先 —— 权威机不该再去问别人。
pub fn mode() -> Mode {
    let is_authority = std::env::var("POLARIS_ACCOUNT_AUTHORITY")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if is_authority {
        return Mode::Authority;
    }
    match super::super::interconnect::load() {
        Ok(config) => match config.authority.url {
            Some(url) => Mode::Delegated(url),
            None => Mode::Local,
        },
        // Invalid configuration must never enable local account creation. Requests
        // validate load() again and return the original error before network I/O.
        Err(_) => Mode::Delegated(String::new()),
    }
}

pub fn is_authority() -> bool {
    mode() == Mode::Authority
}

/// 成员主机模式下的权威地址(其余模式 None)。
pub fn upstream_url() -> Option<String> {
    match mode() {
        Mode::Delegated(u) => Some(u),
        _ => None,
    }
}

/// Existing installations may keep their historical pin with placeholder defaults.
/// Once fingerprints are configured, every cached/offline verification checks them too.
fn validate_cached(pk: &str) -> Result<(), String> {
    let config = super::super::interconnect::load()?;
    if !config.authority.public_key_fingerprints.is_empty() {
        config.verify_key(pk)?;
    } else if config.authority.url.is_some() {
        return Err("已配置权威地址但缺少预置公钥指纹".into());
    }
    Ok(())
}

/// 已钉住的权威 (url, pub_b64, kid)。没钉过返回 None。
pub fn pinned() -> Option<(String, String, String)> {
    let url = db::meta_get("authority_url")?;
    let pk = db::meta_get("authority_pub")?;
    if url.is_empty() || pk.is_empty() {
        return None;
    }
    let kid = db::meta_get("authority_kid").unwrap_or_else(|| kid_of(&pk));
    Some((url, pk, kid))
}

/// 向权威取公钥。只在需要钉/换钉时调用,登录热路径不走这里。
fn fetch_public_key(url: &str) -> Result<String, String> {
    let config = super::super::interconnect::load()?;
    if config.authority.public_key_fingerprints.is_empty() {
        return Err("未配置权威公钥指纹，拒绝首次信任".into());
    }
    super::super::interconnect::validate_url(url)?;
    let ep = format!("{}/api/account/pubkey", url.trim_end_matches('/'));
    let resp = ureq::get(&ep)
        .timeout(std::time::Duration::from_secs(UPSTREAM_TIMEOUT_SECS))
        .call()
        .map_err(|e| format!("连不上账号权威 {url}: {e}"))?;
    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("账号权威返回的不是 JSON: {e}"))?;
    let pk = v
        .get("publicKey")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    if pk.is_empty() {
        return Err("账号权威没返回公钥 —— 确认该地址开了 POLARIS_ACCOUNT_AUTHORITY=1".into());
    }
    config.verify_key(&pk)?;
    Ok(pk)
}

/// 确保已钉住当前配置的权威公钥,返回 (url, pub_b64)。
///
/// 首次接触先匹配预置指纹再钉死。此后**公钥变了一律拒绝**,不静默接受 —— 静默接受等于把
/// 「中间人换一把自己的钥匙」变成无声事件,那整套验签就白做了。真要换钥匙(权威轮换密钥、
/// 换了云机)走 `repin`,由 owner 显式确认。
pub fn ensure_pinned() -> Result<(String, String), String> {
    super::super::interconnect::load()?;
    let url = upstream_url().ok_or("本机未配置账号权威(POLARIS_ACCOUNT_AUTHORITY_URL)")?;
    if let Some((pinned_url, pk, _)) = pinned() {
        if pinned_url == url {
            validate_cached(&pk)?;
            return Ok((url, pk));
        }
        // 换了权威地址:同样按「显式确认」处理,不静默改钉。
        return Err(format!(
            "本机已钉在账号权威「{pinned_url}」,与当前配置「{url}」不一致 —— 请 owner 走「重新信任」确认后再切换"
        ));
    }
    let pk = fetch_public_key(&url)?;
    let k = kid_of(&pk);
    db::meta_set_many(&[("authority_url", &url), ("authority_pub", &pk), ("authority_kid", &k)])?;
    db::audit("system", "authority.pin", &url, &k);
    Ok((url, pk))
}

/// 用**显式给的地址**钉住权威(不看环境变量)。
///
/// 为什么需要它:`ensure_pinned` 的地址来自 `POLARIS_ACCOUNT_AUTHORITY_URL`,而桌面 App 是
/// 双击启动的 —— 那个环境变量根本不存在。于是桌面主机永远钉不上权威,也就**永远不接受
/// 身份断言**:同账号的另一台设备连过来会吃一句「本机未配置云端账号中心」。设备网入网时
/// 用户亲手填了地址、输了密码,正是钉住它的合法时机。
///
/// 语义与 `ensure_pinned` 一致的那一半:已经钉过**同一个** URL 就直接复用;钉过**别的**
/// URL 一律拒绝,不静默改钉 —— 静默换钥匙等于把验签这道防线拆了。真要换走 [`repin`]。
pub fn pin_explicit(url: &str) -> Result<(String, String), String> {
    let url = url.trim().trim_end_matches('/').to_string();
    let config = super::super::interconnect::load()?;
    if config.authority.url.as_ref().is_some_and(|configured| configured != &url) {
        return Err("账号中心地址与互联配置不符，请先更新配置再登录".into());
    }
    if url.is_empty() {
        return Err("账号中心地址不能为空".into());
    }
    if let Some((pinned_url, pk, _)) = pinned() {
        if pinned_url == url {
            validate_cached(&pk)?;
            return Ok((url, pk));
        }
        return Err(format!(
            "本机已信任账号中心「{pinned_url}」,与这次要用的「{url}」不一致 —— \
             要换请走「重新信任」并核对公钥指纹"
        ));
    }
    let pk = fetch_public_key(&url)?;
    let k = kid_of(&pk);
    db::meta_set_many(&[("authority_url", &url), ("authority_pub", &pk), ("authority_kid", &k)])?;
    db::audit("system", "authority.pin", &url, &k);
    Ok((url, pk))
}

/// owner 显式重钉(换云机/轮换密钥)。要求调用方已核过 kid。
pub fn repin(expect_kid: Option<&str>) -> Result<(String, String), String> {
    let url = upstream_url().ok_or("本机未配置账号权威(POLARIS_ACCOUNT_AUTHORITY_URL)")?;
    let pk = fetch_public_key(&url)?;
    let k = kid_of(&pk);
    if let Some(want) = expect_kid {
        if !want.trim().is_empty() && want.trim() != k {
            return Err(format!("公钥指纹对不上:权威给的是 {k},你要确认的是 {want}"));
        }
    }
    db::meta_set_many(&[("authority_url", &url), ("authority_pub", &pk), ("authority_kid", &k)])?;
    db::audit("owner", "authority.repin", &url, &k);
    Ok((url, pk))
}

/// 验一张断言(成员主机侧:用钉住的公钥)。
///
/// 还没钉过就先钉一次(首次接触要连一下权威取公钥)。**只有这一次需要网络** ——
/// 钥匙钉下之后本机就能独立验签,权威挂了、家里断网了都不影响成员登录。
pub fn verify_assertion(assertion: &str) -> Result<Claims, String> {
    let pk = match pinned() {
        Some((_, pk, _)) => pk,
        None => match ensure_pinned() {
            Ok((_, pk)) => pk,
            Err(e) => {
                return Err(if upstream_url().is_some() {
                    format!("本机首次信任云端账号中心需要连一次它:{e}")
                } else {
                    "本机未配置云端账号中心(POLARIS_ACCOUNT_AUTHORITY_URL),不接受身份断言".into()
                })
            }
        },
    };
    validate_cached(&pk)?;
    verify_assertion_with(assertion, &pk)
}

