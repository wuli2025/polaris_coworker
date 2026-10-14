use super::*;

    /// npm 装的两个入口 (前缀根的 shim + node_modules 里的原生 exe) 必须归成**同一份安装**,
    /// 否则一份安装会被误报成「两份并存」的 high 冲突, 天天吓用户。
    #[test]
    fn install_root_groups_npm_entries_together() {
        let shim = PathBuf::from("D:/npm/claude.cmd");
        let native =
            PathBuf::from("D:/npm/node_modules/@anthropic-ai/claude-code/bin/claude.exe");
        assert_eq!(norm(&install_root(&shim)), norm(&install_root(&native)));
        // 原生脚本装的那份是**另一份**, 不能被并进去
        let local = PathBuf::from("C:/Users/x/.local/bin/claude.exe");
        assert_ne!(norm(&install_root(&local)), norm(&install_root(&shim)));
    }

    /// 探版本要挑「能直接跑」的入口: Windows 上 `.exe` > `.cmd` > 无扩展名的 sh 脚本。
    #[test]
    #[cfg(windows)]
    fn best_entry_prefers_exe() {
        let v = vec![
            PathBuf::from("D:/npm/claude"),
            PathBuf::from("D:/npm/claude.cmd"),
            PathBuf::from("D:/npm/claude.exe"),
        ];
        assert_eq!(best_entry(&v), Some(PathBuf::from("D:/npm/claude.exe")));
    }

    /// 掩码必须真的掩住: 不能把明文 key 顺着报告漏到手机端。
    #[test]
    fn mask_hides_secret_body() {
        let m = mask("sk-ant-api03-abcdefghijklmnop");
        assert!(m.starts_with("sk-a") && m.ends_with("mnop"));
        assert!(!m.contains("api03"));
        assert_eq!(mask("short"), "••••"); // 短值整条掩掉, 不泄露长度以外的信息
    }

    #[test]
    fn brief_trims_and_drops_blank_lines() {
        assert_eq!(brief("", 3, 100), None);
        assert_eq!(brief("  \n \n", 3, 100), None);
        assert_eq!(brief("a\n\nb\nc\nd", 3, 100).as_deref(), Some("a\nb\nc"));
        assert_eq!(brief("abcdef", 3, 3).as_deref(), Some("abc…"));
    }

    /// 一份 npm 安装的两个入口 **不得**被报成冲突 —— 这是最容易误报、也最吓人的一条。
    #[test]
    fn single_npm_install_reports_no_conflict() {
        let all = vec![
            PathBuf::from("D:/npm/claude.cmd"),
            PathBuf::from("D:/npm/node_modules/@anthropic-ai/claude-code/bin/claude.exe"),
        ];
        let v = |_: &Path| Some("2.1.220".to_string());
        let out = install_conflicts(&all, Some(&all[1]), Some(&all[0]), &v);
        assert!(out.is_empty(), "同一份安装不该报冲突, 实得: {out:?}");
    }

    /// npm 一份 + 原生脚本一份, 版本还不同 —— 必须同时报「多份并存」与「版本不一致」,
    /// 且两条都是 high (这正是「更新完版本号纹丝不动」的根因)。
    #[test]
    fn two_installs_with_different_versions_report_both_conflicts() {
        let npm = PathBuf::from("D:/npm/claude.cmd");
        let native = PathBuf::from("C:/Users/x/.local/bin/claude.exe");
        let all = vec![npm.clone(), native.clone()];
        let v = |p: &Path| {
            Some(if p == npm.as_path() { "2.0.1" } else { "2.1.220" }.to_string())
        };
        let out = install_conflicts(&all, Some(&native), Some(&npm), &v);
        let keys: Vec<&str> = out.iter().map(|c| c.key.as_str()).collect();
        assert!(keys.contains(&"claude-multi"), "应报多份并存: {keys:?}");
        assert!(
            keys.contains(&"claude-version-mismatch"),
            "版本不同应额外报版本冲突: {keys:?}"
        );
        // 应用解析到 native、终端首个命中 npm → 还要报「两边跑的不是同一份」
        assert!(
            keys.contains(&"claude-app-vs-terminal"),
            "应用与终端解析到不同安装时必须报出来: {keys:?}"
        );
        assert!(out.iter().all(|c| c.severity == "high"));
        // 报告里要能看到两份各自的版本, 否则用户不知道该删哪个
        let multi = out.iter().find(|c| c.key == "claude-multi").unwrap();
        assert!(multi.paths.iter().any(|l| l.contains("2.0.1")));
        assert!(multi.paths.iter().any(|l| l.contains("2.1.220")));
    }

    /// Store 执行别名 (WindowsApps 下的 0 字节占位) 必须被认出来并单独报, 且**不能**被
    /// 当成一份真安装并进「多份并存」—— 否则干净机器会平白多出一条 high 冲突。
    #[test]
    #[cfg(windows)]
    fn store_alias_is_flagged_but_not_counted_as_install() {
        let dir = std::env::temp_dir()
            .join("polaris-verify-test")
            .join("WindowsApps");
        std::fs::create_dir_all(&dir).expect("建测试目录");
        let alias = dir.join("claude.exe");
        std::fs::write(&alias, b"").expect("造 0 字节占位符"); // 真身别名就是 0 字节重解析点
        let real = PathBuf::from("C:/Users/x/.local/bin/claude.exe");

        let out = install_conflicts(
            &[alias.clone(), real.clone()],
            Some(&real),
            Some(&real),
            &|_| Some("2.1.220".into()),
        );
        let keys: Vec<&str> = out.iter().map(|c| c.key.as_str()).collect();
        assert!(keys.contains(&"claude-store-alias"), "应认出别名: {keys:?}");
        assert!(
            !keys.contains(&"claude-multi"),
            "别名不是一份安装, 不该凑成「多份并存」: {keys:?}"
        );
        let _ = std::fs::remove_file(&alias);
    }

    /// 冲突按严重度排序 —— 面板从上往下看, high 必须在最前面。
    #[test]
    fn conflicts_sorted_high_first() {
        let mut v = vec![
            conflict("a", "low", "t", "d".into(), vec![], false),
            conflict("b", "high", "t", "d".into(), vec![], false),
            conflict("c", "medium", "t", "d".into(), vec![], false),
        ];
        let rank = |s: &str| match s {
            "high" => 0,
            "medium" => 1,
            _ => 2,
        };
        v.sort_by_key(|c| rank(&c.severity));
        assert_eq!(
            v.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(),
            vec!["b", "c", "a"]
        );
    }

    /// 冒烟必须**只读**: 它跑在用户真机上, 参数里绝不能出现放开写权限的工具集。
    /// (防回归 —— 有人图省事把 allowedTools 改成全放开, 冒烟就会真去改用户的文件。)
    #[test]
    fn smoke_args_are_read_only() {
        let tools_idx = SMOKE_ARGS
            .iter()
            .position(|a| *a == "--allowedTools")
            .expect("冒烟必须显式限定工具集");
        assert_eq!(SMOKE_ARGS[tools_idx + 1], "Read", "冒烟只放行 Read");
        for bad in ["Write", "Edit", "Bash", "acceptEdits"] {
            assert!(
                !SMOKE_ARGS.iter().any(|a| a.contains(bad)),
                "冒烟参数不得含 {bad}"
            );
        }
    }

    /// 整条链路不 panic, 且字段自洽 (本机装没装 claude 都要能跑完)。
    #[test]
    fn verify_sync_is_self_consistent() {
        let r = env_verify_sync(false);
        assert!(!r.deep);
        assert!(!r.steps.is_empty());
        assert!(
            r.steps.iter().all(|s| matches!(
                s.status.as_str(),
                "ok" | "fail" | "warn" | "skip"
            )),
            "step.status 只能是这四种"
        );
        assert!(
            r.conflicts
                .iter()
                .all(|c| matches!(c.severity.as_str(), "high" | "medium" | "low")),
            "conflict.severity 只能是这三种"
        );
        // 没装 claude 时 ok 必须为 false —— 不能把「查不到」报成健康
        if !r.app_runnable {
            assert!(!r.ok);
        }
        assert!(!r.summary.is_empty());
    }
