//! Signed identity assertions and domain-separated authority documents.
use super::*;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Claims {
    /// 签发者标识(权威公钥指纹)。
    pub iss: String,
    /// 全局账号 id —— 跨主机认人**只认这个**。
    pub uid: String,
    pub username: String,
    /// 绑定邮箱。权威强制每个账号都有,故这里必非空。
    pub email: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    pub iat: i64,
    pub exp: i64,
}

/// 签一张身份断言(权威机专用)。格式 `PA1.<b64u(claims)>.<b64u(sig)>`,
/// 签名覆盖 `PA1.<b64u(claims)>` 的 ASCII 字节(前缀纳入签名,防跨版本重放)。
pub fn sign_assertion(
    uid: &str,
    username: &str,
    email: &str,
    display_name: &str,
) -> Result<String, String> {
    let sk = signing_key()?;
    let t = now();
    let claims = Claims {
        iss: kid_of(&B64.encode(sk.verifying_key().to_bytes())),
        uid: uid.into(),
        username: username.into(),
        email: email.into(),
        display_name: display_name.into(),
        device_id: None,
        node_id: None,
        iat: t,
        exp: t + ASSERTION_TTL,
    };
    let payload = serde_json::to_vec(&claims).map_err(|e| format!("断言序列化失败: {e}"))?;
    let head = format!("PA1.{}", B64.encode(payload));
    let sig = sk.sign(head.as_bytes());
    Ok(format!("{head}.{}", B64.encode(sig.to_bytes())))
}

/// 用给定公钥验一张断言。纯本地运算 —— **不联网**,这正是成员主机断网仍能放人进来的原因。
pub fn verify_assertion_with(assertion: &str, pub_b64: &str) -> Result<Claims, String> {
    let mut it = assertion.trim().splitn(3, '.');
    let (p0, p1, p2) = match (it.next(), it.next(), it.next()) {
        (Some(a), Some(b), Some(c)) => (a, b, c),
        _ => return Err("身份断言格式不对".into()),
    };
    if p0 != "PA1" {
        return Err("身份断言版本不认识(须 PA1)".into());
    }
    let pk_bytes: [u8; 32] = B64
        .decode(pub_b64)
        .map_err(|_| "权威公钥不是 base64".to_string())?
        .try_into()
        .map_err(|_| "权威公钥长度不对".to_string())?;
    let vk = VerifyingKey::from_bytes(&pk_bytes).map_err(|e| format!("权威公钥无效: {e}"))?;
    let sig_bytes: [u8; 64] = B64
        .decode(p2)
        .map_err(|_| "断言签名不是 base64".to_string())?
        .try_into()
        .map_err(|_| "断言签名长度不对".to_string())?;
    let sig = Signature::from_bytes(&sig_bytes);
    let head = format!("{p0}.{p1}");
    vk.verify(head.as_bytes(), &sig)
        .map_err(|_| "身份断言签名验证失败(可能被篡改,或权威换了钥匙)".to_string())?;
    let claims: Claims = serde_json::from_slice(
        &B64.decode(p1)
            .map_err(|_| "断言载荷不是 base64".to_string())?,
    )
    .map_err(|e| format!("断言载荷解析失败: {e}"))?;
    let t = now();
    if claims.exp + CLOCK_SKEW < t {
        return Err("身份断言已过期,请重新登录".into());
    }
    if claims.iat - CLOCK_SKEW > t {
        return Err("身份断言的签发时间在未来(检查两端系统时钟)".into());
    }
    if claims.uid.trim().is_empty() {
        return Err("身份断言缺 uid".into());
    }
    Ok(claims)
}


pub(crate) fn sign_document<T: serde::Serialize>(domain: &str, value: &T) -> Result<String, String> {
    let payload = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let head = format!("{domain}.{}", B64.encode(payload));
    Ok(format!("{head}.{}", B64.encode(signing_key()?.sign(head.as_bytes()).to_bytes())))
}

pub(crate) fn verify_document<T: serde::de::DeserializeOwned>(
    domain: &str, document: &str, public_key: &str,
) -> Result<T, String> {
    let parts: Vec<_> = document.split('.').collect();
    if parts.len() != 3 || parts[0] != domain { return Err("签名文档类型错误".into()); }
    let bytes: [u8; 32] = B64.decode(public_key).map_err(|e| e.to_string())?
        .try_into().map_err(|_| "权威公钥长度错误")?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let bytes: [u8; 64] = B64.decode(parts[2]).map_err(|e| e.to_string())?
        .try_into().map_err(|_| "签名长度错误")?;
    key.verify_strict(format!("{}.{}", parts[0], parts[1]).as_bytes(), &Signature::from_bytes(&bytes))
        .map_err(|_| "权威文档签名验证失败")?;
    serde_json::from_slice(&B64.decode(parts[1]).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn sign_device_assertion(uid: &str, node: &str) -> Result<String, String> {
    let device = super::super::interconnect::devices::active(uid, node)?;
    let (username, email, display_name) = super::super::mesh::account_of(uid)?;
    let claims = Claims {
        iss: kid()?, uid: uid.into(), username, email, display_name,
        device_id: Some(device.device_id), node_id: Some(device.node_id),
        iat: now(), exp: now() + ASSERTION_TTL,
    };
    sign_document("PA1", &claims)
}
