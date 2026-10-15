use super::*;

// ── Gitea 托管与 /git/* 反代 ──

pub(super) async fn gitea_status_api(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(_ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    Json(crate::collab::gitea::status()).into_response()
}

pub(super) async fn gitea_start_api(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    let out =
        tokio::task::spawn_blocking(|| crate::collab::gitea::start().map(|_| json!({"ok": true})))
            .await;
    unwrap_api(out)
}

pub(super) async fn gitea_stop_api(State(state): State<CollabState>, headers: HeaderMap) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if role_rank(&ctx.role) < 3 {
        return forbid();
    }
    crate::collab::gitea::stop();
    Json(json!({"ok": true})).into_response()
}

/// git smart HTTP 反代(v8 2.3):git 客户端 Basic 认证 = Polaris 用户名 + 会话 token,
/// 验过后**替换**为该用户的 Gitea 访问令牌再转发 localhost:3000。效果:隧道放行表
/// 只有 8080 一个端口,权限两道闸(Polaris 会话 + Gitea 仓库协作者位),Gitea 对外隐身。
/// v1 为缓冲式转发(与 /api/invoke 的 512MB 上限一致);超大仓库首拉建议 LFS 分批。
pub(super) async fn git_proxy(
    State(state): State<CollabState>,
    axum::extract::Path(rest): axum::extract::Path<String>,
    Query(q): Query<HashMap<String, String>>,
    method: axum::http::Method,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // ① 验会话:Basic username:token(git 客户端) 或 Bearer(程序化调用)。
    let ctx = 'auth: {
        if let Some(basic) = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Basic "))
        {
            use base64::Engine;
            if let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(basic) {
                if let Ok(s) = String::from_utf8(raw) {
                    if let Some((user, pass)) = s.split_once(':') {
                        // 同步查库挪阻塞池(与 auth_ctx 同理),git 客户端每请求都走这里。
                        let auth_token = state.auth_token.clone();
                        let pass = pass.to_string();
                        let resolved = tokio::task::spawn_blocking(move || {
                            resolve_auth(&auth_token, Some(pass.as_str()))
                        })
                        .await
                        .ok()
                        .flatten();
                        if let Some(c) = resolved {
                            if c.username == user || c.role == "owner" {
                                break 'auth Some(c);
                            }
                        }
                    }
                }
            }
            None
        } else {
            auth_ctx(&state, &headers).await
        }
    };
    let Some(ctx) = ctx else {
        // 401 + WWW-Authenticate 让 git 客户端弹出凭据询问。
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"polaris-git\"")],
            "认证失败:用户名=Polaris 账号,密码=会话 token",
        )
            .into_response();
    };
    if role_rank(&ctx.role) < 2 {
        return (StatusCode::FORBIDDEN, "访问者角色无 git 读写权").into_response();
    }

    // ② 换发凭据并转发。
    let port: u16 = std::env::var("POLARIS_GITEA_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);
    let qs = if q.is_empty() {
        String::new()
    } else {
        let enc: Vec<String> = q
            .iter()
            .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
            .collect();
        format!("?{}", enc.join("&"))
    };
    // Docker compose 下 Gitea 是独立容器(服务名 gitea);单机托管则是 127.0.0.1。
    let gitea_host = std::env::var("POLARIS_GITEA_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let url = format!("http://{gitea_host}:{port}/{rest}{qs}");
    let username = ctx.username.clone();
    let m = method.as_str().to_string();
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body_vec = body.to_vec();

    let out = tokio::task::spawn_blocking(move || -> Result<(u16, String, Vec<u8>), String> {
        // 每用户 Gitea 凭据(首次自动开号+发 token)。
        crate::collab::gitea::ensure_user(&username)?;
        let token = crate::collab::gitea::user_token(&username)?;
        let mut req = ureq::request(&m, &url)
            .set("Authorization", &format!("token {token}"))
            .timeout(std::time::Duration::from_secs(600));
        if let Some(ct) = &ct {
            req = req.set("Content-Type", ct);
        }
        if let Some(a) = &accept {
            req = req.set("Accept", a);
        }
        // git 协议体是二进制;gzip 让 ureq 自行协商。
        let resp = if body_vec.is_empty() && (m == "GET" || m == "HEAD") {
            req.call()
        } else {
            req.send_bytes(&body_vec)
        };
        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r, // 保留上游错误码语义
            Err(e) => return Err(format!("转发 Gitea 失败: {e}")),
        };
        let status = resp.status();
        let ctype = resp.content_type().to_string();
        let mut buf = Vec::new();
        use std::io::Read;
        resp.into_reader()
            .take(2 * 1024 * 1024 * 1024)
            .read_to_end(&mut buf)
            .map_err(|e| format!("读上游响应失败: {e}"))?;
        Ok((status, ctype, buf))
    })
    .await;

    match out {
        Ok(Ok((status, ctype, buf))) => {
            let mut resp = Response::new(Body::from(buf));
            *resp.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            if let Ok(hv) = ctype.parse() {
                resp.headers_mut().insert(header::CONTENT_TYPE, hv);
            }
            resp
        }
        Ok(Err(e)) => (StatusCode::BAD_GATEWAY, e).into_response(),
        Err(e) => err_resp(e.to_string()),
    }
}
