/// 桌面内嵌主机的命令分发(desktop flavor)。desktop 下引擎命令是 async 薄包装,故这里
/// `await`。只覆盖**手机远程数据面实际用到的命令**(文件浏览/预览、对话辅助、会话读取);
/// 其余命令请用 Docker/NAS server 版(全量 dispatch_sync)。手机的账号/项目/任务走
/// /api/collab/*(collab_router),不经此分发。
#[cfg(feature = "desktop")]
async fn dispatch_desktop(cmd: &str, a: &Args, _app: AppHandle) -> Result<Value, String> {
    use crate::*;
    match cmd {
        // ── 文件中心(手机「文件」页 + 预览) ──
        "file_overview" => ok(fable::files::file_overview(opt_str(a, "root")).await?),
        "file_grid" => ok(fable::files::file_grid(
            opt_str(a, "root"),
            a.get("clusterId").and_then(|v| v.as_i64()),
            opt_str(a, "kind"),
            opt_str(a, "lang"),
            opt_str(a, "sort"),
            opt_str(a, "query"),
            opt_usize(a, "page"),
            opt_usize(a, "pageSize"),
        )
        .await?),
        "file_thumb" => ok(fable::files::file_thumb(
            req_str(a, "abspath")?,
            a.get("max").and_then(|v| v.as_u64()).map(|n| n as u32),
        )
        .await?),
        "file_gist" => ok(fable::files::file_gist(req_str(a, "abspath")?).await?),

        // ── 知识库检索(手机备用) ──
        "kb_search" => ok(kb::kb_search(req_str(a, "query")?, opt_usize(a, "topK")).await),

        // ── 受控远程执行策略(exec 本体走 /api/exec 专用端点,不走 invoke) ──
        "exec_policy_get" => ok(crate::exec::exec_policy_get()),
        "exec_policy_set" => ok(crate::exec::exec_policy_set(
            a.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
            a.get("shellMinutes").and_then(|v| v.as_i64()),
        )?),
        // ── 远程盘共享目录(主机侧开放哪些目录给对端访问,逐目录点选放开写) ──
        "fs_share_get" => ok(crate::fsshare::fs_share_get()),
        "fs_share_set" => ok(crate::fsshare::fs_share_set_authorized(vec_str(a, "paths"))?),
        "fs_share_list" => ok(crate::fsshare::fs_share_list()),
        "fs_share_save" => ok(crate::fsshare::fs_share_save_authorized(vec_val(a, "items"))?),
        // ── 应用直投:手机要知道主机发布了哪些应用(点开后走 /app/{slug}/) ──
        "app_pub_list" => ok(crate::appproxy::app_pub_list()),
        "app_open" => ok(crate::appproxy::app_open(req_str(a, "slug")?)?),

        // ── 对话辅助(chat_send 在 invoke 里单独特判) ──
        "chat_permission_respond" => {
            let response = serde_json::from_value(a.get("response").cloned()
                .ok_or("缺少参数 response")?).map_err(|e| format!("授权参数解析失败: {e}"))?;
            ok(chat::backend::host_tools::chat_permission_respond(response)?)
        }
        "chat_cancel" => ok(chat::chat_cancel(req_str(a, "reqId")?)?),
        "chat_is_running" => ok(chat::chat_is_running(req_str(a, "reqId")?)),
        // 附件路径必须过闸(见 gate_attach_paths 的大注释):它会把给定路径的文件
        // 整份拷进会话 uploads 目录, 而那目录是 artifact_read 放行的 —— 不拦就等于
        // 给文件中心那道闸开了条绕行道。
        "chat_attach_files" => {
            let paths = vec_str(a, "paths");
            gate_attach_paths(&paths)?;
            ok(chat::chat_attach_files(opt_str(a, "conversationId"), paths))
        }
        "chat_attach_image" => ok(chat::chat_attach_image(
            opt_str(a, "conversationId"),
            req_str(a, "name")?,
            req_str(a, "dataBase64")?,
        )?),
        "chat_build_manifest" => ok(chat::chat_build_manifest(opt_str(a, "conversationId"))),

        // ── 隔空同屏说明页(beam_pack/export/send 在 invoke 上层特判,这里补同步读命令) ──
        "beam_doc_path" => ok(crate::beam::beam_doc_path()?),

        // ── 产物(手机产物 chip 预览走 /api/file;这里给读取/列举备用) ──
        "artifact_read" => ok(chat::artifact_read(req_str(a, "path")?)?),
        "artifact_list" => ok(chat::artifact_list(opt_str(a, "conversationId")).await),
        "artifact_search" => ok(chat::artifact_search(req_str(a, "query")?).await),

        // ── 电脑上的项目(手机说「打开××项目」后,新会话建在该项目下 → claude 的 cwd
        //     就是这个项目绑定的工作目录, 等同电脑上 `cd <repo> && claude`) ──
        // 不开放 archive/open_dir:归档是破坏性的、在电脑上弹资源管理器对手机也没意义。
        "conv_list_projects" => ok(conv::conv_list_projects()),
        "conv_create_project" => ok(conv::conv_create_project(req_str(a, "name")?)?),
        "conv_set_project_work_dir" => ok(conv::conv_set_project_work_dir(
            req_str(a, "projectId")?,
            opt_str(a, "workDir"),
        )?),
        // ── 会话读取(手机历史主要走本地存储,这些为兼容/备用) ──
        "conv_list_conversations" => ok(conv::conv_list_conversations(req_str(a, "projectId")?)),
        "conv_get_messages" => ok(conv::conv_get_messages(req_str(a, "conversationId")?)),
        "conv_create_conversation" => {
            ok(conv::conv_create_conversation(req_str(a, "projectId")?)?)
        }
        "conv_delete_conversation" => {
            ok(conv::conv_delete_conversation(req_str(a, "conversationId")?)?)
        }
        // 设备联盟遥测:本机资源实况(远端设备经中继/隧道取用)。
        "sys_stats" => ok(crate::sysstat::sample()),

        // ── 输入区选择器(手机豆包式输入条:模型/技能;只读) ──
        // provider_list 的 auth_token / settings_config 现已在内核出口统一打码
        // (store.rs 的 mask_secret);这里再窄一道 —— 手机选择器只要这几个字段,
        // 连打了码的密钥和整份 settings_config 都不必下发。两道闸互为保险。
        "provider_list" => {
            let r = provider::provider_list()?;
            ok(json!({
                "providers": r.providers.iter().map(|p| json!({
                    "id": p.id,
                    "name": p.name,
                    "category": p.category,
                    "protocol": p.protocol,
                    "color": p.color,
                    "hasKey": p.has_key,
                })).collect::<Vec<Value>>(),
                "currentId": r.current_id,
            }))
        }
        "list_skills" => ok(skills::list_skills().await),

        // ── 语音输入(手机端按住说话:录 WAV 传回主机,主机拿火山凭据转写)──
        // 只开放转写这一个口,配置读写仍留在桌面 —— 火山凭据不经手机下发。
        "voice_transcribe_audio" => {
            let audio = req_str(a, "audio")?;
            let fmt = opt_str(a, "format");
            // 转写是 **同步阻塞** 的 ureq 请求(可跑几十秒)。desktop flavor 下
            // dispatch_desktop 是直接 await 在 tokio worker 上跑的(不像 server 壳外面
            // 套了 spawn_blocking),不甩进阻塞池就会钉死一个 worker —— 踩过的坑。
            let r = tokio::task::spawn_blocking(move || voice::voice_transcribe_audio(audio, fmt))
                .await
                .map_err(|e| format!("语音转写任务失败: {e}"))??;
            ok(r)
        }

        _ => Err(format!(
            "命令 {cmd} 在桌面主机模式暂不支持(手机远程仅开放文件/对话数据面;全部命令请用 Docker/NAS server 版)"
        )),
    }
}

