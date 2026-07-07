// ── 引擎模块（桌面 + Docker 两种外壳共用同一份源码）──
pub mod accounts;
pub mod chat;
// 多人协作:账号/会话/设备白名单/任务卡/合并闸门(桌面主机与 Docker server 共用)。
pub mod collab;
pub mod claude_md;
pub mod codex_proxy;
pub mod conv;
pub mod convert;
pub mod doctor;
pub mod feishu;
pub mod forge;
pub mod forge_capture;   // 工业级化:持久 CDP + 5 档 fallback 链(替 forge_video 的 per-frame CLI)
pub mod forge_fx_safe;   // 工业级化:动效错误隔离 + spring 闭式解(任务 c §C.2 §C.3)
pub mod forge_pptx;
pub mod forge_pptx_native; // 路线 B:spec JSON → 原生可编辑 .pptx(零浏览器,Docker slim 可用)
pub mod forge_tts;
pub mod forge_video;
pub mod fable;
pub mod infer;
pub mod kb;
pub mod nas;
pub mod palette;
pub mod persona;
pub mod expert;
pub mod echo;
pub mod project;
pub mod provider;
pub mod scan;
pub mod sense;
pub mod skills;
pub mod voice;
// 语音识别运行时(本地 SenseVoice via sherpa-rs);默认不编译,保护现有 build。
#[cfg(feature = "voice-asr")]
pub mod voice_asr;
// 实时语音输入(录音+全局热键+注入);桌面专属,默认不编译。
#[cfg(feature = "voice-live")]
pub mod voice_live;
pub mod wecom;
// 自动更新依赖 Tauri updater/restart/package_info → 桌面专属（Docker 用 docker pull 更新）。
#[cfg(feature = "desktop")]
pub mod updater;
// 原生标题栏染色（随主题切换，仅桌面窗口有标题栏）
#[cfg(feature = "desktop")]
pub mod titlebar;

// ── host shim(broadcast 事件壳):server 壳与桌面内嵌协作主机(collab-host)共用 ──
pub mod host;
// ── Docker(server) 外壳：axum HTTP/WS 服务 ──
#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "desktop")]
use polaris_core::KbLocator;
#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::Manager;

/// host 适配器：把板块② `kb` 的 `kb_root()` 适配成 core 的 [`KbLocator`] 契约，
/// 在启动时注入给板块⑤ `polaris-sandbox`，从而打破 `sandbox → kb` 的直接依赖。
/// （架构重构 Phase 1：依赖反转的落地点）
#[cfg(feature = "desktop")]
struct HostKbLocator;
#[cfg(feature = "desktop")]
impl KbLocator for HostKbLocator {
    fn kb_root(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(kb::kb_root())
    }
}

#[cfg(feature = "desktop")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        // 自动更新（前端在启动时检查 GitHub Releases）+ 重启
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // 全局 panic 钩子(24/7 长稳第一道):任何后台线程(盘点/索引/做梦/热键/采集等)
            // panic 时,不再被默默吞掉成「死掉的子系统」,而是 eprintln + best-effort 追加到
            // 临时目录下的 polaris-panics.log(留耐久记录便于事后复盘)。链上一手以保留默认行为,
            // 不改 unwind 语义(绝不 abort)。std panic 钩子在运行时执行,故 SystemTime::now() 可用。
            let prev = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let msg = format!("[panic] {info}");
                eprintln!("{msg}");
                let log_path = std::env::temp_dir().join("polaris-panics.log");
                // 滚动: 7×24 一年只 append 会无限膨胀 → >5MiB 轮转成 .1(覆盖旧 .1)。
                // Windows 上 rename 目标存在会失败, 先删旧 .1 再转。全程 best-effort。
                if std::fs::metadata(&log_path).map(|m| m.len() > 5 * 1024 * 1024).unwrap_or(false) {
                    let bak = std::env::temp_dir().join("polaris-panics.log.1");
                    let _ = std::fs::remove_file(&bak);
                    let _ = std::fs::rename(&log_path, &bak);
                }
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log_path)
                {
                    use std::io::Write;
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let _ = writeln!(f, "{ts} {msg}");
                }
                prev(info);
            }));
            let h = app.handle();
            kb::init(h).map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            // 注入 KbLocator 给 sandbox 板块 (须在 kb::init 之后, 命令执行之前)
            app.manage(Arc::new(HostKbLocator) as Arc<dyn KbLocator>);
            polaris_sandbox::init()
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            conv::init(h).map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            chat::init(h).map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            claude_md::init(h)
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            provider::init(h)
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            // 7 个内嵌技能落盘（课件视频 / 演示 / 网站生成 / 极速下载 / browser-use /
            // 壹伴排版 / 微信待办）：全是版本门控的 best-effort 磁盘写，无人 await —— 它们只在
            // 之后 spawn claude agent 时才被读到（那远在启动之后）。整体挪到后台线程，从「窗口
            // 首帧前的 setup 主线程」移除：稳态只是几次版本比对、极快，但慢速机械盘 + 版本升级
            // 那次的多文件写不再计入首帧延迟。各 seed_* 自身仍幂等、不覆盖用户改动。
            std::thread::spawn(|| {
                skills::seed_video_studio_skill();
                skills::seed_deck_studio_skill();
                skills::seed_web_studio_skill();
                skills::seed_turbo_download_skill();
                skills::seed_browser_use_skill();
                skills::seed_wechat_typesetter_skill();
                skills::seed_wechat_tasks_skill();
                skills::seed_project_check_skill();
            });
            // 注：此前这里会为「早期播种过毛主席资料库」的老用户补装 consult-mao 技能。
            // 现「请教毛主席」默认隐藏 —— 只在用户主动安装「毛主席」名人资料包时才装该技能，
            // 启动时不再自动补装（盘上已有的 raw/毛主席、技能、项目均保留，不删用户数据）。
            // 环境预热: 后台把 claude / pwsh 目录塞进进程 PATH + 设 Git Bash 路径,
            // 让之后 spawn 的 claude CLI 直接「找得到、有 shell」, 无需重启 (见 doctor.rs)。
            doctor::prime_path_for_claude();
            // 自动更新状态机初始化（记录当前版本 + 持久化路径 + 重启续提示）。best-effort。
            let _ = updater::init(h);
            // 飞书网关「开机自动启动」：若用户开了 auto_start 且凭证齐全，后台自动拉起（不阻塞启动）。
            feishu::auto_start_if_enabled(h);
            // 寓言计划:感官 API 坞(注册表合并 + 落盘)与回声层「每日做梦」调度。
            sense::init();
            // 语音输入「极速说」:配置 + 个人词表(首启种子)就位,供防污染秒达档使用。
            voice::init();
            echo::start_scheduler(h.clone());
            // 寓言计划:检索枢纽(fable.db 表结构就位;盘点/索引由用户在设置页触发)。
            fable::init();
            // 协作主机自启:上次点过「设为主机」就静默续上(不阻塞启动)。
            collab::hosting::auto_start_if_enabled(h.clone());
            // 开发实例窗口标题带 (Dev+版本): 与已安装正式版(同为 polaris-app.exe,
            // 还可能是改牌分发)一眼区分, 测试时不点混窗口。仅 debug 构建, 发版不受影响。
            #[cfg(debug_assertions)]
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(&format!(
                    "北极星 · Polaris (Dev {})",
                    env!("CARGO_PKG_VERSION")
                ));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 多人协作:完整端工作集(本机 git)+ 隧道客户端
            collab::commands::collab_clone_partial,
            collab::commands::collab_task_setup,
            collab::commands::collab_sync_main,
            collab::commands::collab_push_branch,
            collab::commands::collab_outbox_queue,
            collab::commands::collab_outbox_pending,
            collab::commands::collab_outbox_mark_sent,
            collab::commands::collab_outbox_flush,
            collab::commands::collab_scope_status,
            collab::commands::collab_device_node_id,
            collab::commands::collab_tunnel_connect,
            collab::commands::collab_tunnel_status,
            // 多人协作:一键把本机变成协作主机(内嵌 axum 协作路由)
            collab::hosting::collab_host_start,
            collab::hosting::collab_host_status,
            collab::hosting::collab_host_stop,
            // KB
            kb::kb_root,
            kb::kb_default_root,
            kb::kb_set_root,
            kb::kb_scan,
            kb::kb_compile,
            kb::kb_list,
            kb::kb_read,
            kb::kb_delete,
            kb::kb_clear,
            kb::kb_search,
            kb::kb_ingest,
            kb::kb_upload_files,
            kb::kb_convert_batch,
            kb::kb_graph,
            kb::kb_lint,
            kb::kb_enrich_links,
            kb::kb_dedup,
            // 名人资料包（下载到自己的资料库，附带配套 skill）
            kb::kb_pack_list,
            kb::kb_pack_install,
            kb::kb_pack_remove,
            // 全盘资源归集（扫描 C/D 盘 → 多维表格 → 归档资源库 / 摄入核心层）
            scan::scan_roots,
            scan::scan_resources,
            // Sandbox (板块⑤ 已抽离为 polaris-sandbox crate, 命令名不变)
            polaris_sandbox::commands::sandbox_status,
            polaris_sandbox::commands::sandbox_build_image,
            polaris_sandbox::commands::sandbox_start,
            polaris_sandbox::commands::sandbox_stop,
            polaris_sandbox::commands::sandbox_exec,
            // CubeSandbox (E2B) 后端 — 「替换 Docker」可选后端
            polaris_sandbox::e2b::cube_config_get,
            polaris_sandbox::e2b::cube_config_set,
            polaris_sandbox::e2b::cube_status,
            // Conv (项目 + 对话历史)
            conv::conv_list_projects,
            conv::conv_create_project,
            conv::conv_project_bind_collab,
            conv::conv_archive_project,
            conv::conv_open_project_dir,
            conv::conv_list_conversations,
            conv::conv_create_conversation,
            conv::conv_delete_conversation,
            conv::conv_rename_conversation,
            conv::conv_get_messages,
            conv::conv_set_project_kb_scope,
            // 人格模块 (板块⑫)
            persona::persona_list,
            persona::persona_apply,
            // 百人专家团
            expert::expert_list,
            expert::expert_list_by_group,
            expert::expert_groups,
            expert::expert_route,
            expert::expert_get,
            expert::expert_match_auto,
            expert::expert_apply,
            expert::expert_avatar,
            expert::expert_avatar_slots,
            expert::expert_team_spawn,
            expert::expert_agents_status,
            expert::expert_teams,
            expert::expert_team_get,
            expert::team_apply,
            expert::expert_export,
            expert::team_export,
            expert::expert_route_debug,
            expert::expert_recommend_from_kb,
            // 色彩调配引擎 (全 app 配色唯一真源)
            palette::palette_generate,
            // 飞书网关 (板块⑭ 阶段 A)
            feishu::feishu_get_config,
            feishu::feishu_set_config,
            feishu::feishu_test_connection,
            feishu::feishu_create_qr,
            feishu::feishu_open_console,
            // 飞书对话引擎（阶段B：Node 桥长连接 → headless claude → 回发）
            feishu::feishu_gateway_start,
            feishu::feishu_gateway_stop,
            feishu::feishu_gateway_status,
            // 企业微信智能机器人「扫码自动配置」(OAuth 回环, 绕开 Tauri 弹窗限制)
            wecom::wecom_scan_create,
            // 自媒体「账号管理」: 探测平台登录态 + 解绑（删 profile）
            accounts::media_accounts_status,
            accounts::media_account_forget,
            // 「盘管理」: 记住登陆过的 NAS(SMB) + 一键映射/断开网络盘
            nas::nas_list,
            nas::nas_save,
            nas::nas_forget,
            nas::nas_connect,
            nas::nas_disconnect,
            // Chat
            chat::chat_send,
            chat::chat_cancel,
            chat::chat_attach_files,
            chat::chat_attach_image,
            chat::open_url,
            chat::chat_build_manifest,
            chat::artifact_read,
            chat::artifact_write,
            chat::artifact_open_external,
            chat::artifact_reveal,
            chat::artifact_list,
            chat::artifact_search,
            // 可运行项目 (板块⑮): 一键启动前后端 + 内嵌预览
            project::project_list,
            project::project_status,
            project::project_run,
            project::project_stop,
            // CLAUDE.md
            claude_md::claude_md_list_projects,
            claude_md::claude_md_kb_info,
            claude_md::claude_md_read,
            claude_md::claude_md_write,
            // Skills
            skills::list_skills,
            skills::get_skill,
            skills::create_skill,
            skills::install_skill,
            skills::import_skill,
            skills::delete_skill,
            // API 供应商坞 + 用量看板
            provider::provider_list,
            provider::provider_switch,
            provider::provider_set_link_mode,
            provider::provider_save,
            provider::provider_delete,
            provider::usage_summary,
            provider::provider_balance,
            provider::codex_status,
            provider::codex_start_login,
            provider::codex_poll_login,
            provider::codex_login_poll,
            provider::codex_login_cancel,
            provider::claude_oauth_status,
            provider::claude_start_login,
            provider::claude_finish_login,
            provider::claude_login_poll,
            provider::claude_login_cancel,
            codex_proxy::codex_proxy_info,
            // Forge 跨平台渲染能力 preflight（能出 PPT/视频吗、缺啥降级，三平台各报各的阶梯）
            forge::forge_preflight,
            // Forge 渲染引擎首落地：deck 截图 → 纯 Rust OOXML 打 .pptx（替 pptxgenjs，三平台同一份）
            forge::forge_build_pptx,
            forge::forge_screenshot,
            forge::forge_deck_to_pptx,
            // 路线 B：spec JSON → 原生可编辑 .pptx（传统PPT模式，零浏览器）
            forge::forge_spec_to_pptx,
            forge::forge_deck_to_video,
            forge::forge_deck_fx_video,
            forge::forge_tts,
            // 环境医生 (环境监测 + 配置安装)
            doctor::env_check,
            doctor::env_fix_path,
            doctor::env_install_claude,
            doctor::env_install_node,
            doctor::env_install_pwsh,
            doctor::env_install_uv,
            doctor::env_uv_cache_info,
            doctor::env_uv_cache_clean,
            doctor::env_claude_update_check,
            doctor::env_update_claude,
            doctor::env_cancel,
            // 自动更新状态机 (借鉴 OpenCode updater-controller: 单飞 + 可观测 + 持久化续提示)
            updater::updater_get_state,
            updater::updater_check,
            updater::updater_apply,
            // 原生标题栏染色（主题切换联动）
            titlebar::set_titlebar_color,
            // 寓言计划 · 感官 API 坞(设置页:服务商配置/探活/本地感官包下载)
            sense::sense_list,
            sense::sense_set,
            sense::sense_switches_set,
            sense::sense_test,
            sense::sense_pack_install,
            sense::sense_pack_remove,
            // 语音输入「极速说」:配置 / 个人词表 / 防污染(秒达档)/ 词表自学
            voice::voice_config_get,
            voice::voice_config_set,
            voice::voice_lexicon_get,
            voice::voice_hotword_add,
            voice::voice_hotword_remove,
            voice::voice_correction_add,
            voice::voice_correction_remove,
            voice::voice_anti_pollute,
            voice::voice_learn_correction,
            voice::voice_lexicon_learn,
            voice::voice_transcribe_file,
            voice::voice_listen_start,
            voice::voice_listen_stop,
            voice::voice_dictate_start,
            voice::voice_dictate_stop,
            // 寓言计划 · 回声层(对话归档 + 每日做梦蒸馏)
            conv::conv_archive_conversation,
            echo::echo_status,
            echo::echo_set,
            echo::echo_dream_now,
            echo::echo_distill_conversation,
            echo::echo_clear_context,
            echo::echo_briefing_today,
            echo::echo_briefing_dismiss,
            echo::echo_briefing_run,
            kb::kb_overview_get,
            // 寓言计划 · 检索枢纽(盘点 L1a + 向量索引 + 塌平混检)
            fable::fable_status,
            fable::fable_cancel,
            fable::inventory::fable_inventory_start,
            fable::inventory::fable_scan_folders,
            fable::inventory::fable_scan_folder_children,
            fable::inventory::fable_folder_size,
            fable::inventory::fable_backfill_lang,
            fable::inventory::fable_audit,
            fable::index::fable_index_start,
            fable::index::fable_lex_build_start,
            fable::index::fable_index_optimize,
            fable::index::fable_index_repair,
            fable::index::fable_dedupe_scan,
            fable::index::fable_local_embed_status,
            fable::index::fable_local_embed_download,
            fable::index::fable_local_embed_set_enabled,
            fable::retrieve::fable_search,
            fable::retrieve::fable_search_ai,
            fable::eval::fable_eval,
            fable::eval::fable_eval_template,
            // 文件中心(知识库内的可视化文件库:类型/语义聚类/缩略图/速览)
            fable::files::file_overview,
            fable::files::file_grid,
            fable::files::file_thumb,
            fable::files::file_gist,
            fable::files::file_cluster_build,
            fable::files::file_smart_cluster,
            fable::files::file_profile_html,
            fable::files::file_suggest_workflows,
            fable::files::file_graph,
            fable::files::file_warm_thumbs,
            fable::files::file_cluster_llm,
            fable::files::file_titles_llm,
            fable::files::file_titles_clear,
            fable::files::file_cluster_model_get,
            fable::files::file_cluster_model_set,
            fable::ontology::ontology_schemas,
            fable::ontology::ontology_overview,
            fable::ontology::ontology_seed,
            fable::ontology::ontology_extract,
            fable::ontology::ontology_triples,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Polaris application")
        .run(|_app, event| {
            // App 退出 (关窗 / 主动退出) 时回收所有在飞的 claude 子进程树, 防孤儿继续占端口/CPU。
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                chat::kill_all_children();
                // 对话状态强制落盘:append_message 走「脏标记 + 500ms 合并落盘」,
                // 退出瞬间可能还有最近半秒的消息只在内存里 —— 这里补一刀(不脏则零开销)。
                conv::flush();
                feishu::shutdown_on_exit(); // 回收飞书 node 桥,防其 autoReconnect 空转成孤儿烧 CPU
                // 释放全局键盘热键监听:置 ENABLED=false,退出时不再处理热键事件
                //(rdev::listen 无法干净中止是已知限制,置闸 + 进程退出即可接受的清理)。
                #[cfg(feature = "voice-live")]
                voice_live::stop();
            }
        });
}
