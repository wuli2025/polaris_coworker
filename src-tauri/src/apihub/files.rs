fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// 原子占名 + 创建:`create_new` 一步完成「唯一名探测 + 建文件」。
/// 旧写法先 `exists()` 探测再 `File::create`(截断式),两个并发同名上传会探到同一个
/// 「唯一」名 → 互相截断写花文件;`create_new` 撞名返回 AlreadyExists,递增序号重试即可。
async fn create_unique(base: &Path, fname: &str) -> std::io::Result<(PathBuf, tokio::fs::File)> {
    let stem = Path::new(fname)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = Path::new(fname)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    let mut i = 0u32;
    loop {
        let cand = if i == 0 {
            base.join(fname)
        } else {
            match &ext {
                Some(e) => base.join(format!("{stem}-{i}.{e}")),
                None => base.join(format!("{stem}-{i}")),
            }
        };
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&cand)
            .await
        {
            Ok(f) => return Ok((cand, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                i += 1;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

// ───────────────────────── 受限文件读取（iframe 预览 / 图片）─────────────────────────

#[derive(serde::Deserialize)]
struct FileQuery {
    path: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    download: Option<String>,
}

async fn serve_file(
    State(state): State<ApiState>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Query(q): Query<FileQuery>,
) -> Response {
    let gate = gate_of(&state, peer, &headers);
    let ctx = match app_ctx(&state, &headers, gate).await {
        Some(c) => Some(c),
        None => resolve_app_auth(&state, q.token.clone(), gate).await,
    };
    let Some(ctx) = ctx else {
        return (StatusCode::UNAUTHORIZED, "未授权").into_response();
    };
    // 读文件放到 collaborator/lead(rank>=2):手机端团队成员要能点开对话产物看。
    // 仍然 fail-closed 挡住 visitor/未知角色;真正的边界是下面的 allowed_roots 白名单
    // ——只有知识库根、~/Polaris/{data/artifacts,projects,uploads-inbox} 和项目绑定的
    // 工作目录能读,整机文件系统读不到。逐用户 ACL 做好前不再往下放。
    if role_rank(&ctx.role) < 2 {
        return (StatusCode::FORBIDDEN, "文件访问需要协作者及以上权限").into_response();
    }
    let allowed = allowed_roots();
    let canon = match resolve_readable(&q.path, &allowed) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let file = match tokio::fs::File::open(&canon).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "读取失败").into_response(),
    };
    let stream = futures_util::stream::unfold(file, |mut f| async move {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 64 * 1024];
        match f.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok::<_, std::io::Error>(axum::body::Bytes::from(buf)), f))
            }
            Err(e) => Some((Err(e), f)),
        }
    });
    let mut resp = Body::from_stream(stream).into_response();
    if let Ok(v) = header::HeaderValue::from_str(mime_for(&canon)) {
        resp.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    resp.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    // 浏览器导航 / WebSocket 无法附 Authorization，文件 URL 因而可能带短期 token。
    // 禁止它随预览文件里的外链 Referer 泄到第三方站点。
    resp.headers_mut().insert(
        header::HeaderName::from_static("referrer-policy"),
        header::HeaderValue::from_static("no-referrer"),
    );
    let active_content = matches!(
        canon
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "html" | "htm" | "svg" | "js" | "mjs" | "cjs"
    );
    if q.download.as_deref() == Some("1") || active_content {
        let fname = canon
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("download");
        let cd = format!("attachment; filename*=UTF-8''{}", pct_encode(fname));
        if let Ok(v) = header::HeaderValue::from_str(&cd) {
            resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
        }
    }
    resp
}

/// RFC 5987 百分号编码：unreserved 原样，其余按 UTF-8 字节转 %XX。
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
        }
    }
    out
}

/// 把请求里的 `path` 解析成一个**确实在白名单内**的真实文件。
///
/// 绝对路径照旧直接 canonicalize;新增的是**相对路径 / `~/` 开头 / 裸文件名**也认 ——
/// 手机端「点开对话正文里提到的文件」拿到的往往是助手随手写的相对路径
/// (`docs/报告.md`、`~/Polaris/projects/x/index.html`),按进程 CWD 去解析要么落空、
/// 要么落到意想不到的目录。这里改成逐个白名单根去拼,第一个命中的算数。
///
/// **安全性不变**:无论走哪条,最后一律拿 canonicalize 之后的真实路径复核白名单,
/// `..` 穿越、符号链接逃逸照旧被 `path_contains` 挡住;白名单本身一个字没放宽。
///
/// 实现已下沉 [`crate::beam`](crate::beam::resolve_readable_path) —— 「隔空同屏」的打包器
/// 要用同一把闸,两处各写一份迟早漂移。这里只负责把拒绝原因翻成 HTTP 状态码。
fn resolve_readable(raw: &str, allowed: &[PathBuf]) -> Result<PathBuf, Response> {
    crate::beam::resolve_readable_path(raw, allowed).map_err(|d| match d {
        crate::beam::PathDenied::NotFound => (StatusCode::NOT_FOUND, "文件不存在").into_response(),
        crate::beam::PathDenied::Outside => {
            (StatusCode::FORBIDDEN, "路径不在允许范围").into_response()
        }
    })
}

fn allowed_roots() -> Vec<PathBuf> {
    crate::beam::allowed_roots()
}

pub(crate) fn mime_for(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "md" | "markdown" | "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

// ── 「免口令」的 HTTP 层 e2e ────────────────────────────────────────────────
//
// 单测只能证明判定函数对,证明不了整条链路上真的没人再拦。故这里起真服务发真请求,
// 两种挂法(有/无 ConnectInfo)都跑一遍 —— 反代后面、非 TCP 传输等拿不到对端地址的
// 场景走的正是后者,以前它会被当公网拒掉,那是「打不开」投诉的一大来源。
