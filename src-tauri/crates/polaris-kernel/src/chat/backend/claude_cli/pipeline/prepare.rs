use super::*;

pub(crate) fn prepare(args: ChatSendArgs) -> Prepared { prepare_impl(args, false) }
pub(crate) fn prepare_pi(args: ChatSendArgs) -> Prepared { prepare_impl(args, true) }
fn prepare_impl(args: ChatSendArgs, pi: bool) -> Prepared {
    // 产物目录 (每个会话一份): claude 把成品文件写到这里 → 侧边栏可预览
    let art_dir = artifacts_dir(args.conversation_id.as_deref());
    let _ = std::fs::create_dir_all(&art_dir);
    let art_before = dir_snapshot(&art_dir);

    // 一体注入: Skill prompt → KB CLAUDE.md + kb_search 召回 → 用户问题
    let current_project_id = args
        .conversation_id
        .as_deref()
        .and_then(conv::project_id_of_conversation);
    // 项目绑定的工作目录(用户在侧栏「选择工作文件夹」设的真实仓库): 本项目下所有对话都以它
    // 为 claude CLI 的 cwd —— 等同终端 `cd <repo> && claude`, 让 AI 直接在用户自己的大项目里干活。
    // 未绑 / 目录已被应用外删除 → None, spawn 回落默认 cwd(与旧行为一致)。
    let project_work_dir = current_project_id
        .as_deref()
        .and_then(conv::project_work_dir);
    // 本源模式: 整轮零注入(见 raw_mode 文档)。这里就把最贵的两处源头掐掉 ——
    // CLAUDE.md 渲染(读盘 + wiki 地图)与下面的 KB 召回(SQLite 检索, 可能还带 AI 扩写),
    // 否则算完再在末尾丢弃, 白付一次延迟。
    let raw = raw_mode();
    let cm_ctx = if raw || pi {
        String::new()
    } else {
        claude_md::render_for_project(current_project_id.as_deref(), &args.prompt, args.use_kb)
    };

    // prompt 改为「分段计划」而非单个大字符串: 旧路径照旧取 full_text()(逐段拼接,
    // 顺序与此前完全一致); 常驻 agent 路径按分段类别做首条/续轮拆分(见 session_pool):
    // - session(): 会话级不变 → 续轮剔除(claude 进程内自持上下文);
    // - keyed():   意图门控块 → 按 key 去重, 本会话没发过且本轮触发才发;
    // - turn():    每轮都变(KB 召回/手改产物/专家块/用户问题) → 续轮照发。
    let mut plan = session_pool::PromptPlan::new();

    // 1(先算). Skill system prompts —— 显式点选 + 按任务意图自动激活(去重)。
    //    先收集进独立列表: 命中创作型 skill 时本轮进「创作模式」(CREATIVE_SKILL_IDS),
    //    下面的 KB 指令与回答风格据此取舍; 拼装顺序保持不变(KB 指令仍在最前)。
    let mut skill_prompts: Vec<(String, String)> = Vec::new();
    let mut injected: Vec<String> = Vec::new();
    // 1a. 用户在对话框显式激活的 skill
    if let Some(ids) = &args.skill_ids {
        for id in ids {
            if injected.iter().any(|x| x == id) {
                continue;
            }
            if let Some((meta, system_prompt)) = skills::find(id) {
                skill_prompts.push((meta.id.clone(), system_prompt));
                injected.push(meta.id);
            }
        }
    }
    // 1b. 按任务意图自动激活（即使对话框没点选）：
    //     创建技能 → skill-creator；网页/浏览器自动化 → cloak-browser
    for (meta, system_prompt) in skills::auto_skills_for_intent(&args.prompt) {
        if injected.iter().any(|x| *x == meta.id) {
            continue;
        }
        skill_prompts.push((meta.id.clone(), system_prompt));
        injected.push(meta.id);
    }
    let creative = injected
        .iter()
        .any(|id| CREATIVE_SKILL_IDS.contains(&id.as_str()));

    // 工作模式: 仅 "work" 进工作模式(纯 Claude Code), 其余(含 None)一律快速模式。决定:
    // ①禁用哪些冗余工具 ②注入哪些约定(可运行项目/长任务仅工作模式)③KB 召回快档 vs 全质量
    // ④上下文预算 ⑤权限/模型默认(前端)。
    let work_full = args.work_mode.as_deref() == Some("work");
    // 知识库召回**只跟着 use_kb 开关走**(2026-08-13 改)。原先快速模式恒真(即便前端没开 KB),
    // 于是每一轮日常对话都要先查一遍库、把命中片段塞进上下文 —— 库里没有相关内容时, 这些
    // 片段纯属噪声, 还配合 KB-First 铁律把模型引向「翻库→没找到→收工」。现在: 点了知识库
    // 才召回, 没点就当它不存在。创作模式照旧跳过(素材已在 prompt 里)。
    let kb_recall = !raw && args.use_kb && !creative;

    // 0. KB 顶层指令 (优先级最高)
    // **门控(2026-08-13 改): 只有用户点了知识库(use_kb)才上 KB-First 全量。**
    // 原先是「非创作模式一律强制」—— 这条要求先 4 步取证(Glob/Grep/Read/双链)再作答, 且配了
    // 「查不到就明确说缺什么」的反幻想护栏。对没开知识库的日常对话, 它约束的是一个根本不存在的
    // 库: 必然查不到 → 按护栏收工在「找不到」, 这正是「事事有回应、事事无着落」的源头
    // (A/B 实证见 prompt.rs `kb_first_directive` 的注释)。知识库是可选能力, 它的强制取证
    // 铁律就该跟着开关走, 而不是常驻。
    // 未开 KB / 创作模式 → 只留精简版的「资料与指令隔离」安全条款: 这是提示词注入防线,
    // 只要模型还会 Read 素材文件就必须在, **不随任何模式豁免**, 且它本身不要求先取证、
    // 不构成不动手的理由。
    // (会话级: creative 已入常驻指纹, 翻转即重建, 会话内该段稳定不变)
    if !creative && args.use_kb {
        plan.session(format!("{}\n\n---\n\n", kb_first_directive()));
    } else {
        plan.session(format!("{}\n\n---\n\n", kb_isolation_directive_light()));
    }

    // skill 逐个成为门控段(key=skill id): 第 5 轮才点选/意图激活的 skill 也能在
    // 常驻续轮里补发, 不因「续轮剔除」永远缺席。(分隔符从整节末尾一个改为每个 skill
    // 各带一个, 语义等价。)
    for (id, sp) in skill_prompts {
        plan.keyed(format!("skill:{id}"), format!("{sp}\n---\n\n"));
    }

    // 1.5 回答风格约定。普通对话 = Codex 式扁平(砍废话); 创作模式 = 「回复克制、成品丰满」
    //     —— 扁平风格的「压缩字数」倾向会渗进幻灯片/网页文案, 把成品也写干瘪。
    if creative {
        plan.session(format!("{}\n\n---\n\n", creative_style_directive()));
    } else {
        plan.session(format!("{}\n\n---\n\n", reply_style_directive()));
    }

    // 静态指令门控总闸: POLARIS_PROMPT_FULL=1 恢复全部静态指令全量注入(排障/对比用)。
    let prompt_full = prompt_full_forced();
    // 产物意图只算一次, 输出约定与脚本公约两个门共用。
    let artifact_intent = detect_artifact_intent(&args.prompt);

    // 2. 输出文件约定 —— **已移除 (2026-08-13)**。
    //    原先每轮都注入(全量 ~700 tokens / 精简版 2 句): 要求成品必须落到会话产物目录、
    //    末尾用绝对路径罗列、网页要单文件自包含、过程产物不许列出并跑完清理。目的是让右侧
    //    边栏能预览成品。代价是模型每轮都被一套「文件该写到哪、该怎么汇报」的格式要求箍着,
    //    与终端里裸调 claude 的行为背离(那里文件就落在 cwd, 想写哪写哪)。
    //    按用户决定直接删掉: 成品此后落在 claude 的 cwd(= 项目绑定的工作目录), 与裸 CLI 一致。
    //    **连带影响(有意接受)**: 右侧产物边栏靠 art_dir 的前后快照(dir_snapshot)发现新文件,
    //    模型不再被告知该目录 → 边栏基本不会再自动冒出成品。art_dir 本身与 --add-dir 放行
    //    保持原样, 不影响别处; `output_convention`/`output_convention_lite` 两个函数暂留在
    //    prompt.rs(无调用点), 想恢复把上面这段门控加回来即可。

    // 2.1 可运行项目约定 (板块⑮) — 要跑起来的应用(尤其前后端)打包成带运行清单的项目文件夹,
    //     用户在右侧点「运行」即一键启动前后端并内嵌预览。创作模式跳过(成品是单文件);
    //     **仅工作模式注入**: 快速模式只为「查库+答」, 不打包工程, 注入只会膨胀 prompt。
    if !creative && work_full {
        plan.keyed(
            "conv:project",
            format!("{}\n\n---\n\n", project_convention(&art_dir)),
        );
    }

    // 2.2 长任务铁律: 回合结束 = claude 退出 = 其整棵子进程树被回收(防孤儿)。模型若把出片/上传/
    //     下载等耗时任务放后台再结束回复, 任务必死且无人知晓。
    //     门控(2026-08-13 放宽): 原为「仅 work + 创作模式」, 理由是快速模式只做秒级问答。但
    //     快速模式是默认档, 用户在这里照样会说「帮我装个即梦 CLI」「下载这个模型」—— 那时模型
    //     拿不到本铁律, 就会把 npm i / 下载丢后台并回一句「安装中, 好了通知你」, 回合一结束
    //     进程树被回收、任务真死 → 这正是「事事有回应、事事无着落」的另一半成因(前一半是
    //     KB-First 缺执行豁免)。故改为与脚本公约同闸: 凡是要动手干活的轮次, 两段成对注入。
    if work_full
        || creative
        || prompt_full
        || artifact_intent
        || skills::detect_dev_intent(&args.prompt)
        || detect_script_intent(&args.prompt)
        || skills::detect_download_intent(&args.prompt)
    {
        plan.keyed(
            "conv:longtask",
            format!("{}\n\n---\n\n", longtask_convention()),
        );
    }

    // 2.21 脚本执行公约: 模型爱写临时脚本干活, 但用户机器上的 `python`/`python3`
    //      极可能是 Microsoft Store 的 0 字节占位符(实证截图: python3.exe 是占位符 → 做 PPT
    //      只能降级成 HTML), 裸调必失败或假成功。统一要求: Python 一律 `uv run` + PEP 723 内联
    //      依赖, 禁裸调 python/系统 pip; Node 脚本先自检可用性。uv 由环境医生预置并已注入 PATH
    //      (见 doctor::ensure_uv_on_process_path), 三端(win/mac/docker)同构。
    //      门控(此前 always-on, ~1850 tokens): work 模式(干活模式)恒注入; 创作模式恒注入
    //      (出 PPT/视频要跑导出脚本); 或消息命中 开发意图(detect_dev_intent) / 脚本·执行·
    //      批量·文件处理意图(detect_script_intent) / 产物意图(生成 xlsx/pptx/pdf 等要靠
    //      脚本落地)时注入; 普通闲聊/短问答跳过。
    if prompt_full
        || work_full
        || creative
        || artifact_intent
        || skills::detect_dev_intent(&args.prompt)
        || detect_script_intent(&args.prompt)
    {
        plan.keyed("conv:script", format!("{}\n\n---\n\n", script_convention()));
    }

    // 2.22 大文件下载公约 (按需注入): >200MB 大文件禁单线 wget, 必须 aria2c 多连接分段并行。
    //      此前每轮都注入, 但绝大多数对话(尤其办公)从不下大文件 —— 改成**仅下载意图命中才注入**,
    //      办公/日常提示词随之瘦身, 首 token 更快、也少一段无关上下文诱导模型跑题。
    if skills::detect_download_intent(&args.prompt) {
        plan.keyed(
            "conv:download",
            format!("{}\n\n---\n\n", download_convention()),
        );
    }

    // 2.23 高效检索公约: 大库上 grep/glob 全树盲扫会慢 —— 注入「先缩范围(path+
    //      glob/type 过滤)+ 结果封顶 + 用 rg 不用 GNU grep」铁律, 配合 KB 根的 .ignore 让
    //      Grep 工具(底层 ripgrep)默认就跳重目录, 检索默认快。
    //      门控(此前 always-on, ~1000 tokens): work 模式 / 显式开 KB(use_kb) / 消息含
    //      文件·查找·检索意图(detect_search_intent) → 全量; 否则一句话精简版。
    if prompt_full || work_full || args.use_kb || detect_search_intent(&args.prompt) {
        plan.keyed(
            "conv:search-full",
            format!("{}\n\n---\n\n", search_convention()),
        );
    } else {
        plan.keyed(
            "conv:search-lite",
            format!("{}\n\n---\n\n", search_convention_lite()),
        );
    }

    // 2.15 分批长任务: 超长生成(60 页 PPT 这类)拆成有界批次, 每轮只建 ≤K 个 pending 单元,
    //      用 polaris.build.json 清单做 checkpoint, 断线从下一个 pending 续跑 ——
    //      规避单轮输出过长把流式连接拖死(socket closed → 进程坏死)。
    if args.batch_build {
        let bs = args.batch_size.unwrap_or(8).clamp(1, 50);
        plan.keyed(
            format!("conv:batch:{bs}"), // 批量大小变了按新块补发
            format!("{}\n\n---\n\n", batch_build_directive(&art_dir, bs)),
        );
    }

    // 2.5 目标模式: 用户设了完成条件时, 注入「持续推进直到达成」指令
    if let Some(goal) = args
        .goal
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
    {
        plan.keyed(
            format!("conv:goal:{goal}"), // 目标文本变了按新块补发
            format!("{}\n\n---\n\n", goal_directive(goal)),
        );
    }

    // 2.65 动态编排: 把本轮当成多智能体编排, 用 Task 子代理并行扇出, 每条流水线
    //      实现 -> 对抗式校验 -> 修复, 最后汇总(详见 dynamic_workflow_directive)。
    if args.dynamic_workflow {
        // with_task 已入常驻指纹(工具面不同), 该段在会话内稳定; 仍用门控 key 表达。
        plan.keyed(
            "conv:dynwf",
            format!("{}\n\n---\n\n", if pi {
                dynamic_workflow_directive().replace("Task", "subagent")
            } else { dynamic_workflow_directive().to_string() }),
        );
    }

    // 2.68 专家团 / 智能匹配
    //   - auto-match（默认）: 每轮自动路由最合适的 1~3 位专家，注入「智能匹配·专家团」视角块。
    //     这是默认对话体验——一上来就用智能匹配专家团，无命中信号则不注入（闲聊不被套专家）。
    //   - expert-team: 显式专家团；检测到多专家任务时召集成队并注入分工。
    //   注意: 这里只**计算**专家块, 注入推迟到「## 用户问题」紧前(见步骤 4)——
    //   此前专家块排在第 6 段, 与用户问题之间隔着 KB 概览/召回/记忆地图/对话历史数千 token,
    //   专家准则在注意力上被淹没; 约束贴着问题放才最有效。
    // 专家团一律走内核桥(引擎未拼装时 expert_bridge()=None → 不注入, 语义同无命中)。
    let expert_block: Option<String> = if pi { None } else { match args.agent_mode.as_deref() {
        Some("expert-team") => crate::chat::bridges::expert_bridge().and_then(|eb| {
            if eb.detect_multi_expert_task(&args.prompt) {
                current_project_id.clone().and_then(|project_id| {
                    // 多专家召集: 注入每位主选专家的完整准则正文(而非只有名字+标签)
                    eb.team_block_spawn(project_id, args.prompt.clone())
                })
            } else {
                // 单专家任务也给个智能匹配视角，不必非要凑成多人团
                eb.route_block(&args.prompt)
            }
        }),
        // 默认（None 或 "auto-match"）走智能匹配；"single-agent" / "single-expert" 不在此注入。
        Some("auto-match") | None => {
            crate::chat::bridges::expert_bridge().and_then(|eb| eb.route_block(&args.prompt))
        }
        _ => None,
    } };

    // 2.7 生图能力检测: 用户想生成图片, 但供应商坞里全是文本/代码大模型, 没有一个能真生图。
    //     注入「当前供应商 + 能否真生图」的事实, 让 image-gen 技能据此决定:
    //     不支持 → 用中文说清楚, 并改用「很有图片质感的 HTML」兜底。
    //     模型有时不遵守「开头摊牌」指令(会先说「已生成」), 所以由后端在回复最前面
    //     **确定性地**插入这句中文说明(见下方 image_notice), 保证用户一上来就看到。
    let image_notice: Option<String> = if skills::detect_image_intent(&args.prompt) {
        let (provider_name, supported) = crate::provider::image_gen_capability();
        // 每轮段: 生图能力随「设置→生图模型」随时变, 常驻续轮也要重报当下事实。
        plan.turn(format!(
            "{}\n\n---\n\n",
            image_capability_directive(&provider_name, supported, &art_dir)
        ));
        if supported {
            None
        } else {
            // 注意: 未配置时 image_gen_capability() 回的 provider_name 是**空串**
            // (生图坞是独立的一张表, 没配就没有"当前家"这回事) —— 这句话里不能再引用它,
            // 否则会渲染成「你当前使用的「」是文本大模型」。
            Some(
                "> ⚠️ **说明**：你还没配生图模型，聊天用的大模型**不支持生成真实图片**。下面用一张「HTML 模拟的画面」来替代；如需真实 AI 生图，到「设置 → API 供应商 → 生图模型」加一家并填 Key（MiniMax / OpenAI / 豆包方舟都行），配好后再让我重画即可。\n\n"
                    .to_string(),
            )
        }
    } else {
        None
    };

    // 3. CLAUDE.md 上下文 (KB 地图 + 项目人格) —— 会话级: wiki 结构轮间基本不动,
    //    常驻续轮不重发(变更会在下次进程重建时带上; 权衡: 省数千 token vs 极小的时滞)。
    if !cm_ctx.is_empty() {
        plan.session(format!("{cm_ctx}\n\n---\n\n"));
    }

    // fable 状态只查一次(打开 SQLite + COUNT): 家底概览与强制召回共用同一份结果。
    // 两者现在都只在开了 KB 时才要 → 没开就整个跳过这次查询, 省一次开库。
    let fable_st = if kb_recall {
        crate::chat::bridges::kb_bridge().and_then(|b| b.fable_status())
    } else {
        None
    };

    // 3.15 知识库家底概览: 让模型一开口就答得清「你的库在哪 / 有什么」, 报全四层
    //      (妈妈库 wiki / raw / output / memory)家底。
    //      门控(2026-08-13 改, 原为「始终注入」): 跟着 use_kb 走 —— 没开知识库的对话里,
    //      报一遍家底只会诱导模型往库里钻。
    if kb_recall {
        let ov = kb_overview_block(fable_st.as_ref());
        if !ov.is_empty() {
            plan.session(format!("{ov}\n\n---\n\n")); // 会话级: 家底轮间基本不变
        }
    }

    // 3.2 双库召回: 后端先替模型查两个库(妈妈库 wiki 权威 + 外库 raw/output 混检), 命中
    //     片段直接喂进上下文。**两种模式现在都只在用户点了知识库时才查**(2026-08-13 改;
    //     此前快速模式恒真)。创作模式跳过(素材已在 prompt 里)。
    //     下面的快档/全质量之分仍按 work_full 走 —— 那是「怎么查」, 与「查不查」无关。
    if kb_recall {
        // 快速模式提速核心: 召回走「快档」—— 双车道(grep + 向量)融合但**跳过重排 API**, 把这步
        // 从网络主导的 ~1.8s 砍到 ~250ms(重排是 hybrid 唯一的慢源, 见 retrieve::search 的
        // mode=="hybrid" 闸)。预算也调小, 片段少 → 提示词更短、首 token 更快。
        // 工作模式(纯 Claude Code, 手动开 KB)走全质量 hybrid(带重排)。
        let fast_recall = !work_full;
        let recall_budget = if work_full {
            FORCED_RECALL_BUDGET
        } else {
            FAST_RECALL_BUDGET
        };
        let recall =
            forced_recall_block(&args.prompt, recall_budget, fast_recall, fable_st.as_ref());
        if !recall.is_empty() {
            plan.turn(format!("{recall}\n\n---\n\n")); // 每轮段: 召回是本轮问题的增量取证
        }
    }

    // 3.4 回声层记忆地图: 「每日做梦」从历史对话蒸馏出的、关于主人本人的记忆(偏好/规则/
    //     纠正过的做法)。跨项目全局, 注地图不注全文(PRD v5 §6.3③) —— 让灵魂不仅记得盘里的
    //     往事, 也记得与主人相处的方式。memory/ 为空时返回空串、零开销。
    {
        let mmap = memory_map_block(MEMORY_MAP_BUDGET);
        if !mmap.is_empty() {
            plan.session(format!("{mmap}\n\n---\n\n")); // 会话级: 每日做梦才更新
        }
    }

    // 3.5 跨对话产物地图: 本项目其它对话生成过、仍在磁盘上的文件(绝对路径)。
    //     让模型可直接 Read「上次那个文件」, 用户不用重新拖拽。当前对话排除(它的文件
    //     已在下面的对话历史里出现)。
    if let Some(pid) = current_project_id.as_deref() {
        let amap =
            project_artifacts_block(pid, args.conversation_id.as_deref(), ARTIFACT_MAP_BUDGET);
        if !amap.is_empty() {
            // 会话级(权衡): 常驻会话存续期内, **其它对话**新生成的文件对本会话不可见,
            // 直到进程重建(TTL/指纹变化)。本对话自己的产物经工具事件天然在上下文里,
            // 跨对话取新文件是低频动作 → 换取每轮省一整段地图, 值得。
            plan.session(format!("{amap}\n\n---\n\n"));
        }
    }

    // 3.6 对话历史: 本对话最近若干轮原文(预算封顶), 让同一对话能接上文 ——
    //     此前每轮都是无状态新进程, claude 看不到上一句, 这里补上。
    if let Some(cid) = args.conversation_id.as_deref() {
        // 快速模式历史预算调小: 秒级问答用不到大段历史/代码上下文, 少喂 → 输入 token 少 → 更快;
        // 工作模式要更全的上下文(多文件/多轮重构), 保留较大预算。
        let hist_budget = if work_full {
            HISTORY_CTX_BUDGET
        } else {
            FAST_HISTORY_BUDGET
        };
        let hist = history_block(cid, hist_budget);
        if !hist.is_empty() {
            // 会话级: 这是常驻模式最大的省 —— 续轮的上文就在 claude 进程自己的
            // 上下文里(它亲历了前几轮), 重发历史纯属浪费; 进程重建(首条)时才需要
            // 这份落库历史来"回忆"。
            plan.session(format!("{hist}\n\n---\n\n"));
        }
    }

    // 3.7 用户手改产物提醒: 本对话的产物文件在上一轮之后被用户手动编辑过(右抽屉编辑器
    //     保存或外部工具改动)时, 明确告知模型「磁盘为准、改前必须 Read」—— 对齐 CLI 的
    //     文件真源体验, 防止模型凭对话历史里的旧版本整体重写、毁掉用户的手工修改。
    if let Some(cid) = args.conversation_id.as_deref() {
        let edited = user_edited_artifacts_block(cid);
        if !edited.is_empty() {
            plan.turn(format!("{edited}\n\n---\n\n")); // 每轮段: 手改检测就是逐轮增量
        }
    }

    // 3.9 专家块(智能匹配/专家团): 在 2.68 计算, 此处注入 —— 贴着用户问题, 准则不被
    //     KB/历史大段上下文稀释。
    if let Some(block) = expert_block {
        plan.turn(format!("{block}\n\n---\n\n")); // 每轮段: 专家路由按本轮问题算
    }

    // 4. 用户原始问题
    if raw {
        // 本源模式: 把上面所有可能已入计划的段整个丢掉, 只留用户原话 —— 连 `## 用户问题`
        // 这个包装标题都不加, 让 CLI 收到的字节与你在终端 `claude` 里敲进去的完全一致。
        // 这里做「整盘重置」而不是逐段加 `if !raw` 判断, 是因为注入点有 19 处且后续还会加,
        // 逐点加判断迟早漏一处; 重置是收敛的 —— 无论上面新增多少段, 本源模式恒为空。
        // 上面已把最贵的 CLAUDE.md 渲染与 KB 召回按 raw 短路掉, 剩下的都是纯字符串拼接, 丢弃不心疼。
        plan = session_pool::PromptPlan::new();
        plan.turn(args.prompt.clone());
    } else {
        plan.turn(format!("## 用户问题\n\n{}", args.prompt));
    }

    Prepared { args, art_dir, art_before, project_work_dir, plan, image_notice, work_full, creative }
}
