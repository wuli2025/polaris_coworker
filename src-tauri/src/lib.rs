// ── 引擎模块（桌面 + Docker 两种外壳共用同一份源码）──
pub mod accounts;
// 多人协作已抽为独立 crate(polaris-collab);壳件留本仓:
// apihub=应用数据面分发(认识全部引擎), hosting=桌面一键当主机拼装。
pub use polaris_collab::collab;
#[cfg(feature = "collab-host")]
pub mod apihub;
#[cfg(feature = "desktop")]
pub mod hosting;
pub mod expert;
// ── 板块已抽为独立 crate(分仓规划 v2 Phase 1), 别名保持全部旧路径 ──
// (含 generate_handler! 的 __cmd__ 宏解析与 server dispatch 的 `crate::X::…`)零改动。
pub use polaris_fable::{fable, kb};
pub use polaris_forge::forge;
pub use polaris_kernel::{chat, claude_md, conv, convert, doctor, integrations, provider, skills};
pub use polaris_wiki::wiki;
pub mod infer;
pub mod palette;
pub mod persona;
pub mod project;
pub mod voice;

// ── Phase 0 文件归位的 crate 根别名(分仓规划 v2)──
// echo/sense/scan 归 fable(懂你+检索板块)、figma_bridge 归 forge(设计成品板块);
// 别名让 `crate::echo` 等全部旧路径(含 generate_handler! 的 __cmd__ 宏解析)零改动,
// 抽仓时删别名、调用方一次性切新路径。
pub use fable::{echo, scan, sense};
pub use forge::figma_bridge;
// runtime 已抽为独立 crate(polaris-runtime, 目录→crate 强制边界第一块):
// 别名保持 `crate::runtime::…` 全部旧路径零改动;边界由编译器物理保证。
pub use polaris_runtime as runtime;
// 外壳拼装点: 把引擎实现注入内核桥(chat::bridges), 桌面 setup 与 server serve 共用。
pub mod wiring;
// 生图壳桥接: 唯一同时认识 kernel 生图坞与 forge 生图引擎的地方(forge 不认识 kernel, 见其 Cargo.toml)。
pub mod imagegen;
// 自动更新依赖 Tauri updater/restart/package_info → 桌面专属（Docker 用 docker pull 更新）。
#[cfg(feature = "desktop")]
pub mod updater;
// 原生标题栏染色（随主题切换，仅桌面窗口有标题栏）
#[cfg(feature = "desktop")]
pub mod titlebar;

// ── host shim(broadcast 事件壳):server 壳与桌面内嵌协作主机(collab-host)共用 ──
// 已下沉 polaris-runtime(引擎 crate 双壳签名共用它);别名保 `crate::host::…` 旧路径零改动。
pub use polaris_runtime::host;
// ── Docker(server) 外壳：axum HTTP/WS 服务 ──
#[cfg(feature = "server")]
pub mod server;
pub mod sysstat;
// 受控远程执行：iroh 互联的对端在本机跑命令（总开关默认关 + 白名单/Shell 两档 + 全审计）。
pub mod exec;
pub mod fsshare;
// 远程盘挂载:把互联对端的共享目录挂成本机系统盘符(Z:/Y:…),Tailscale 式「连上即多一块盘」。
#[cfg(feature = "collab-host")]
pub mod fsmount;
// 同账号设备网:登录一次云端账号中心,此后自己账号的设备自动互连自动挂盘(Tailscale 式)。
#[cfg(feature = "collab-host")]
pub mod mesh;
// 隔空同屏:文件 → 自包含网页 + 同步消息总线(手机与电脑经 iroh P2P 同看一页)。
// 顺带持有「可读文件白名单」,apihub 的 /api/file 与它共用同一把闸。
pub mod beam;
// 应用直投:把本机在跑的 HTTP 应用反代给手机 —— 电脑当服务器,手机只是个视图。
#[cfg(feature = "collab-host")]
pub mod appproxy;

/// 本机 polaris 服务端口(桌面 = 内嵌主机实际绑定的口;server 壳 = POLARIS_PORT)。
/// 应用直投据此挡住「把 polaris 自己发布出去」造成的请求自噬成环。
#[cfg(feature = "collab-host")]
pub fn hosting_port() -> Option<u16> {
    #[cfg(feature = "desktop")]
    {
        hosting::current_port()
    }
    #[cfg(not(feature = "desktop"))]
    {
        std::env::var("POLARIS_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .or(Some(8080))
    }
}

// ── 桌面外壳入口(run + 适配器):`not(test)` 门控 ──
// 单测二进制永远不会跑 Tauri 事件循环, 却会因编入 run() 把 tauri-plugin-dialog→rfd
// 的静态导入 TaskDialogIndirect(comctl32 **v6** 专有, 需 manifest 激活上下文)带进
// test exe —— 而 tauri_build 只给 app bin 嵌 manifest, test exe 没有 → 测试进程
// 加载即 STATUS_ENTRYPOINT_NOT_FOUND, 一个测试都起不来(2026-07-12 Windows 实测)。
// 从 test 构建里整体剔除 run(), 锚点确定性消失; 真实 app bin(cfg(test)=false)不受影响。
#[cfg(all(feature = "desktop", not(test)))]
use polaris_core::KbLocator;
#[cfg(all(feature = "desktop", not(test)))]
use std::sync::Arc;
#[cfg(all(feature = "desktop", not(test)))]
use tauri::Manager;

/// host 适配器：把板块② `kb` 的 `kb_root()` 适配成 core 的 [`KbLocator`] 契约，
/// 在启动时注入给板块⑤ `polaris-sandbox`，从而打破 `sandbox → kb` 的直接依赖。
/// （架构重构 Phase 1：依赖反转的落地点）
#[cfg(all(feature = "desktop", not(test)))]
struct HostKbLocator;
#[cfg(all(feature = "desktop", not(test)))]
impl KbLocator for HostKbLocator {
    fn kb_root(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(kb::kb_root())
    }
}

/// 把主窗口从托盘/最小化里拉回前台（托盘点击、二次启动唤起共用）。
#[cfg(all(feature = "desktop", not(test)))]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg(all(feature = "desktop", not(test)))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例守卫（须第一个注册）：应用藏在托盘时用户再点桌面图标, 不起第二个进程
        // (两实例会互抢协作主机 8484/网关端口、双份 claude 常驻池), 而是唤起已有窗口。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
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
                if std::fs::metadata(&log_path)
                    .map(|m| m.len() > 5 * 1024 * 1024)
                    .unwrap_or(false)
                {
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
            // 内核桥注入(kb/fable/expert → chat): 须在任何 chat_send 可执行之前。
            wiring::wire_engine_bridges();
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
            // 环境静默托管: 缺 Claude Code 就在后台悄悄装好(全程免 UAC、不打断用户)。
            // uv/Python/Git Bash 已随包内置, 通常这里什么都不用做、一声不吭。
            doctor::start_autopilot(h.clone());
            // 自动更新状态机初始化（记录当前版本 + 持久化路径 + 重启续提示）。best-effort。
            let _ = updater::init(h);
            // 飞书网关「开机自动启动」：若用户开了 auto_start 且凭证齐全，后台自动拉起（不阻塞启动）。
            integrations::feishu::auto_start_if_enabled(h);
            // 寓言计划:感官 API 坞(注册表合并 + 落盘)与回声层「每日做梦」调度。
            sense::init();
            // 语音输入「极速说」:配置 + 个人词表(首启种子)就位,供防污染秒达档使用。
            voice::init();
            echo::start_scheduler(h.clone());
            // 寓言计划:检索枢纽(fable.db 表结构就位;盘点/索引由用户在设置页触发)。
            fable::init();
            // 协作主机自启:上次点过「设为主机」就静默续上(不阻塞启动)。
            hosting::auto_start_if_enabled(h.clone());
            // 云机网关自启重挂:上次挂过牌就重新向云机注册(云机重启后注册表清空需重挂)。
            #[cfg(feature = "collab-net")]
            collab::commands::gateway_auto_reattach();
            // 同账号设备网:入过网就自动接上自己账号的其它设备(隧道+挂盘全自动)。
            // 用户只在第一次填过账号,此后每次开机都是「打开就已经连好」。
            mesh::spawn_if_enrolled();
            // ── 系统托盘（Tailscale 式静默驻留）──
            // 关窗只是藏进托盘, 协作主机/隧道/网关/常驻 claude 池全部继续跑;
            // 左键单击托盘图标唤回主窗口, 右键菜单里才有真正的「退出」。
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
                let show = MenuItem::with_id(app, "tray-show", "显示主界面", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "tray-quit", "退出 Polaris", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show, &quit])?;
                let mut tray = TrayIconBuilder::with_id("main")
                    .tooltip("北极星 · Polaris（后台运行中,点击打开）")
                    .menu(&menu)
                    // 左键=直接唤醒窗口(Tailscale 手感), 只有右键才弹菜单
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "tray-show" => show_main_window(app),
                        // 唯一的真退出入口:走 app.exit → RunEvent::ExitRequested,
                        // 下方 run 回调里的子进程收割/落盘清理照常执行。
                        "tray-quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            show_main_window(tray.app_handle());
                        }
                    });
                if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone());
                }
                tray.build(app)?;
            }
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
        // 关窗 ≠ 退出:主窗口的 X 只是隐藏(托盘继续驻留), 所有后台链路不断。
        // 真退出唯一入口 = 托盘菜单「退出 Polaris」(app.exit)。系统关机/注销时
        // Windows 会绕过 CloseRequested 直接终止进程, 不会被这里卡住。
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(include!("desktop_commands.rs"))
        .build(tauri::generate_context!())
        .expect("error while building Polaris application")
        .run(|_app, event| {
            // App 退出 (关窗 / 主动退出) 时回收所有在飞的 claude 子进程树, 防孤儿继续占端口/CPU。
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                runtime::procs::CHILDREN.kill_all();
                // 常驻 claude agent 进程池(session_pool)不在 CHILDREN 里(按 conv 而非
                // req 管理), 单独收割 —— 否则退出后每个会话遗留一个 ~数百 MB 的孤儿 claude。
                chat::session_pool::kill_all();
                // 对话状态强制落盘:append_message 走「脏标记 + 500ms 合并落盘」,
                // 退出瞬间可能还有最近半秒的消息只在内存里 —— 这里补一刀(不脏则零开销)。
                conv::flush();
                integrations::feishu::shutdown_on_exit(); // 回收飞书 node 桥,防其 autoReconnect 空转成孤儿烧 CPU
                #[cfg(feature = "collab-host")]
                fsmount::unmount_all(); // 删远程盘盘符 + 停 WebDAV 桥,别给系统留死映射
                                                          // 释放全局键盘热键监听:置 ENABLED=false,退出时不再处理热键事件
                                                          //(rdev::listen 无法干净中止是已知限制,置闸 + 进程退出即可接受的清理)。
                #[cfg(feature = "voice-live")]
                voice::live::stop();
            }
        });
}
