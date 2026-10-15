#[cfg(not(feature = "desktop"))]
fn dispatch_chat_and_services(cmd: &str, a: &Args, app: AppHandle) -> Result<Value, String> {
    use crate::*;
    match cmd {
        // ── Chat (sync 部分) ──
        "chat_permission_respond" => {
            let response = serde_json::from_value(a.get("response").cloned()
                .ok_or("缺少参数 response")?).map_err(|e| format!("授权参数解析失败: {e}"))?;
            ok(chat::backend::host_tools::chat_permission_respond(response)?)
        }
        "chat_cancel" => ok(chat::chat_cancel(req_str(a, "reqId")?)?),
        "chat_is_running" => ok(chat::chat_is_running(req_str(a, "reqId")?)),
        "chat_build_manifest" => ok(chat::chat_build_manifest(opt_str(a, "conversationId"))),
        // 同手机数据面:server 壳(Docker/NAS)的浏览器 UI 也只经 /api/upload 拿路径,
        // 没有原生文件对话框 —— 加闸不影响它的任何正常用法。
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
        "open_url" => ok(chat::open_url(req_str(a, "url")?)?),
        "artifact_read" => ok(chat::artifact_read(req_str(a, "path")?)?),
        "artifact_write" => ok(chat::artifact_write(
            req_str(a, "path")?,
            req_str(a, "content")?,
        )?),
        "artifact_open_external" => ok(chat::artifact_open_external(req_str(a, "path")?)?),
        "artifact_reveal" => ok(chat::artifact_reveal(req_str(a, "path")?)?),
        "artifact_list" => ok(chat::artifact_list(opt_str(a, "conversationId"))),
        "artifact_search" => ok(chat::artifact_search(req_str(a, "query")?)),

        // ── Project（容器内降级：list/status 可用，run/stop 受限但保留）──
        "project_list" => ok(project::project_list(opt_str(a, "conversationId"))),
        "project_status" => ok(project::project_status(req_str(a, "root")?)),
        "project_run" => ok(project::project_run(app, req_str(a, "root")?)?),
        "project_stop" => ok(project::project_stop(app, req_str(a, "root")?)?),

        "context_file_metadata" => ok(claude_md::context::context_file_metadata(
            req_str(a, "projectId")?, opt_str(a, "createFile"))?),
        "context_file_read" => ok(claude_md::context::context_file_read(
            req_str(a, "projectId")?, serde_json::from_value(a.get("selection").cloned()
                .ok_or("Missing selection")?).map_err(|e| format!("ContextSelectionInvalid: {e}"))?)?),
        "context_file_write" => ok(claude_md::context::context_file_write(
            req_str(a, "projectId")?, serde_json::from_value(a.get("selection").cloned()
                .ok_or("Missing selection")?).map_err(|e| format!("ContextSelectionInvalid: {e}"))?,
            req_str(a, "content")?)?),
        // ── CLAUDE.md ──
        "claude_md_list_projects" => ok(claude_md::claude_md_list_projects()),
        "claude_md_kb_info" => ok(claude_md::claude_md_kb_info()),
        "claude_md_read" => ok(claude_md::claude_md_read(
            req_str(a, "area")?,
            opt_str(a, "projectId"),
        )?),
        "claude_md_write" => ok(claude_md::claude_md_write(
            req_str(a, "area")?,
            opt_str(a, "projectId"),
            req_str(a, "content")?,
        )?),

        // ── Skills ──
        "list_skills" => ok(skills::list_skills()),
        "get_skill" => ok(skills::get_skill(req_str(a, "id")?)?),
        "create_skill" => {
            let args = skills::CreateSkillArgs {
                id: req_str(a, "id")?,
                name: req_str(a, "name")?,
                description: req_str(a, "description")?,
                system_prompt: opt_str(a, "systemPrompt")
                    .or_else(|| opt_str(a, "system_prompt"))
                    .unwrap_or_default(),
            };
            ok(skills::create_skill(args)?)
        }
        "install_skill" => ok(skills::install_skill(req_str(a, "id")?)?),
        "import_skill" => ok(skills::import_skill(req_str(a, "source")?)?),
        "delete_skill" => ok(skills::delete_skill(req_str(a, "id")?)?),

        // ── Provider + 用量 + Codex ──
        "provider_list" => ok(provider::provider_list()?),
        "provider_switch" => ok(provider::provider_switch(req_str(a, "id")?)?),
        "provider_set_link_mode" => ok(provider::provider_set_link_mode(bool_def(
            a, "link", false,
        ))?),
        "provider_set_route_mode" => ok(provider::provider_set_route_mode(bool_def(
            a, "route", false,
        ))?),
        "provider_save" => {
            let input: provider::ProviderInput =
                serde_json::from_value(a.get("input").cloned().unwrap_or(Value::Null))
                    .map_err(|e| format!("provider_save 参数解析失败: {e}"))?;
            ok(provider::provider_save(input)?)
        }
        "provider_delete" => ok(provider::provider_delete(req_str(a, "id")?)?),
        "usage_summary" => ok(provider::usage_summary()?),
        "provider_balance" => ok(provider::provider_balance(req_str(a, "id")?)?),
        // ── 生图供应商坞(独立表)+ 生图 ──
        "image_provider_list" => ok(provider::image_provider_list()?),
        "image_provider_save" => {
            let input: provider::ImageProviderInput =
                serde_json::from_value(a.get("input").cloned().unwrap_or(Value::Null))
                    .map_err(|e| format!("image_provider_save 参数解析失败: {e}"))?;
            ok(provider::image_provider_save(input)?)
        }
        "image_provider_delete" => ok(provider::image_provider_delete(req_str(a, "id")?)?),
        "image_provider_switch" => ok(provider::image_provider_switch(req_str(a, "id")?)?),
        // 桌面同名命令是 async 包装; apihub 本就在阻塞线程池里, 直调同步内核。
        "forge_image" => ok(crate::imagegen::forge_image_sync(
            req_str(a, "prompt")?,
            req_str(a, "out")?,
            opt_str(a, "ratio"),
        )?),
        "codex_status" => ok(provider::codex_status()?),
        "codex_start_login" => ok(provider::codex_start_login(Some(bool_def(
            a,
            "forceDevice",
            false,
        )))?),
        "codex_poll_login" => ok(provider::codex_poll_login(
            req_str(a, "deviceCode")?,
            req_str(a, "userCode")?,
        )?),
        "codex_login_poll" => ok(provider::codex_login_poll()?),
        "codex_login_cancel" => ok(provider::codex_login_cancel()?),
        "claude_oauth_status" => ok(provider::claude_oauth_status()?),
        "claude_start_login" => ok(provider::claude_start_login(Some(bool_def(
            a,
            "forceManual",
            false,
        )))?),
        "claude_login_poll" => ok(provider::claude_login_poll()?),
        "claude_login_cancel" => ok(provider::claude_login_cancel()?),
        "claude_finish_login" => ok(provider::claude_finish_login(
            req_str(a, "pasted")?,
            req_str(a, "verifier")?,
            req_str(a, "state")?,
        )?),
        "codex_proxy_info" => ok(integrations::codex_proxy::codex_proxy_info()),

        // ── 推理后端(R3)：外部 GPU 节点端点状态(含连通性探测)──
        "infer_status" => ok(infer::status_json()),

        // ── Forge 渲染能力 preflight：跨平台「能出 PPT/视频吗、缺啥降级」透明上报 ──
        "forge_preflight" => ok(forge::forge_preflight()),
        // ── Forge 渲染：截图 + 纯 Rust OOXML 打 .pptx（三平台同一份，替 pptxgenjs）──
        "forge_build_pptx" => forge::build_pptx_sync(vec_str(a, "images"), req_str(a, "out")?),
        "forge_screenshot" => forge::forge_screenshot(
            req_str(a, "url")?,
            req_str(a, "out")?,
            opt_usize(a, "width").map(|n| n as u32),
            opt_usize(a, "height").map(|n| n as u32),
            opt_usize(a, "scale").map(|n| n as u32),
        ),
        // spec JSON → 原生可编辑 .pptx(路线 B 传统PPT,零浏览器 → slim 镜像也能出 PPT)
        "forge_spec_to_pptx" => forge::spec_to_pptx_sync(req_str(a, "spec")?, req_str(a, "out")?),
        // 桌面同名命令是 async 包装(防冻 UI); 这里本就在阻塞线程池, 直调同步内核
        "forge_deck_to_pptx" => forge::deck_to_pptx_sync(
            req_str(a, "deck")?,
            req_str(a, "out")?,
            opt_usize(a, "width").map(|n| n as u32),
            opt_usize(a, "height").map(|n| n as u32),
            a.get("searchable").and_then(|v| v.as_bool()),
            opt_usize(a, "slides"),
        ),
        "forge_deck_to_video" => forge::deck_to_video_sync(
            req_str(a, "deck")?,
            req_str(a, "out")?,
            a.get("secondsPerSlide").and_then(|v| v.as_f64()),
            opt_usize(a, "fps").map(|n| n as u32),
            opt_usize(a, "width").map(|n| n as u32),
            opt_usize(a, "height").map(|n| n as u32),
            opt_usize(a, "slides"),
            opt_str(a, "audio"),
            opt_str(a, "narration"),
            a.get("transition").and_then(|v| v.as_f64()),
            a.get("motion").and_then(|v| v.as_bool()),
        ),
        "forge_deck_fx_video" => forge::deck_fx_video_sync(
            req_str(a, "deck")?,
            req_str(a, "out")?,
            opt_usize(a, "fps").map(|n| n as u32),
            a.get("durationMs").and_then(|v| v.as_u64()),
            opt_usize(a, "width").map(|n| n as u32),
            opt_usize(a, "height").map(|n| n as u32),
            opt_usize(a, "slide"),
        ),
        "forge_tts" => forge::forge_tts_sync(
            req_str(a, "text")?,
            req_str(a, "out")?,
            opt_str(a, "voice"),
            opt_str(a, "languageBoost"),
        ),

        // ── 环境医生（容器内只读检测；安装类降级为提示）──
        "env_check" => ok(doctor::env_check()),
        // 深度校验:纯只读探测(起子进程跑 --version / 扫冲突),容器内直通。
        // deep=true 会真发一次请求做端到端冒烟,默认 false —— 前端不传就不花额度。
        "env_verify" => ok(doctor::env_verify(bool_def(a, "deep", false))),
        // 静默托管状态: 容器版从不自己装东西(组件随镜像预装), 如实回一个「没跑过」的空状态
        "env_autopilot_status" => ok(doctor::env_autopilot_status()),
        "env_fix_path" => ok(doctor::env_fix_path()?),
        "env_claude_update_check" => ok(doctor::env_claude_update_check()),
        "env_install_claude" | "env_install_node" | "env_install_pwsh" | "env_update_claude" => {
            Err(
                "容器环境已预装运行所需组件，无需在此安装。如需升级请更新镜像 (docker pull)。"
                    .to_string(),
            )
        }
        // uv 未预烤进容器镜像,自动安装脚本也只支持 Win/mac 桌面 → 给明确指引而非 404
        "env_install_uv" => Err(
            "容器环境不支持在线安装 uv:请进容器执行 `curl -LsSf https://astral.sh/uv/install.sh | sh`,或更新预装 uv 的镜像 (docker pull)。"
                .to_string(),
        ),
        // uv 缓存治理:纯子进程调用 `uv cache dir/clean`,容器内直通(没装 uv 时函数自会报「未找到」)
        "env_uv_cache_info" => ok(doctor::env_uv_cache_info()),
        "env_uv_cache_clean" => ok(doctor::env_uv_cache_clean()?),
        "env_cancel" => ok(doctor::env_cancel(req_str(a, "reqId")?)?),

        // ── 飞书 / 企微 / 自媒体账号 ──
        "feishu_get_config" => ok(integrations::feishu::feishu_get_config()),
        "feishu_set_config" => {
            let cfg: integrations::feishu::FeishuConfig =
                serde_json::from_value(a.get("config").cloned().unwrap_or(Value::Null))
                    .map_err(|e| format!("feishu_set_config 参数解析失败: {e}"))?;
            ok(integrations::feishu::feishu_set_config(cfg)?)
        }
        "feishu_test_connection" => ok(integrations::feishu::feishu_test_connection()),
        "feishu_create_qr" => ok(integrations::feishu::feishu_create_qr()?),
        "feishu_open_console" => ok(integrations::feishu::feishu_open_console()?),
        "feishu_gateway_start" => ok(integrations::feishu::feishu_gateway_start(app)?),
        "feishu_gateway_stop" => ok(integrations::feishu::feishu_gateway_stop(app)?),
        "feishu_gateway_status" => ok(integrations::feishu::feishu_gateway_status()),
        "wecom_scan_create" => ok(integrations::wecom::wecom_scan_create(req_str(
            a, "source",
        )?)?),
        "media_accounts_status" => ok(accounts::media_accounts_status()),
        "media_account_forget" => ok(accounts::media_account_forget(req_str(a, "platform")?)?),

        // ── 盘管理(NAS 网络盘记忆 + 映射)──
        "nas_list" => ok(crate::integrations::nas::nas_list()),
        "nas_save" => {
            let rec = serde_json::from_value(a.get("record").cloned().unwrap_or(Value::Null))
                .map_err(|e| format!("record 参数无效：{e}"))?;
            ok(crate::integrations::nas::nas_save(rec)?)
        }
        "nas_forget" => ok(crate::integrations::nas::nas_forget(req_str(a, "id")?)?),
        "nas_connect" => {
            let rec = serde_json::from_value(a.get("record").cloned().unwrap_or(Value::Null))
                .map_err(|e| format!("record 参数无效：{e}"))?;
            ok(crate::integrations::nas::nas_connect(rec)?)
        }
        "nas_disconnect" => {
            let rec = serde_json::from_value(a.get("record").cloned().unwrap_or(Value::Null))
                .map_err(|e| format!("record 参数无效：{e}"))?;
            ok(crate::integrations::nas::nas_disconnect(rec)?)
        }

        // ── 降级/桌面专属：给惰性 stub，保证前端不报错 ──
        "sandbox_status" => ok(json!({
            "docker_installed": false, "docker_running": false, "image_built": false,
            "image_name": "polaris-sandbox:alpine", "container_running": false,
            "container_name": "polaris-sandbox",
            "notes": ["容器(Docker)模式：Docker-in-Docker 沙箱本期降级，不可用"]
        })),
        "sandbox_build_image" | "sandbox_start" | "sandbox_stop" | "sandbox_exec" => {
            Err("容器模式下沙箱板块已降级（Docker-in-Docker 风险高）。".to_string())
        }
        "cube_config_get" => ok(json!({"backend":"docker","endpoint":"","apiKey":""})),
        "cube_config_set" => ok(a
            .get("config")
            .cloned()
            .unwrap_or(json!({"backend":"docker"}))),
        "cube_status" => ok(json!({
            "backend":"docker","endpoint":"","configured":false,"reachable":false,
            "note":"容器模式 - 无沙箱探测"
        })),
        "updater_get_state" => ok(json!({"phase":"idle","note":"容器版用 docker pull 更新"})),
        "updater_check" => ok(json!({"phase":"idle"})),
        "updater_apply" => Err("容器版请用 docker pull 拉新镜像更新。".to_string()),

        // ── 容器远程更新(前端 useUpdater.ts 容器线调用)──
        // docker_status:App 只检查隔离 updater 的 URL/token 是否配置；它永不探测 docker.sock。
        "docker_status" => {
            let (enabled, service, script) = docker_updater_bits();
            ok(json!({
                "updater_enabled": enabled,
                "updater_service": service,
                "update_script": script,
                "auth_configured": docker_update_auth_configured(),
                "current_version": current_build_version(),
                "current_revision": current_build_revision(),
                "boot_id": docker_build_identity().get("bootId").cloned().unwrap_or(Value::Null),
                "current_tag": std::env::var("POLARIS_TAG").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "latest".to_string()),
                "image_repo": std::env::var("POLARIS_IMAGE_REPO").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| DEFAULT_IMAGE_REPO.to_string()),
            }))
        }
        // docker_check_update:只查不动 —— 逐个镜像源拉 manifest,回「当前/最新/有没有新版」。
        //   不需要 updater sidecar,所以未启用一键替换的容器也能看到新版(再引导 SSH 兜底)。
        "docker_check_update" => ok(docker_check_update()),
        // docker_update:先验证 registry 目标并持久化 request，再 detached 触发 Watchtower。
        // HTTP handler 立即返回；真正替换时旧 App 可正常退出，sidecar 内更新继续执行。
        "docker_update" => {
            if !bool_def(a, "confirm", false) {
                return Err("更新需要确认 (confirm: true)".to_string());
            }
            let (enabled, service, script) = docker_updater_bits();
            if !docker_update_auth_configured() {
                return Err("Docker 一键更新需要先设置 POLARIS_AUTH_TOKEN 或 POLARIS_REQUIRE_LOGIN=1；开放模式不会获得容器替换能力。".to_string());
            }
            if !script {
                return Err("/usr/local/bin/update.sh 不存在(镜像未含更新脚本,旧版镜像请先手动装一次新的)。".to_string());
            }
            if !service {
                return Err("隔离更新服务未配置。请生成 POLARIS_UPDATER_TOKEN，并用 docker-compose.update.yml 重建服务。".to_string());
            }
            if !enabled {
                return Err("容器一键更新未满足安全条件，请检查应用鉴权、updater token 与更新脚本。".to_string());
            }
            let target = docker_registry_target()?;
            if current_build_revision() == target.revision {
                return ok(json!({
                    "accepted": false,
                    "upToDate": true,
                    "targetRevision": target.revision,
                    "targetVersion": target.version,
                    "targetDigest": target.index_digest,
                    "note": "当前容器已经运行 registry 中这个 tag 的目标 build。",
                }));
            }
            ok(spawn_docker_update(&target)?)
        }
        "docker_update_status" => ok(docker_update_status(&req_str(a, "requestId")?)?),

        other => Err(format!("未知命令: {other}")),
    }
}
