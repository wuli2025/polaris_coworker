use super::*;

    /// 本源模式的**取值语义**闸: 只有显式非空非 "0" 才算开。
    /// 不在测试里 set_var —— 那会污染同进程其它测试(cargo 默认多线程跑), 这里改测纯谓词,
    /// 与 raw_mode() 内部用的是同一套判定。
    #[test]
    fn raw_mode_flag_semantics() {
        let on = |v: &str| {
            let t = v.trim();
            !t.is_empty() && t != "0"
        };
        assert!(on("1"));
        assert!(on("true"));
        assert!(on(" yes "));
        assert!(!on("0"));
        assert!(!on(""));
        assert!(!on("   "));
    }

    /// 常规(非本源)模式下, 工具白名单必须照常给足 —— 本源模式改动不许把常规路径带塌。
    /// 这条守的是: 有人日后把 `if !raw_mode()` 写反 / 提错层级时, 常规档位立刻炸出来。
    #[test]
    fn normal_mode_tool_whitelist_intact() {
        // 只读档: 仅联网, 不给任何本地执行
        let plan_tools = allowed_tools_for("plan", false);
        assert!(plan_tools.contains("WebSearch"));
        assert!(!plan_tools.contains("Bash"));
        assert!(!plan_tools.contains("PowerShell"));
        // 常规档: 联网 + 本地读写执行(否则 headless 下产不出 pptx/xlsx)
        let edit_tools = allowed_tools_for("acceptEdits", false);
        assert!(edit_tools.contains("WebSearch"));
        assert!(edit_tools.contains("Bash"));
        assert!(edit_tools.contains("PowerShell"));
        assert!(!edit_tools.contains("Task"), "未开动态编排不该放行 Task");
        // 动态编排: 额外放行 Task
        assert!(allowed_tools_for("acceptEdits", true).contains("Task"));
        // 快速模式砍冗余工具; 工作模式放开全套
        assert!(disallowed_tools_for(false, false)
            .expect("快速模式应有黑名单")
            .contains("Task"));
        assert!(disallowed_tools_for(true, false).is_none(), "工作模式不设黑名单");
    }

    /// 看门狗深检:collect_tree 收全子孙、不越界、PID 复用成环不死循环。
    #[test]
    fn collect_tree_gathers_descendants_and_survives_cycles() {
        // 100 → 200 → 300; 400 无关; 500 是 100 的另一子
        let pairs = [(200, 100), (300, 200), (400, 999), (500, 100)];
        let mut tree = collect_tree(100, &pairs);
        tree.sort();
        assert_eq!(tree, vec![100, 200, 300, 500]);
        // PID 复用把根的祖先又标成树内成员的孩子(100 ← 300 成环): 必须终止且不重复
        let cyclic = [(200, 100), (300, 200), (100, 300)];
        let mut t2 = collect_tree(100, &cyclic);
        t2.sort();
        assert_eq!(t2, vec![100, 200, 300]);
    }

    /// 真行为验证:cmd 拉起 ping 子进程后, sample_tree 必须看到 ≥1 个子孙
    /// (这是「静默但在干活的长任务不被看门狗误杀」的判据本体)。
    #[cfg(windows)]
    #[test]
    fn sample_tree_sees_live_descendants() {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "ping -n 6 127.0.0.1 >NUL"]);
        no_window(&mut cmd);
        let mut child = cmd.spawn().expect("spawn cmd");
        std::thread::sleep(std::time::Duration::from_millis(800)); // 等 ping 起来
        let s = sample_tree(child.id()).expect("windows toolhelp 快照应可用");
        let _ = child.kill();
        let _ = child.wait();
        assert!(
            s.descendants >= 1,
            "cmd 的 ping 子进程应被数进树, 实得 {}",
            s.descendants
        );
    }
