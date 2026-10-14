
/// server 壳全量命令分发(≈200 命令,同步直调各引擎函数)。desktop 下这些引擎命令是
/// async 薄包装(见 dispatch_desktop),签名不兼容,故本函数**仅 server flavor 编译**。
#[cfg(not(feature = "desktop"))]
fn dispatch_sync(cmd: &str, a: &Args, app: AppHandle) -> Result<Value, String> {
    use crate::*;
    match cmd {
        // ── KB ──
        "kb_root" => ok(kb::kb_root()),
        "kb_default_root" => ok(kb::kb_default_root()),
        // 设备联盟遥测:云主机/server 壳自采本机资源。
        "sys_stats" => ok(crate::sysstat::sample()),
        // 应用直投:远端要知道主机发布了哪些本机应用
        "app_pub_list" => ok(crate::appproxy::app_pub_list()),
        "app_open" => ok(crate::appproxy::app_open(req_str(a, "slug")?)?),
        "beam_doc_path" => ok(crate::beam::beam_doc_path()?),
        // 受控远程执行策略(exec 本体走 /api/exec 专用端点,不走 invoke)
        "exec_policy_get" => ok(crate::exec::exec_policy_get()),
        "exec_policy_set" => ok(crate::exec::exec_policy_set(
            a.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
            a.get("shellMinutes").and_then(|v| v.as_i64()),
        )?),
        // 远程盘共享目录(主机侧开放哪些目录给对端访问,逐目录点选放开写)
        "fs_share_get" => ok(crate::fsshare::fs_share_get()),
        "fs_share_set" => ok(crate::fsshare::fs_share_set(vec_str(a, "paths"))?),
        "fs_share_list" => ok(crate::fsshare::fs_share_list()),
        "fs_share_save" => ok(crate::fsshare::fs_share_save(vec_val(a, "items"))?),
        "kb_set_root" => ok(kb::kb_set_root(req_str(a, "newPath")?)?),
        "kb_scan" => ok(kb::kb_scan_sync()?),
        "kb_compile" => ok(wiki::kb_compile(app)?),
        "kb_list" => ok(kb::kb_list(opt_str(a, "subdir"))),
        "kb_read" => ok(kb::kb_read(req_str(a, "relPath")?)?),
        "kb_delete" => ok(kb::kb_delete(req_str(a, "relPath")?)?),
        "kb_clear" => ok(kb::kb_clear()?),
        "kb_search" => ok(kb::kb_search(req_str(a, "query")?, opt_usize(a, "topK"))),
        "kb_ingest" => ok(kb::kb_ingest(req_str(a, "sourcePath")?)?),
        "kb_upload_files" => ok(kb::kb_upload_files(vec_str(a, "paths"))),
        "kb_convert_batch" => ok(kb::kb_convert_batch(vec_str(a, "paths"))?),
        "kb_graph" => ok(kb::kb_graph()),
        "kb_lint" => ok(kb::kb_lint()),
        "kb_enrich_links" => ok(kb::kb_enrich_links(app)?),
        "kb_dedup" => ok(kb::kb_dedup(app)?),
        "kb_scan_sources" => ok(kb::kb_scan_sources()),
        "kb_quarantine" => ok(kb::kb_quarantine(req_str(a, "relPath")?)?),
        "kb_pack_list" => ok(kb::kb_pack_list()),
        "kb_pack_install" => ok(kb::kb_pack_install(app, req_str(a, "id")?)?),
        "kb_pack_remove" => ok(kb::kb_pack_remove(req_str(a, "id")?)?),

        // ── 全盘资源归集 ──
        "scan_roots" => ok(scan::scan_roots()),
        "scan_resources" => ok(scan::scan_resources(
            vec_str(a, "roots"),
            opt_usize(a, "max"),
        )?),

        // ── 寓言计划 · 感官 API 坞 ──
        "sense_list" => ok(sense::sense_list()),
        "sense_set" => ok(sense::sense_set(
            req_str(a, "id")?,
            opt_str(a, "apiKey"),
            opt_str(a, "baseUrl"),
            opt_bool(a, "enabled"),
            opt_str(a, "defaultModel"),
        )?),
        "sense_switches_set" => ok(sense::sense_switches_set(
            opt_bool(a, "cloudEnabled"),
            opt_bool(a, "audioEgress"),
            opt_bool(a, "imageEgress"),
            opt_f64(a, "budgetMonthlyCny"),
        )?),
        "sense_test" => ok(sense::sense_test(req_str(a, "id")?)?),
        "sense_pack_install" => ok(sense::sense_pack_install(app, req_str(a, "id")?)?),
        "sense_pack_remove" => ok(sense::sense_pack_remove(req_str(a, "id")?)?),

        // ── 语音输入「极速说」· 防污染 + 配置 + 个人词表 ──
        "voice_config_get" => ok(voice::voice_config_get()),
        "voice_config_set" => ok(voice::voice_config_set(
            opt_str(a, "activation"),
            opt_str(a, "hotkey"),
            opt_str(a, "engine"),
            opt_bool(a, "fluentMode"),
            opt_bool(a, "polish"),
            opt_str(a, "antipollute"),
            a.get("pinyinThreshold")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            opt_str(a, "overlayPos"),
            opt_str(a, "polishApiBase"),
            opt_str(a, "polishApiKey"),
            opt_str(a, "polishModel"),
            opt_str(a, "volcAppKey"),
            opt_str(a, "volcAccessKey"),
            opt_str(a, "volcAsrModel"),
        )?),
        "voice_lexicon_get" => ok(voice::voice_lexicon_get()),
        "voice_hotword_add" => ok(voice::voice_hotword_add(req_str(a, "word")?)?),
        "voice_hotword_remove" => ok(voice::voice_hotword_remove(req_str(a, "word")?)?),
        "voice_correction_add" => ok(voice::voice_correction_add(
            req_str(a, "wrong")?,
            req_str(a, "right")?,
        )?),
        "voice_correction_remove" => ok(voice::voice_correction_remove(req_str(a, "wrong")?)?),
        "voice_anti_pollute" => ok(voice::voice_anti_pollute(req_str(a, "text")?)),
        // AI 整形试跑(设置页「测一下整形」):纯 HTTP 调 LLM,容器内可用
        "voice_polish" => ok(voice::voice_polish(req_str(a, "text")?)?),
        "voice_transcribe_file" => ok(voice::voice_transcribe_file(req_str(a, "path")?)?),
        // 手机端语音输入:录好的 WAV(base64)传回来,由主机拿火山凭据转写。
        "voice_transcribe_audio" => ok(voice::voice_transcribe_audio(
            req_str(a, "audio")?,
            opt_str(a, "format"),
        )?),
        "voice_listen_start" => ok(voice::voice_listen_start(app)?),
        "voice_listen_stop" => ok(voice::voice_listen_stop()?),
        "voice_dictate_start" => ok(voice::voice_dictate_start(app)?),
        "voice_dictate_stop" => ok(voice::voice_dictate_stop()?),
        "voice_learn_correction" => ok(voice::voice_learn_correction(
            req_str(a, "wrong")?,
            req_str(a, "right")?,
        )?),
        "voice_lexicon_learn" => ok(voice::voice_lexicon_learn(
            req_str(a, "text")?,
            opt_usize(a, "top"),
        )?),

        // ── 寓言计划 · 回声层(对话沉淀/做梦)──
        "conv_archive_conversation" => ok(conv::conv_archive_conversation(
            req_str(a, "id")?,
            bool_def(a, "archived", true),
        )?),
        "echo_status" => ok(echo::echo_status()),
        "echo_set" => ok(echo::echo_set(
            opt_bool(a, "enabled"),
            opt_u8(a, "hour"),
            opt_bool(a, "runOnBoot"),
        )),
        "echo_dream_now" => ok(echo::echo_dream_now(app)?),
        "echo_distill_conversation" => {
            ok(echo::echo_distill_conversation(app, req_str(a, "convId")?)?)
        }
        "echo_clear_context" => ok(echo::echo_clear_context(app, req_str(a, "convId")?)?),
        // Figma 往返桥（回程拉取）
        "figma_pull" => ok(figma_bridge::figma_pull(
            req_str(a, "file")?,
            req_str(a, "token")?,
        )?),
        "figma_export_svgs" => ok(figma_bridge::figma_export_svgs(
            req_str(a, "file")?,
            req_vec_str(a, "ids")?,
            req_str(a, "token")?,
        )?),
        "echo_briefing_today" => ok(echo::echo_briefing_today()),
        "echo_briefing_dismiss" => ok(echo::echo_briefing_dismiss(req_str(a, "id")?)),
        "echo_briefing_run" => ok(echo::echo_briefing_run(app)?),
        "kb_overview_get" => ok(kb::kb_overview_get()),

        // ── 寓言计划 · 检索枢纽(盘点 L1a + 向量索引 + 塌平混检)──
        "fable_status" => ok(fable::fable_status()?),
        "fable_cancel" => ok(fable::fable_cancel(opt_str(a, "task"))),
        "fable_inventory_start" => ok(fable::inventory::fable_inventory_start(
            app,
            Some(vec_str(a, "roots")),
            Some(vec_str(a, "exclude")),
            a.get("full").and_then(|v| v.as_bool()),
        )?),
        "fable_scan_folders" => ok(fable::inventory::fable_scan_folders(opt_str(a, "root"))?),
        "fable_scan_folder_children" => ok(fable::inventory::fable_scan_folder_children(
            req_str(a, "root")?,
            req_str(a, "path")?,
        )?),
        "fable_folder_size" => ok(fable::inventory::fable_folder_size(req_str(a, "path")?)?),
        "fable_backfill_lang" => ok(fable::inventory::fable_backfill_lang()?),
        "fable_audit" => ok(fable::inventory::fable_audit(
            opt_str(a, "mode"),
            opt_usize(a, "sample"),
        )?),

        // ── 企业 Schema 知识库(本体)——desktop 走 #[tauri::command],server/Docker 须在此显式接 dispatch ──
        "ontology_schemas" => ok(fable::ontology::ontology_schemas()?),
        "ontology_overview" => ok(fable::ontology::ontology_overview()?),
        "ontology_seed" => ok(fable::ontology::ontology_seed(req_str(a, "schemaId")?)?),
        "ontology_extract" => ok(fable::ontology::ontology_extract(
            app,
            req_str(a, "schemaId")?,
        )?),
        "ontology_triples" => ok(fable::ontology::ontology_triples(
            req_str(a, "schemaId")?,
            opt_usize(a, "limit").map(|v| v as u32),
        )?),
        "fable_index_start" => ok(fable::index::fable_index_start(
            app,
            opt_usize(a, "maxChunks"),
        )?),
        "fable_lex_build_start" => ok(fable::index::fable_lex_build_start(app)?),
        "fable_index_optimize" => ok(fable::index::fable_index_optimize()?),
        "fable_index_repair" => ok(fable::index::fable_index_repair()?),
        "fable_dedupe_scan" => ok(fable::index::fable_dedupe_scan(Some(bool_def(
            a, "backfill", false,
        )))?),
        "fable_local_embed_status" => ok(fable::index::fable_local_embed_status()?),
        "fable_local_embed_download" => ok(fable::index::fable_local_embed_download(app)?),
        "fable_local_embed_set_enabled" => ok(fable::index::fable_local_embed_set_enabled(
            bool_def(a, "on", false),
        )?),
        "fable_search" => ok(fable::retrieve::fable_search(
            req_str(a, "query")?,
            opt_usize(a, "topK"),
            opt_str(a, "mode"),
            opt_str(a, "scope"),
        )?),
        "fable_search_ai" => ok(fable::retrieve::fable_search_ai(
            req_str(a, "query")?,
            opt_usize(a, "topK"),
            opt_str(a, "scope"),
        )?),
        "fable_eval" => ok(fable::eval::fable_eval(
            opt_str(a, "path"),
            opt_usize(a, "topK"),
            opt_str(a, "mode"),
        )?),
        "fable_eval_template" => ok(fable::eval::fable_eval_template(opt_str(a, "path"))?),

        // ── 文件中心(可视化文件库)──
        "file_overview" => ok(fable::files::file_overview(opt_str(a, "root"))?),
        "file_grid" => ok(fable::files::file_grid(
            opt_str(a, "root"),
            a.get("clusterId").and_then(|v| v.as_i64()),
            opt_str(a, "kind"),
            opt_str(a, "lang"),
            opt_str(a, "sort"),
            opt_str(a, "query"),
            opt_usize(a, "page"),
            opt_usize(a, "pageSize"),
        )?),
        "file_thumb" => ok(fable::files::file_thumb(
            req_str(a, "abspath")?,
            a.get("max").and_then(|v| v.as_u64()).map(|n| n as u32),
        )?),
        "file_gist" => ok(fable::files::file_gist(req_str(a, "abspath")?)?),
        "file_cluster_build" => ok(fable::files::file_cluster_build(app, opt_str(a, "root"))?),
        "file_smart_cluster" => ok(fable::files::file_smart_cluster(
            app,
            opt_str(a, "root"),
            opt_bool(a, "quick"),
        )?),
        "file_profile_html" => ok(fable::files::file_profile_html(opt_str(a, "root"))?),
        "file_suggest_workflows" => ok(fable::files::suggest_workflows(opt_str(a, "root"))?),
        "file_graph" => ok(fable::files::file_graph(opt_str(a, "root"))?),
        "file_warm_thumbs" => ok(fable::files::file_warm_thumbs(
            vec_str(a, "paths"),
            a.get("max").and_then(|v| v.as_u64()).map(|n| n as u32),
        )?),
        "file_cluster_llm" => ok(fable::files::file_cluster_llm(app, opt_str(a, "root"))?),
        "file_titles_llm" => ok(fable::files::file_titles_llm(app, opt_str(a, "root"))?),
        "file_titles_clear" => ok(fable::files::file_titles_clear()?),
        "file_cluster_model_get" => ok(fable::files::file_cluster_model_get()),
        "file_cluster_model_set" => ok(fable::files::file_cluster_model_set(
            opt_bool(a, "enabled"),
            opt_str(a, "baseUrl"),
            opt_str(a, "model"),
            opt_str(a, "apiKey"),
        )?),

        // ── Conv ──
        "conv_list_projects" => ok(conv::conv_list_projects()),
        "conv_create_project" => ok(conv::conv_create_project(req_str(a, "name")?)?),
        "conv_project_bind_collab" => ok(conv::conv_project_bind_collab(
            req_str(a, "projectId")?,
            a.get("collabProjectId")
                .and_then(|v| v.as_i64())
                .ok_or("缺 collabProjectId")?,
            req_str(a, "collabHost").unwrap_or_default(),
        )?),
        "conv_set_project_kb_scope" => ok(conv::conv_set_project_kb_scope(
            req_str(a, "projectId")?,
            opt_str(a, "kbScope"),
        )?),
        "conv_set_project_work_dir" => ok(conv::conv_set_project_work_dir(
            req_str(a, "projectId")?,
            opt_str(a, "workDir"),
        )?),
        "conv_open_project_dir" => ok(conv::conv_open_project_dir(req_str(a, "projectId")?)?),
        "conv_archive_project" => ok(conv::conv_archive_project(req_str(a, "projectId")?)?),
        "conv_list_conversations" => ok(conv::conv_list_conversations(req_str(a, "projectId")?)),
        "conv_create_conversation" => ok(conv::conv_create_conversation(req_str(a, "projectId")?)?),
        "conv_delete_conversation" => ok(conv::conv_delete_conversation(req_str(
            a,
            "conversationId",
        )?)?),
        "conv_get_messages" => ok(conv::conv_get_messages(req_str(a, "conversationId")?)),
        "conv_rename_conversation" => ok(conv::conv_rename_conversation(
            req_str(a, "conversationId")?,
            req_str(a, "title")?,
        )?),

        // ── Persona ──
        "persona_list" => ok(persona::persona_list()),
        "persona_apply" => ok(persona::persona_apply(
            req_str(a, "projectId")?,
            req_str(a, "personaId")?,
            bool_def(a, "overwrite", false),
        )?),

        // ── Expert / 专家团（Docker/web 版同样要能用专家市场、向导推荐、一键入驻）──
        "expert_list" => ok(expert::expert_list()),
        "expert_list_by_group" => ok(expert::expert_list_by_group(req_str(a, "group")?)),
        "expert_groups" => ok(expert::expert_groups()),
        "expert_route" => {
            let req: expert::RouteRequest =
                serde_json::from_value(a.get("req").cloned().unwrap_or(Value::Null))
                    .map_err(|e| format!("expert_route 参数解析失败: {e}"))?;
            ok(expert::expert_route(req))
        }
        "expert_get" => ok(expert::expert_get(req_str(a, "id")?)),
        "expert_match_auto" => ok(expert::expert_match_auto(req_str(a, "query")?)),
        "expert_apply" => ok(expert::expert_apply(
            req_str(a, "projectId")?,
            req_str(a, "expertId")?,
            bool_def(a, "overwrite", false),
        )?),
        "expert_avatar" => ok(expert::expert_avatar(req_str(a, "id")?)),
        "expert_avatar_slots" => ok(expert::expert_avatar_slots()),
        "expert_team_spawn" => ok(expert::expert_team_spawn(
            req_str(a, "projectId")?,
            req_str(a, "taskDescription")?,
        )),
        "expert_agents_status" => ok(expert::expert_agents_status(req_str(a, "projectId")?)),
        "expert_teams" => ok(expert::expert_teams()),
        "expert_team_get" => ok(expert::expert_team_get(req_str(a, "id")?)),
        "team_apply" => ok(expert::team_apply(
            req_str(a, "projectId")?,
            req_str(a, "teamId")?,
            bool_def(a, "overwrite", false),
        )?),
        "expert_export" => ok(expert::expert_export(req_str(a, "id")?)?),
        "team_export" => ok(expert::team_export(req_str(a, "id")?)?),
        "expert_route_debug" => ok(expert::expert_route_debug(req_str(a, "query")?)),
        "expert_recommend_from_kb" => ok(expert::expert_recommend_from_kb(opt_str(a, "scope"))),

        // ── 配色引擎(全 app 配色唯一真源)──
        // server dispatch 曾漏注册 → web/server 端 palette_generate 一律 404(codex 深测揪出)。
        // 桌面 generate_handler 早已注册;补齐双壳一致。注意参数是 mood(不是 mode)。
        "palette_generate" => ok(palette::palette_generate(opt_str(a, "seed"), opt_str(a, "mood"))?),

        _ => dispatch_chat_and_services(cmd, a, app),
    }
}
