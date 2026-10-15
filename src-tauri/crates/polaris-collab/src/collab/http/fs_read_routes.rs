use super::*;

// ───────────────────────── 远程文件浏览(fsface)─────────────────────────
// 隧道另一端的「盘」:owner 鉴权 + 路径关押(POLARIS_FS_ROOTS 白名单)。
// 仅需 collab-host(纯文件访问),桌面 hosting 与 Docker server 壳都挂得上。

/// GET /api/fs/list?path=<rel> → { entries: [...] }
pub(super) async fn fs_list_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if let Err(e) = fs_allowed(&ctx, false) {
        return fs_deny(e);
    }
    let rel = q.get("path").cloned().unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let entries = crate::collab::fsface::list(&rel)?;
        ok(json!({ "entries": entries }))
    })
    .await;
    match out {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => (StatusCode::FORBIDDEN, Json(json!({ "error": e }))).into_response(),
        Err(e) => err_resp(format!("内部任务失败: {e}")),
    }
}

/// 解析单区间 `Range: bytes=a-b` / `bytes=a-` / `bytes=-n`(后缀 n 字节)。
/// 返回 (start, end)(闭区间);None = 头缺失/看不懂(按整文件 200 处理);
/// start 越界由调用方判 416。多区间只取第一段(挂载盘/下载器都只发单段)。
pub(super) fn parse_byte_range(h: Option<&str>, len: u64) -> Option<(u64, u64)> {
    let spec = h?.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    let last = len.saturating_sub(1);
    if a.is_empty() {
        // 后缀区间:最后 n 字节。
        let n: u64 = b.trim().parse().ok()?;
        if n == 0 || len == 0 {
            return None;
        }
        return Some((len - n.min(len), last));
    }
    let start: u64 = a.trim().parse().ok()?;
    let end = if b.trim().is_empty() {
        last
    } else {
        b.trim().parse::<u64>().ok()?.min(last)
    };
    if start > end {
        return None; // 倒置区间 = 无效,按整文件 200 处理(end 越界钳制后也可能落到这)
    }
    Some((start, end))
}

/// 把已 seek 好的文件句柄泵成流式 body:spawn_blocking 分块读 → mpsc → Stream。
/// 内存占用恒定(256KB 块 × 通道容量),文件多大都一样。
pub(super) fn stream_file_body(mut file: std::fs::File, mut remaining: u64) -> axum::body::Body {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(4);
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut buf = vec![0u8; 256 * 1024];
        while remaining > 0 {
            let want = remaining.min(buf.len() as u64) as usize;
            match file.read(&mut buf[..want]) {
                Ok(0) => break, // 文件被并发截短:如实提前收流
                Ok(n) => {
                    remaining -= n as u64;
                    if tx
                        .blocking_send(Ok(axum::body::Bytes::copy_from_slice(&buf[..n])))
                        .is_err()
                    {
                        break; // 客户端断开
                    }
                }
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                    break;
                }
            }
        }
    });
    axum::body::Body::from_stream(futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    }))
}

/// GET /api/fs/read?path=<rel> → 文件字节,**流式 + Range**。
/// 任意大小(不整读进内存);支持断点续传/播放器 seek(206 + Content-Range)。
pub(super) async fn fs_read_api(
    State(state): State<CollabState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers).await else {
        return forbid();
    };
    if let Err(e) = fs_allowed(&ctx, false) {
        return fs_deny(e);
    }
    let rel = q.get("path").cloned().unwrap_or_default();
    let rel_for_ext = rel.clone(); // 判「要不要压缩」看扩展名,而 rel 要被搬进阻塞任务
    let opened =
        tokio::task::spawn_blocking(move || crate::collab::fsface::open_jailed(&rel)).await;
    let (mut file, len, mtime) = match opened {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => return (StatusCode::FORBIDDEN, Json(json!({ "error": e }))).into_response(),
        Err(e) => return err_resp(format!("内部任务失败: {e}")),
    };
    let range_hdr = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    // 解析成功即保证 start ≤ end < len;不可满足/看不懂的区间一律回整文件 200
    //(宁可多传不传错;fs_fetch 端会先比对总长,已完整就不发请求)。
    let range = parse_byte_range(range_hdr.as_deref(), len);
    let (start, end) = range.unwrap_or((0, len.saturating_sub(1)));
    let remaining = if len == 0 { 0 } else { end - start + 1 };
    if start > 0 {
        use std::io::Seek;
        if let Err(e) = file.seek(std::io::SeekFrom::Start(start)) {
            return err_resp(format!("seek 失败: {e}"));
        }
    }
    // 压缩靠一个**私有标记头**开启,而不是标准的 Accept-Encoding —— 这不是洁癖:
    // Chrome 如今默认就发 `Accept-Encoding: ..., zstd`,而「压缩 + Range」对通用客户端
    // 是个坏组合(区间头说的是原文字节,body 却是压缩流,浏览器拿它做 seek 必错)。
    // 只有我们自己的挂载桥懂这套记账,就只让它点名。别的客户端(浏览器/curl/老版本)
    // 看到的行为与从前一字不差。
    //
    // 这台云机 2 核常年 0% 占用,而管子只有 3MB/s:拿闲着的 CPU 换带宽,文本/代码/日志
    // 这类能压掉几倍 —— 实测一份 30MB 日志 10.2s → 0.1s。
    let zstd_on = zstd_level() > 0
        && !is_precompressed(&rel_for_ext)
        && headers
            .get("x-polaris-zstd")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim() == "1")
            .unwrap_or(false);
    let mut b = Response::builder()
        .status(if range.is_some() && len > 0 {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::ACCEPT_RANGES, "bytes");
    // 压缩后长度事先不知道 → 不发 Content-Length,走分块编码。区间头照发**未压缩**的
    // 语义(起止/总长都按原文件算),对端据此记账;它拿到的解压流正好是这段字节。
    if !zstd_on {
        b = b.header(header::CONTENT_LENGTH, remaining);
    }
    if mtime > 0 {
        b = b.header(header::LAST_MODIFIED, httpdate_from_secs(mtime));
    }
    if range.is_some() && len > 0 {
        b = b.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    if zstd_on {
        b = b.header(header::CONTENT_ENCODING, "zstd");
        return b.body(stream_file_body_zstd(file, remaining)).unwrap_or_else(|e| err_resp(e.to_string()));
    }
    b.body(stream_file_body(file, remaining)).unwrap_or_else(|e| err_resp(e.to_string()))
}

/// zstd 压缩级别:0 = 关掉压缩(`POLARIS_FS_ZSTD_LEVEL=0`)。默认 3 —— 在这类机器上
/// 单核也有一百多 MB/s,相对 3MB/s 的管子等于免费,而压缩率比 1 级明显好。
pub(super) fn zstd_level() -> i32 {
    std::env::var("POLARIS_FS_ZSTD_LEVEL")
        .ok()
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(3)
        .clamp(0, 19)
}

/// 已经是压缩格式的文件再压一遍纯属浪费 CPU(还会让本就吃紧的小机器多一份负担)。
/// 判不准也不致命:zstd 对不可压数据是原样存储,顶多白花几个百分点的 CPU。
pub(super) fn is_precompressed(rel: &str) -> bool {
    const EXTS: &[&str] = &[
        "zip", "gz", "tgz", "bz2", "xz", "zst", "7z", "rar", "jpg", "jpeg", "png", "gif", "webp",
        "avif", "heic", "mp3", "aac", "flac", "ogg", "opus", "mp4", "mkv", "mov", "avi", "webm",
        "pdf", "docx", "xlsx", "pptx", "apk", "jar", "whl", "iso", "dmg", "exe", "wasm",
    ];
    let ext = rel
        .rsplit('/')
        .next()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    EXTS.contains(&ext.as_str())
}

/// 同 [`stream_file_body`],但边读边 zstd 压缩。压缩在阻塞池的线程里做,不占 async worker。
pub(super) fn stream_file_body_zstd(file: std::fs::File, remaining: u64) -> axum::body::Body {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(4);
    let level = zstd_level();
    tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        /// 把 zstd 写出来的每一块直接推进通道。通道容量有界 = 天然背压:
        /// 客户端读得慢,压缩线程就在 send 上停下来,内存不会涨。
        struct ChanWriter(tokio::sync::mpsc::Sender<Result<axum::body::Bytes, std::io::Error>>);
        impl Write for ChanWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0
                    .blocking_send(Ok(axum::body::Bytes::copy_from_slice(buf)))
                    .map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::BrokenPipe, "客户端已断开")
                    })?;
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut enc = match zstd::stream::Encoder::new(ChanWriter(tx), level) {
            Ok(e) => e,
            Err(_) => return,
        };
        // 只压区间内的那些字节;copy 自带 8KB 缓冲,压缩器内部另有自己的窗口。
        let mut src = file.take(remaining);
        if std::io::copy(&mut src, &mut enc).is_ok() {
            let _ = enc.finish(); // 收尾帧:少了这一步对端解不出最后一块
        }
    });
    axum::body::Body::from_stream(futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    }))
}

/// unix 秒 → RFC1123(Last-Modified 用)。Hinnant civil-from-days,无额外依赖。
pub(super) fn httpdate_from_secs(secs: u64) -> String {
    const WD: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MO: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let wd = ((days + 4).rem_euclid(7)) as usize;
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as usize;
    let y = yoe as i64 + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        WD[wd],
        d,
        MO[m - 1],
        y,
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
