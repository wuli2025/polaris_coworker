    use super::*;

    fn fp(perm: &str, model: &str) -> Fingerprint {
        Fingerprint {
            perm: perm.into(),
            provider_id: "auto".into(),
            work_full: false,
            with_task: false,
            model: model.into(),
            add_dirs: vec!["/kb".into()],
            creative: false,
            cwd: String::new(),
        }
    }

    /// 指纹比较: 任一字段变化都判不等 → 触发杀旧重建。
    #[test]
    fn fingerprint_change_detected() {
        let a = fp("acceptEdits", "");
        assert_eq!(a, fp("acceptEdits", ""));
        assert_ne!(a, fp("plan", ""), "权限档位变化必须判不等");
        assert_ne!(a, fp("acceptEdits", "opus"), "model 变化必须判不等");
        let mut b = fp("acceptEdits", "");
        b.add_dirs.push("/art".into());
        assert_ne!(a, b, "add-dirs 变化必须判不等");
        let mut c = fp("acceptEdits", "");
        c.creative = true;
        assert_ne!(a, c, "创作模式翻转必须判不等(前置指令取向不同)");
        let mut d = fp("acceptEdits", "");
        d.cwd = "/home/me/big-project".into();
        assert_ne!(a, d, "项目工作目录(cwd)变化必须判不等(cwd 进程中途改不了, 须杀旧重建)");
    }

    /// 无忙碌者的默认谓词(旧行为)。
    fn idle_only(_: &u32) -> bool {
        false
    }

    /// LRU 上限 2: 第 3 个会话插入时, 最久未用的被逐出; touch 能续命。
    #[test]
    fn lru_evicts_least_recently_used() {
        let mut core: PoolCore<u32> = PoolCore::new();
        assert!(core.insert("a".into(), fp("p", ""), 1, 0, idle_only).is_empty());
        assert!(core.insert("b".into(), fp("p", ""), 2, 100, idle_only).is_empty());
        // touch a → b 变成最久未用
        core.touch("a", 200);
        let evicted = core.insert("c".into(), fp("p", ""), 3, 300, idle_only);
        assert_eq!(evicted, vec![2], "应逐出最久未用的 b(payload=2)");
        assert!(core.get_mut("a").is_some());
        assert!(core.get_mut("b").is_none());
        assert!(core.get_mut("c").is_some());
    }

    /// 同 conv 重复插入不叠加条目(旧 payload 作为被逐出者返回)。
    #[test]
    fn reinsert_same_conv_replaces() {
        let mut core: PoolCore<u32> = PoolCore::new();
        core.insert("a".into(), fp("p", ""), 1, 0, idle_only);
        let evicted = core.insert("a".into(), fp("q", ""), 9, 10, idle_only);
        assert_eq!(evicted, vec![1]);
        let (f, v) = core.get_mut("a").unwrap();
        assert_eq!((f.perm.as_str(), *v), ("q", 9));
    }

    /// TTL 惰性收割: 空闲 ≥10 分钟的条目被摘出, 新鲜的留下。时钟注入, 不 sleep。
    #[test]
    fn ttl_sweep_reaps_idle_sessions() {
        let mut core: PoolCore<u32> = PoolCore::new();
        core.insert("old".into(), fp("p", ""), 1, 0, idle_only);
        core.insert("new".into(), fp("p", ""), 2, 5 * 60 * 1000, idle_only);
        let reaped = core.sweep_expired(10 * 60 * 1000, idle_only); // old 恰满 10min, new 才 5min
        assert_eq!(reaped, vec![1]);
        assert!(core.get_mut("old").is_none());
        assert!(core.get_mut("new").is_some());
        // touch 后不再过期
        core.touch("new", 14 * 60 * 1000);
        assert!(core.sweep_expired(20 * 60 * 1000, idle_only).is_empty());
    }

    /// 长任务保命核心: 正在跑轮次(busy)的会话, TTL 到点也**绝不**被清扫;
    /// 轮次结束(不再 busy)后才可回收。
    #[test]
    fn ttl_sweep_never_reaps_busy_session() {
        let mut core: PoolCore<u32> = PoolCore::new();
        core.insert("longtask".into(), fp("p", ""), 1, 0, idle_only);
        // 轮次开跑 7 小时后别的对话触发清扫: busy 豁免, 条目必须还在
        let reaped = core.sweep_expired(7 * 3600 * 1000, |v| *v == 1);
        assert!(reaped.is_empty(), "进行中的长任务绝不能被 TTL 误杀");
        assert!(core.get_mut("longtask").is_some());
        // 轮次结束后同一时刻再扫: 正常回收
        let reaped = core.sweep_expired(7 * 3600 * 1000, idle_only);
        assert_eq!(reaped, vec![1]);
    }

    /// 长任务保命之二: LRU 逐出只挑非忙碌者; 全员忙碌时容忍超上限, 不杀活轮。
    #[test]
    fn lru_never_evicts_busy_session() {
        let mut core: PoolCore<u32> = PoolCore::new();
        core.insert("a".into(), fp("p", ""), 1, 0, idle_only);
        core.insert("b".into(), fp("p", ""), 2, 100, idle_only);
        // a 最久未用但正在跑轮次 → 逐出的必须是 b
        let evicted = core.insert("c".into(), fp("p", ""), 3, 200, |v| *v == 1);
        assert_eq!(evicted, vec![2], "忙碌的 a 不可逐出, 应逐出空闲的 b");
        // a、c 都忙 → 第 4 个插入不逐出任何人(包括 d 自己), 暂时超上限
        let evicted = core.insert("d".into(), fp("p", ""), 4, 300, |v| *v == 1 || *v == 3);
        assert!(evicted.is_empty(), "其余全员忙碌时宁可超上限也不杀活轮");
        assert!(core.get_mut("a").is_some() && core.get_mut("c").is_some());
        assert!(core.get_mut("d").is_some(), "新插入者不可被自己逐出");
    }

    /// 并发新建对话不得互相收割(2026-07-29 压测实测的 P0 回归钉子)。
    ///
    /// 旧行为:新 spawn 的会话要到「写完 stdin、派发 job」那步才登记 active_req,
    /// 在此之前 session_busy 判它空闲 → 另一条并发新对话的 trim_over_cap/insert
    /// 顺手就把它逐掉,受害者回头检出时报「会话条目在检出后消失(并发收割)」,
    /// 重试打光后用户看到发送失败(4 路并发 3/4 失败,9 路 8/9 失败)。
    /// 修法是把本轮登记提前到入池之前 —— 这里用「入池即忙碌」来复现那个不变量。
    #[test]
    fn 并发新建的会话入池即忙碌_不会被彼此逐出() {
        let mut core: PoolCore<u32> = PoolCore::new();
        // 三条对话几乎同时新建;每条一入池就算「忙碌」(= 已登记本轮)。
        let all_busy = |_: &u32| true;
        for (i, c) in ["c1", "c2", "c3"].iter().enumerate() {
            let evicted = core.insert((*c).into(), fp("p", ""), i as u32, i as u64, all_busy);
            assert!(
                evicted.is_empty(),
                "{c} 入池不该逐掉任何一条已登记的并发新对话,实际逐出 {evicted:?}"
            );
        }
        for c in ["c1", "c2", "c3"] {
            assert!(core.get_mut(c).is_some(), "{c} 必须还在池里(否则检出会落空)");
        }
        // 另一条对话的检出路径也不能把它们收走。
        assert!(
            core.trim_over_cap("c3", all_busy).is_empty(),
            "全员忙碌时 trim_over_cap 必须原地等待"
        );
        assert_eq!(core.entries.len(), 3, "宁可暂时超编,也不能杀掉在飞的轮次");

        // 反证:若登记没提前(入池时仍判空闲),第三条就会把第一条挤掉 —— 那正是旧 bug。
        let mut old: PoolCore<u32> = PoolCore::new();
        old.insert("c1".into(), fp("p", ""), 1, 0, idle_only);
        old.insert("c2".into(), fp("p", ""), 2, 1, idle_only);
        let evicted = old.insert("c3".into(), fp("p", ""), 3, 2, idle_only);
        assert_eq!(evicted, vec![1], "复现旧行为:未登记的新会话会被后来者收割");
    }

    /// 超编收敛闭环: 全员忙碌导致超上限后, 轮次结束(不再忙)的下一次检出必须把编制
    /// 收回上限 —— 只逐「最久未用 · 非忙碌 · 非当前 conv」, 忙碌期间则原地等待。
    #[test]
    fn trim_over_cap_converges_after_turns_end() {
        let mut core: PoolCore<u32> = PoolCore::new();
        core.insert("a".into(), fp("p", ""), 1, 0, idle_only);
        core.insert("b".into(), fp("p", ""), 2, 100, idle_only);
        // a、b 忙时插入 c → 超编到 3
        core.insert("c".into(), fp("p", ""), 3, 200, |v| *v == 1 || *v == 2);
        assert_eq!(core.entries.len(), 3, "前置: 全员忙碌导致超编");
        // 全员仍忙 → 不裁(当前 conv=c)
        assert!(core
            .trim_over_cap("c", |v| *v == 1 || *v == 2)
            .is_empty());
        assert_eq!(core.entries.len(), 3);
        // 轮次结束(无人忙碌) → 检出 c 时收敛回 2, 逐出最久未用的 a
        let evicted = core.trim_over_cap("c", idle_only);
        assert_eq!(evicted, vec![1], "应逐出最久未用的 a");
        assert_eq!(core.entries.len(), 2);
        assert!(core.get_mut("b").is_some() && core.get_mut("c").is_some());
        // 当前 conv 自己是最久未用者时不可被逐出
        let mut core2: PoolCore<u32> = PoolCore::new();
        core2.insert("x".into(), fp("p", ""), 7, 0, idle_only);
        core2.insert("y".into(), fp("p", ""), 8, 10, |v| *v == 7);
        core2.insert("z".into(), fp("p", ""), 9, 20, |v| *v == 7 || *v == 8);
        let ev = core2.trim_over_cap("x", idle_only);
        assert_eq!(ev, vec![8], "keep_conv=x 受保护, 逐出次久未用的 y");
    }

    /// PromptPlan 首条/续轮拆分: 续轮剔除 Session 段与已发过的 Keyed 段, 保留 Turn 段。
    #[test]
    fn prompt_plan_split_first_vs_continuation() {
        let mut plan = PromptPlan::new();
        plan.session("SYS|".into());
        plan.keyed("script", "SCRIPT|".into());
        plan.turn("RECALL|".into());
        plan.keyed("download", "DL|".into());
        plan.turn("Q1".into());
        assert_eq!(plan.full_text(), "SYS|SCRIPT|RECALL|DL|Q1");
        // 首轮把 script 发过了, download 是本轮新触发 → 续轮只带 未发过的 keyed + turn
        let sent: HashSet<String> = ["script".to_string()].into_iter().collect();
        assert_eq!(plan.continuation_text(&sent), "RECALL|DL|Q1");
        let all: HashSet<String> = plan.keys().into_iter().collect();
        assert_eq!(all.len(), 2);
        // 全部发过后, 续轮只剩每轮都变的部分
        assert_eq!(plan.continuation_text(&all), "RECALL|Q1");
    }
