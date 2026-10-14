/// 浏览器拖拽/选择文件 → 存到服务端临时目录 → 返回服务端绝对路径列表。
async fn upload(
    State(state): State<ApiState>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let gate = gate_of(&state, peer, &headers);
    let Some(ctx) = app_ctx(&state, &headers, gate).await else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"未授权"}))).into_response();
    };
    if role_rank(&ctx.role) < 3 {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"文件上传需要 owner 权限"})),
        )
            .into_response();
    }
    let base = upload_dir();
    if let Err(e) = std::fs::create_dir_all(&base) {
        return err_resp(format!("创建上传目录失败: {e}"));
    }
    use tokio::io::AsyncWriteExt;
    let mut saved: Vec<Value> = Vec::new();
    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return err_resp(format!("上传流中断: {e}")),
        };
        let fname = field
            .file_name()
            .map(sanitize_filename)
            .unwrap_or_else(|| "upload.bin".to_string());
        let (dst, mut f) = match create_unique(&base, &fname).await {
            Ok(v) => v,
            Err(e) => return err_resp(format!("创建上传文件失败: {e}")),
        };
        let mut size: u64 = 0;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    size += chunk.len() as u64;
                    if let Err(e) = f.write_all(&chunk).await {
                        drop(f);
                        let _ = tokio::fs::remove_file(&dst).await;
                        return err_resp(format!("写入上传文件失败: {e}"));
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    drop(f);
                    let _ = tokio::fs::remove_file(&dst).await;
                    return err_resp(format!("读取上传字段失败: {e}"));
                }
            }
        }
        if let Err(e) = f.flush().await {
            return err_resp(format!("写入上传文件失败: {e}"));
        }
        saved.push(json!({
            "name": fname,
            "path": dst.to_string_lossy().replace('\\', "/"),
            "size": size,
        }));
    }
    Json(json!({ "files": saved })).into_response()
}

fn upload_dir() -> PathBuf {
    if let Some(u) = directories::UserDirs::new() {
        u.home_dir().join("Polaris").join("uploads-inbox")
    } else {
        PathBuf::from("/tmp/polaris-uploads")
    }
}

// ───────────────────────── 远端数据面的附件路径闸 ─────────────────────────
//
// `chat_attach_files` 收的是**主机绝对路径**, 并把该路径的文件整份**拷进会话 uploads 目录** ——
// 而那个目录正落在 `artifact_read` 的放行范围内。于是「随便给个路径 → 拷进对话 → 读回来」
// 就把 2026-07-29 给文件中心补的那道闸(fable/files/mod.rs)整个绕过去了, 同族同因。
//
// 闸只加在 HTTP 数据面(手机 / Docker·NAS server 壳)。桌面壳的前端走 Tauri 命令直调,
// 根本不经过 apihub —— 拖拽磁盘上任意文件当附件照常, 一点不受影响。
//
// 放行两类:
//   ① `/api/upload` 的收件箱 —— 手机端**唯一**的合法来路(先 upload 拿到服务端临时路径,
//      再把它喂给本命令, 见 mobile/src/lib/chat.ts 的 sendMessage);
//   ② 文件中心已放行的根(已盘点 roots + 知识库根)—— 留给「从文件中心挑主机文件当附件」
//      这类界面, 与 file_gist/file_thumb 现在能看见的范围完全一致, 不多开一寸。
// 其余一律拒。不存在的路径直接放过: 它拷不出任何东西, 交给命令自己报「文件不存在」,
// 免得把「打错字」和「越权」混成同一句话。
fn gate_attach_paths(paths: &[String]) -> Result<(), String> {
    gate_attach_paths_in(paths, &upload_dir())
}

/// 同上, 收件箱目录可注入 —— 单测不必去碰真实 home 目录。
fn gate_attach_paths_in(paths: &[String], inbox: &Path) -> Result<(), String> {
    // 收件箱可能还没建出来(没人上传过), canonicalize 会失败 → 那就只剩文件中心那一类。
    let inbox = inbox.canonicalize().ok();
    for p in paths {
        let Ok(canon) = Path::new(p).canonicalize() else {
            continue;
        };
        let in_inbox = inbox.as_ref().is_some_and(|d| canon.starts_with(d));
        if in_inbox || crate::fable::files::file_center_path_allowed(&canon.to_string_lossy()) {
            continue;
        }
        return Err("路径越界, 拒绝访问".into());
    }
    Ok(())
}

#[cfg(test)]
mod attach_gate_tests {
    use super::*;

    /// 越界路径必须被拒 —— 这是「拷进对话再读回来」那条绕行链的入口。
    /// (测试环境没有 fable 库 → 文件中心那一类恒为否, 于是只剩收件箱这一条放行路。)
    #[test]
    fn 附件路径闸_只放行收件箱与文件中心() {
        let base = std::env::temp_dir().join(format!("attach-gate-{}", std::process::id()));
        let inbox = base.join("uploads-inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        let inside = inbox.join("手机传上来的.txt");
        std::fs::write(&inside, "ok").unwrap();
        let outside = base.join("主机上的机密.txt");
        std::fs::write(&outside, "SECRET").unwrap();

        let s = |p: &Path| p.to_string_lossy().to_string();
        assert!(
            gate_attach_paths_in(&[s(&inside)], &inbox).is_ok(),
            "收件箱内必须放行"
        );

        let err = gate_attach_paths_in(&[s(&outside)], &inbox);
        assert!(err.is_err(), "收件箱外必须拒绝, 实际={err:?}");
        assert!(err.unwrap_err().contains("越界"));

        // `..` 穿越同样打不穿(两端都 canonicalize 后再比)。
        let traversal = inbox.join("..").join("主机上的机密.txt");
        assert!(
            gate_attach_paths_in(&[s(&traversal)], &inbox).is_err(),
            "`..` 穿越必须拒绝"
        );

        // 一批里只要有一条越界就整批拒 —— 别让合法路径给非法路径打掩护。
        assert!(
            gate_attach_paths_in(&[s(&inside), s(&outside)], &inbox).is_err(),
            "混入越界路径的整批必须拒"
        );

        // 不存在的路径不归本闸管(拷不出东西), 交由命令自己报「文件不存在」。
        assert!(gate_attach_paths_in(&[s(&base.join("没有这个文件"))], &inbox).is_ok());

        let _ = std::fs::remove_dir_all(&base);
    }
}

