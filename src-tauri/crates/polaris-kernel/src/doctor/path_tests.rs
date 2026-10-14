use super::*;

    #[test]
    #[cfg(windows)]
    fn path_contains_dir_is_case_and_slash_insensitive() {
        let p = r"C:\Program Files\PowerShell\7;C:\Users\mi\.local\bin";
        assert!(path_contains_dir(p, r"c:\program files\powershell\7")); // 大小写不敏感
        assert!(path_contains_dir(p, r"C:\Users\mi\.local\bin\")); // 尾斜杠归一
        assert!(!path_contains_dir(p, r"C:\Program Files\nodejs"));
        assert!(!path_contains_dir(p, "")); // 空目标不误命中
    }

    #[test]
    fn claude_dir_from_path_picks_parent_or_npm_prefix() {
        // 普通原生 exe → 取父目录
        let native = PathBuf::from("C:/Users/mi/.local/bin/claude.exe");
        assert_eq!(
            claude_dir_from_path(&native),
            Some(PathBuf::from("C:/Users/mi/.local/bin"))
        );
        // node_modules 路径 → 永不返回那个内部 bin 目录 (要么 npm 前缀, 要么至少不是 .../bin 自身的误用)
        let npmish = PathBuf::from("D:/npm/node_modules/@anthropic-ai/claude-code/bin/claude.exe");
        let dir = claude_dir_from_path(&npmish).expect("应解析出某个目录");
        assert!(
            !dir.ends_with("claude.exe"),
            "返回的应是目录而非文件: {dir:?}"
        );
    }

    /// 所有会改进程 PATH/env 的断言放进同一个测试串行跑, 避免与其他测试并发改 env 抢同一全局态。
    #[test]
    fn prime_and_prepend_behaviour() {
        // prepend 幂等: 首次真加、PATH 命中、再加返回 false
        let marker = if cfg!(windows) { r"Z:\polaris-test-marker-dir-do-not-exist" } else { "/polaris-test-marker-dir-do-not-exist" };
        let first = prepend_process_path(marker);
        let path_now = std::env::var("PATH").unwrap_or_default();
        assert!(first, "首次应真的前插");
        assert!(
            path_contains_dir(&path_now, marker),
            "前插后 PATH 应含该目录"
        );
        assert!(
            !prepend_process_path(marker),
            "已在 PATH 中应返回 false (幂等)"
        );

        // resolve_claude_exe: 若本机装了 claude, 解析出的路径必须真实存在 (Windows 上偏好 .exe)
        if let Some(exe) = resolve_claude_exe() {
            assert!(exe.exists(), "解析出的 claude 路径应存在: {exe:?}");
            #[cfg(windows)]
            {
                let is_exe = exe
                    .extension()
                    .map(|e| e.eq_ignore_ascii_case("exe"))
                    .unwrap_or(false);
                // 本机同时有 .exe 与 .cmd 时, 必须挑 .exe (chat spawn 只认 .exe)
                let has_exe_alt = which_all("claude").iter().any(|p| {
                    p.extension()
                        .map(|e| e.eq_ignore_ascii_case("exe"))
                        .unwrap_or(false)
                });
                if has_exe_alt {
                    assert!(is_exe, "存在 .exe 候选时应优先解析为 .exe, 实得: {exe:?}");
                }
            }
        }

        // 预热不应 panic; 且 (Windows) 若真身 pwsh 存在, 其目录预热后应在进程 PATH 中 → claude 能找到 shell
        prime_path_for_claude_inner();
        #[cfg(windows)]
        if let Some(pwsh) = pwsh_candidates().into_iter().find(|p| p.exists()) {
            let dir = pwsh.parent().unwrap().to_string_lossy().to_string();
            let path_after = std::env::var("PATH").unwrap_or_default();
            assert!(
                path_contains_dir(&path_after, &dir),
                "预热后真身 pwsh 目录应在进程 PATH: {dir}"
            );
        }
    }
