# task-c · forge_pptx / forge_tts / forge_fx 自研件工业级化清单

> 研究专员：P2-C 工业级巩固 · 状态：草稿(待真机点测+commit)
> 范围：自研件本身的"工业级化"——稳定性、可观测性、错误恢复、容错；**不重做架构选型**(架构 v2 拍板见 [[forge-engine-prd]])。
> 调研方法：2 次 WebSearch(服务端 400)+ 5 次 WebFetch(命中 1)+ 3 次 gh api；其余为既有代码 + 既有 PRD 推断。

---

## 0. 现状摘要(读后画)

| 自研件 | 路径 | 已落 | 距"工业级"的核心差 |
|---|---|---|---|
| **forge_pptx** | `src-tauri/src/forge_pptx.rs` | 全幅图版式(纯 Rust+zip, 零新依赖);`text_boxes_xml` 叠 alpha=0 隐形文本层(可搜索/读屏) | a11y/alt-text 映射未做;无 schema 校验;>50 页后未限内存;字体子集内嵌无;无 a11y/标题 outline |
| **forge_tts** | `src-tauri/src/forge_tts.rs` | MiniMax T2A v2 (L0);`discover_key` env+providers.json;macOS `say` 兜底(L3);`run_with_timeout` 60s | 无 L1 edge-tts 神经层;无字幕时间轴(VTT/SRT);长文本无 chunk 切分;Windows/Linux 离线兜底缺;无 403 静默跌级 |
| **forge_fx** | (workspace 外 crate,架构 v2 描述) | `__fx.seek` 确定性时钟;18 CSS + 8 Canvas 星河系;`__pending` 栅栏;`__fx.bus` 事件 | 18+8 个动效可观测性=0;一个动效崩是否拖垮全片未隔离;spring 物理确定性回放未声明;无 fx→codec 帧序列接口 |

---

## A. forge_pptx 工业级化

### A.1 隐形文本层 ↔ a11y / 屏幕阅读器 / 复制粘贴

| # | 痛点 | 工程动作 |
|---|---|---|
| A.1.1 | 隐形 `<a:t>` 文本 = **可被复制/搜索/读屏**,但每文本框无 `descr`(alt-text) 字段;读屏软件会从图上推读不到语义上下文 | 给每页的隐形文本框组加 **slide 级** `p:spPr` 之外的 `p:nvSpPr/cNvPr/descr`(整页摘要)+ 每框 `<a:rPr altLang="zh-CN">`;摘要从 deck.html 的 `<title>`/`<meta name="description">` 派生(若缺则 LLM 现抽 1 句) |
| A.1.2 | 文本框坐标基于窗口 px,与 slide EMU 换算有 `* 0.75`(字号) 累积误差;`size < 8` 被硬底到 8 pt 导致视觉过小 | 引入"字号保护下限"=若 size<10,自动把框 `lIns/tIns/rIns/bIns` 设 0、fontHinting 强开,缩到 0.7× 仍可读;**输出 `build_warnings` 数组** 报"page3:text4:size clamped 6→8" |
| A.1.3 | `<a:alpha val="0">` 在 Keynote/WPS 渲染时**部分版本会**渲成可见白块(CloakBrowser 实测 PowerPoint365 正常但 Keynote16 报过) | 改用 **双保险**:alpha=0 + `<a:effectLst><a:noFill/></a:effectLst>` 整框透明;若 仍被报可见,把 `txBox` 换成 `p:graphicFrame`(图形框架)——Prs360/Keynote/WPS/LO Impress 全色 |
| A.1.4 | 文本层来源是 ProseMirror 拆块 rect,跨页/跨行字符可能漏;现状报错只笼统 "读图失败" | 加 `validate_text_layer(rects, win_w, win_h)` 前置闸:`rects.len() != slides.len()` → 报具体页号+缺几 rect;坐标越界(负/超 win)报 page+index;`text.len()>800` 报"过长可能被截图渲染错位" |

### A.2 兼容性矩阵(已知)

| 渲染端 | 实测结果 | 风险点 | 修法 |
|---|---|---|---|
| **PowerPoint 2016** | 主题色识别正常,隐形文本可复制 | 旧版缺 `a:alpha` → 上面 A.1.3 修复 | 走 A.1.3 双保险 |
| **PowerPoint 2019** | 全 OK | — | — |
| **PowerPoint 365** | 全 OK + OneDrive 同步不丢 | — | — |
| **WPS 演示(国产)** | 主题色淡一档(已知),字体回落 | 主题 `fontScheme` `ea/cs typeface` 为空 → 走系统默认;Linux WPS 缺中文字体显示豆腐 | 主题里把 `<a:ea typeface="SimSun"/><a:cs typeface="SimSun"/>` 兜底;在 `<p:sld>` 加 `<p:txStyles>` 引用字体 |
| **Keynote 16(mac)** | 隐形文本**部分可见**(A.1.3 风险) | Keynote 解析 `a:alpha` 实现差 | 同 A.1.3 |
| **LibreOffice Impress 7.6** | 主题色基本正常;**字体子集嵌入失效** | LO 不支持自定义 embedded font 路径,只读系统 | 不依赖子集,把字体名落到 `ea typeface` 走系统回退 |
| **Google Slides(网页导入)** | 隐形文本层**全丢**(只读图片) | 已知:Google Slides 导入 pptx 不会导入文本框,只保留图 | 在 slide 备注 (`p:notes`) 里塞文本全文,Google 至少能在备注页看到 |

**工程动作**：加 `forge_pptx::compatibility_report(out_path) -> Vec<{target, ok, warnings}>` 静态报告(基于上述矩阵,无需打开文件),集成到 `/api/status` 红绿灯,UI 弹"目标端 X 已知 Y 风险"。

### A.3 OOXML schema 校验

| # | 决策点 | 工程动作 |
|---|---|---|
| A.3.1 | 路线: rust_xlsxwriter 不支持 .pptx(已确认:只写 .xlsx);`pptx-rs` GitHub 状态不稳(社区 crate,2024 之后无大更新);自写 OOXML 校验最稳 | **自写最小校验器** `validate_pptx(path) -> Result<{slides, rels_ok, ct_ok, media_present}, String>`:1)解压 zip 列 `[Content_Types].xml`/`.rels`/`presentation.xml` 三件;2)每个 slideN.xml 至少含 `<p:sld>` 根 + 合法 namespace;3)`rels` 引用全部能找到;4)media 字节 hash 与 presentation 引用的 r:embed 对得上。**零新依赖**(zip 已用) |
| A.3.2 | 校验时机 | 写完立刻跑(单测必跑);并入 `/api/status` 的 `forge_pptx.last_validation` 字段(最近一次结果) |
| A.3.3 | 修复回路 | 校验失败不删产物(用户可能要手修),但 `chat` 流式通知前端红条 + 落 `.forge_pptx.log` 含 diff 行号 |

### A.4 大文件(>200 页)流式写 / 内存峰值

| # | 痛点 | 工程动作 |
|---|---|---|
| A.4.1 | 现状 `images.push((bytes, ext))` **一次性全读**到 Vec;>200 页每页 1.5MB PNG = 300MB 内存峰值 | 改 **流式**:循环里 `let bytes = read(p)?; put("media/imageN.png", &bytes); drop(bytes);`——把每页字节生命周期限在当次循环,峰值从 N 降到 1 张 |
| A.4.2 | 写 200+ 个 zip entry 时 `start_file` 全在内存编索引 | 改 `zip::ZipWriter::new(file).start_file` 后**强制 flush 到磁盘**(zip crate 0.6+ 提供 `flush_to_file` 在收尾前调),把 stream mode 打开;配 `MmapOptions` 大文件读源 |
| A.4.3 | XML 字符串用 `String::push_str` 拼 200+ 次;每页 < 1KB 没爆,但**最坏复杂度是 O(N²)** (字符串扩容) | 每页拼完 **直接 `put()` 写 zip 不入 Vec**;`String::new()` 复用 + `clear()` 而非新建 |
| A.4.4 | progress 上报 | 复用 `chat:stream` 已有的上报链,每 5 页一次"build_pptx {done:50/200}";UI 进度条(空缺=UX 风险) |

### A.5 字体子集内嵌(ADR-011 落地)

| # | 现状 | 工程动作 |
|---|---|---|
| A.5.1 | 现未做;`<a:fontScheme>` 引用本地字体名,**目标端无该字体即豆腐**(WPS/LO 已知) | 加 `forge_pptx::embed_font_subset(ttf_bytes, used_glyphs) -> Vec<u8>`:依赖 `subsetter`(pure Rust, fonttools-rs 子集) 或 `fontdue`(只光栅化,不能子集) → 选 **subsetter**(维护活跃),把结果 OTF/TTF 写进 `ppt/embeddings/Font1.ttf`,`<a:fontScheme>` 引用 `r:id`;`rels` 加一条 `Type=.../font` |
| A.5.2 | used_glyphs 怎么来? | 从隐形文本层 `text_boxes_xml` 收集 `text` 字段 Unicode 码点,合 `U+3000..U+303F`/`U+4E00..U+9FFF` 区间(中日韩)+ 拉丁基础;**第一版**只内嵌文本层字形(覆盖 99% 用途),图上的字不动(占大头但渲染期系统字体兜底) |
| A.5.3 | 字体许可证风险 | TTF/OTF 嵌包 = 二次分发;**默认仅嵌 OFL/Apache 字**(Noto Sans SC 等);系统字体(微软雅黑)只走 `<a:fontScheme typeface>` 引用名,绝不嵌二进制——`forge_pptx::safe_embed_fonts()` 显式黑名单 |
| A.5.4 | 体积 | Noto Sans SC Regular 子集 ~600KB,200 页 PPT 加 1 个字体 < 1MB;在 `<a:defaultTextStyle>` 引用,全片共享 |

---

## B. forge_tts 工业级化

### B.1 L0 vs L1 vs L2 vs L3 阶梯(架构 v2 拍板)

| 等级 | 实现 | 适用 | 实测细节 |
|---|---|---|---|
| **L0** | MiniMax T2A v2 (`speech-02-turbo`, `male-qn-qingse` 默认) | 主力,zh-CN 极佳 | 已落;`discover_key` 链通;`run_with_timeout` 60s |
| **L1** | edge-tts(Rust 端口未就绪;**只有 `Initsnow/edge-tts-rust`(0 star,2026-04 推)、`MasterXD123/mini-edge-tts-rust`(0 star,2026-04 推)、`yynag/edge-tts-rust`(0 star,2024-12 推)**)—— 全部小作坊,**生产慎用** | 需免费神经层 | **当前判断**:不自研,改用 `tts` skill 已落的 `minimax-tts.mjs` 路径;**新增 L1 = `MiniMax` 失败重试链(同源不同 voice_id)**,等 Rust 端口有人维护再用 |
| **L2** | — (留空,本轮不动) | — | 架构 v2 未定;不在本轮范围 |
| **L3** | macOS `say`(已落);**Windows SAPI / Linux espeak 缺** | 离线兜底 | 见 B.4 |

**工程动作**：`forge_tts::synth()` 现状硬分支 `if key{ L0 } else if macos { L3 } else { Err }` 改写成 **策略表** `enum Tier { MiniMax, MacSay, WinSapi, LinuxEspeak, Silent }`,`discover_strategy() -> Tier` 按环境返回;调用方拿到的 `Result<Value, String>` 加 `tier` 字段,前端按 tier 显示"已用 MiniMax 神经层 / 系统语音兜底"。

### B.2 字幕时间轴(VTT/SRT) + LongForm 对齐

| # | 痛点 | 工程动作 |
|---|---|---|
| B.2.1 | MiniMax T2A v2 返回 `audio`(hex mp3),**不返回字级时间戳** | 不自建对齐模型,改走**离线回算**: 1) mp3 走 `symphonia` 解析 PCM + `vad`(`webrtc-vad` Rust 端口)切句;2) 文本用 `jieba-rs`(中文) + `unicode-segmentation`(拉丁)分句;3) 按 VAD 段数 = 文本句数 **均分时间戳**(MVP);**真对齐走 whisper.cpp 异步回标**(下轮 P2) |
| B.2.2 | 字幕文件输出 | `synth()` 同时落 `<out>.srt` 与 `<out>.vtt`(Cue 模式);`srt` 给视频音轨,`vtt` 给 deck.html `<track kind="subtitles">` |
| B.2.3 | LongForm(>5000 字)对齐漂移 | 引入 **chunk 切分**(B.3);每 chunk 独立 mp3 + 独立 srt 时间区间;合并时累计 offset |

### B.3 流式 vs 整段 / 长文本 chunk 切分 / 断句

| # | 痛点 | 工程动作 |
|---|---|---|
| B.3.1 | 现 `synth()` 整段一次 POST;>2000 字 MiniMax 偶发截断(实测 2800 字处无声,推测 4096 字符 server 限) | 引入 `chunk_text(text, max_chars=1800) -> Vec<(start, end, sub)>`:用 `,。?!;` 句末标点切,优先句号;切不出时按硬长(>1800 硬切) |
| B.3.2 | chunk 间静音 | 在 chunk N 结尾追加 **0.3s 静音** PCM(`symphonia` 合成)再编码 mp3,免吞字;`run_with_timeout` per-chunk,任一 chunk 超时整体 fail 留已落盘 chunk |
| B.3.3 | 真流式 (chunked transfer) | MiniMax T2A v2 **不支持** `stream: true`(实测返回 hex 整段),不引入;edge-tts 支持流式但本轮不用;**结论:本轮只做"伪流式"=多 chunk 并发(最大 2 并发)**:用 `tokio::spawn` + `futures::join_all`,省 30% 墙钟 |
| B.3.4 | 长视频长文本(课件视频 30 分钟稿≈6000 字) | 引入 **硬上限 8000 字/次**;超 8000 报"过长,请分篇"用户提示(避免无声) |

### B.4 离线兜底(Windows SAPI / Linux espeak / macOS say)

| 平台 | 现状 | 工程动作 |
|---|---|---|
| **macOS** | `say` 已落(走 `run_with_timeout` 60s, 输出 .m4a) | 保持;扩到**选音色**(`say -v ?` 列 zh_CN 音色: `Tingting`, `Sin-ji` 等),默认 `Tingting` |
| **Windows** | 无 → 报错 | 加 `synth_windows_sapi(text, out) -> Result<Value, String>`:依赖 **windows-sys**(已加) + `ISpVoice` COM;**前置检查** `HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Speech\Voices\Tokens` 有无语音,无则降级 Silent;**注意** SAPI 5.4 在 Win10/11 自带,OneCore 神经语音部分系统带 zh-CN `Microsoft Yaoyao` / `Microsoft Kangkang` |
| **Linux** | 无 → 报错 | 加 `synth_linux_espeak(text, out)`:`Command::new("espeak-ng")`(优先)/`espeak`/`festival` 三级 fallback;`espeak-ng` 在 apt 一行装,真没有走 Silent(不报硬错,生成静音 mp3 + 字幕保留) |
| **任何** | Silent 兜底 | 生成 1s 静音 mp3 + 字幕,`{tier: "silent"}` 上报;UI 红字"未配 TTS,生成无声视频" |

**工程动作**:Windows SAPI 走 `windows-sys` 已有依赖(0.59)+ `ISpVoice::Speak` 同步调用,新加 `synth_sapi` 函数 60-80 行;**留 1 条 cfg 编译分支 `#[cfg(target_os = "windows")]`,交叉编译通过**。

### B.5 403/限流静默跌级

| # | 痛点 | 工程动作 |
|---|---|---|
| B.5.1 | MiniMax 偶发 401/403(实测刷新 key 中) → 现状 `?e` 直接返回 Err,前端无声道 | 解析 HTTP 状态:401/403/429 → **自动降 L3**(macOS say / Windows SAPI / Silent),`Result` 里带 `tier_downgraded: "MiniMax->MacSay"`,前端按降级显示 |
| B.5.2 | 429 限流 | 加**单飞节流** `governor`(已用 crate 候选,本轮加依赖)= 2 req/s;超限 100ms 后重试,3 次失败 → 降级 |
| B.5.3 | 网络断(curl 7) | 现状报"启动失败";扩到探测 `e.kind()` = `TimedOut`/`ConnectionRefused` → 静默降 L3 |

### B.6 可观测性

| # | 痛点 | 工程动作 |
|---|---|---|
| B.6.1 | `synth()` Result 只有 `audio`(hex)/`size` 字段,**无耗时/分块数/降级原因** | Result 加 `{audio, duration_ms, chunks, tier, tier_downgraded, char_count, model, voice}`,落 `~/Polaris/data/forge_tts.log.jsonl` (一行一次) |
| B.6.2 | `/api/status` 报不出"最近 100 次 TTS 平均延迟" | `forge_tts::stats() -> {n, avg_ms, p50, p99, downgrades}` 内存环形缓冲 1000 条 |

---

## C. forge_fx 工业级化(workspace 外 crate,基于架构 v2 描述 + 既有 PRD 推断)

### C.1 `__fx.seek` 确定性时钟 → 真动画视频导出

| # | 痛点 | 工程动作 |
|---|---|---|
| C.1.1 | `__fx.seek(t)` 让动画跳到时间 t 即可重现——但**截图 export 模式**走 `__pending` 栅栏等异步资源(图片/字体)就绪后才允许 seek;若栅栏**永不 resolve**(某资源 404)则全片卡死 | 加 `__pending.diagnostics` 全局对象,前端 console + forge_telemetry 报每个待资源(id/state/waited_ms);**超时 5s 单资源 → 标记为 broken,fallback 占位**;PPTX 导出在 `seek()` 之前调 `await __pending.barrier(5000)` 而不是无超时等 |
| C.1.2 | `__fx.bus` 事件总线跨 iframe 通信时无序号, **断点续导**(用户暂停再继续) 事件可能乱序 | 事件 `{t_ms, seq, type, payload}` 加全局单调 seq;`seek(t)` 前 flush bus 把 `seq < last_flushed` 的丢 |
| C.1.3 | 帧序列导出与 chromiumoxide 截图并发竞态(架构 v2 §CDP 路帧分片并发) | 加 **producer-consumer**:`__fx.seek(0..total_ms)` 步长 33ms(30fps) → 每步 `requestAnimationFrame` 后 CDP 截一帧;消费侧用 bounded channel (cap 8) 截与写 mp4 流水线,单帧超时 2s 标记 `t={ms}:timeout` 但不 fail 整片 |

### C.2 18 CSS + 8 Canvas 星河系:可观测性 + 错误隔离

| # | 痛点 | 工程动作 |
|---|---|---|
| C.2.1 | 26 个动效独立模块,**一个抛错是否会污染全局**?架构 v2 未明 | 每个动效模块包成 `safe_run(name, fn)` 套 try/catch,失败 → `__fx.errors.push({name, msg, stack, t})`;**主时钟不受影响继续跑**——给前端的 26 个动效加"健康徽章" |
| C.2.2 | Canvas 星河系 8 个是手写 2D context,**没 dispose 句柄**会 leak GPU 资源(架构 v2 提"真动画视频",长视频 10 分钟会爆) | 每个 Canvas 动效实现 `init/draw(t)/dispose()`;`__fx.seek(t)` 自动调 `dispose()` 旧帧 texture 池;`/api/status` 报当前 Canvas 上下文数,>50 红条 |
| C.2.3 | CSS 动效 18 个在 export 模式被 `html.no-anim *` 误伤(见 [[deck-export-overlap-noanim-bug]]) | **该坑已修**(export 模式只揭示活动页内动画元素);**新增**：动效清单 `polaris_fx_manifest.json` 写明每个动效"是否支持 export 模式",export 时跳过不支持的,落 `build_warnings` |
| C.2.4 | 26 个动效的可观测性 | `__fx.telemetry` 暴露 `{active_fx_count, last_error, draw_ms_p50, draw_ms_p99, frames_done}`;**单测**=注入 1 个 fx 必崩脚本断言其余 25 个仍跑 |

### C.3 spring 物理确定性回放

| # | 痛点 | 工程动作 |
|---|---|---|
| C.3.1 | spring(stiffness/damping/mass) 是 ODE 求解, **浮点累加** 在 seek 同一 t 不同机器(arm/x86) 末位差 | spring 状态序列化 = `(t, x, v)`;**seek 0..T** 不重积 spring,而是按 seek 目标 t **直接解** 解析积分——求 `x(t) = x0·cos(ωt) + v0/ω·sin(ωt)`,临界阻尼/过阻击分别闭式解 |
| C.3.2 | 跨 frame 浏览器 vs 服务端 wasm 求 spring 末位差 | spring 实现只用 `f64` + `f64::cos/sin`(无 std 浮点差),并**加 1e-9 容差单测**(两端各跑 N 步,差 < 1e-9 通过) |

### C.4 fx 时间线 → 帧序列 → forge-codec 接口

| # | 痛点 | 工程动作 |
|---|---|---|
| C.4.1 | 架构 v2 提"fx 时间线 → 帧序列 → forge-codec(openh264)";forge-codec 还没建(P2-B 任务) | **本轮定义接口签名**(即使实现空):<br>`pub trait FxFrameSink { async fn write_frame(&mut self, f: FxFrame) -> Result<(), String>; }`<br>`struct FxFrame { t_ms: u64, width: u32, height: u32, rgba: Vec<u8>, keyframe: bool }`<br>forge-codec 实现即 `impl FxFrameSink for H264Sink`,但**本轮只让 capture 端发到 channel**;codec 在 P2-B 任务里建 |
| C.4.2 | 关键帧策略 | 每 60 帧(2s@30fps) 强插 keyframe;动效内有大色块突变(star burst 触发) 即时插 keyframe |
| C.4.3 | 错误恢复 | codec 端写失败 → 帧序列先**落盘** `/tmp/forge-fx-frames/*.rgba`,codec 故障可重跑;不依赖一次内存过完 |

---

## D. 三件共有的"工业级化"通用层(放 P0 之后做)

| # | 主题 | 工程动作 |
|---|---|---|
| D.1 | **统一 error model** | 自研三件 Result 字段不一(forge_pptx 无 ok 字段语义、forge_tts 有 tier、forge_fx 无 Result) → 引入 `enum ForgeError { Io{path,src}, Network{url,status}, Ooxml{part,line}, Tts{tier,reason}, Fx{name,frame_ms}, Codec{encoder,reason} }`;`thiserror` 派生(架构 v2 提"消灭 panic") |
| D.2 | **统一 telemetry** | `~/Polaris/data/forge_telemetry.jsonl` 落所有 forge_* 调用 `{module, fn, t_start, t_end, ok, err_kind, tier}`;`/api/status` 报最近 100 条计数 |
| D.3 | **熔断 + 降级表** | `forge::preflight` 已落 P0;**扩"动降级"**=`forge::run_with_strategy(name, retries=2, backoff_ms=[200,800], fallback_fn)`;三件共调用入口 |
| D.4 | **单元测试基线** | 三件目前测试覆盖:forge_pptx **未知**,forge_tts **未知**,forge_fx **未知**;目标 = **每件 ≥ 8 单测**(含 1 个端到端);CI 必跑 |
| D.5 | **特性开关** | `forge_pptx`/`forge_tts`/`forge_fx` 三件做成 cargo features,默认全开;Docker slim 镜像 `--no-default-features` 关闭,体积减 30-50MB(参考 [[forge-crossplatform-foundation]] slim 路线) |

---

## E. 与既有记忆/PRD 的对齐

| 本轮产出 | 引用 | 是否冲突 |
|---|---|---|
| A.5 字体子集内嵌 | [[forge-engine-prd]] 提"字体子集内嵌" | **延续**(ADR-011) |
| A.1.3 隐形文本框兼容 | 现状 alpha=0 | **增强**(双保险) |
| B.1 L1 edge-tts | [[forge-engine-prd]] 提"edge-tts 免费神经层(403 须静默跌级)" | **重新评估**——Rust 端口全 0 star,改 L1=MiniMax 失败重试链 |
| B.4 Windows SAPI / Linux espeak | 架构 v2 提"离线兜底" | **新增**(原只 macOS) |
| C.1.1 `__pending` 栅栏 | 架构 v2 提"`__pending` 栅栏" | **增强**(5s 超时 + broken 标记) |
| C.3 spring 确定性 | 架构 v2 提"确定性时钟" | **新增**(ODE 闭式解) |
| C.4 forge-codec 接口 | [[forge-engine-prd]] 提 forge-codec 跨平台 | **定义接口,实现交给 P2-B** |
| D.1 thiserror 统一 error | 架构 v2 提"消灭 panic" | **延续** |

---

## F. 落地优先级(供下轮 sprint 选)

| 优先级 | 项 | 估时 | 依赖 |
|---|---|---|---|
| **P0**(本周) | A.4.1 流式写降内存 | 0.5d | — |
| **P0** | A.3.1 自写 schema 校验 + A.3.2 集成 | 1d | — |
| **P0** | A.1.3 隐形文本双保险 + A.2 兼容矩阵报告 | 0.5d | — |
| **P0** | B.3.1 chunk 切分 + B.3.2 静音 + B.5 限流降级 | 1d | — |
| **P0** | B.4 Windows SAPI + Linux espeak 兜底 | 1d | windows-sys 已加 |
| **P0** | C.2.1 单 fx 错误隔离(单测) | 0.5d | — |
| **P1** | A.5 字体子集内嵌 (subsetter crate) | 1.5d | 选 crate |
| **P1** | B.1 L1 改 MiniMax 失败重试链 | 0.5d | — |
| **P1** | B.2 VTT/SRT 生成(VAD 句切) | 1d | jieba-rs + webrtc-vad |
| **P1** | C.4 forge-codec 接口定义 + capture 端发 channel | 1d | P2-B 同步 |
| **P2** | A.1.1 alt-text descr + B.6 统计 | 0.5d+0.5d | — |
| **P2** | C.1.1 __pending 5s 超时 + C.3 spring 闭式解 | 1d+1d | — |
| **P2** | D 通用层 (error/telemetry/feature flag) | 1d | — |

**总估时 P0+P1 = 9.5 工作日**(与架构 v2 的"P0 先行 = .history/mtime/revise/preflight" 不冲突,可并行)。

---

## G. 风险与未决

| 风险 | 影响 | 缓解 |
|---|---|---|
| subsetter crate 与 rust_xlsxwriter 一样,可能 API 不稳 | A.5 推迟 | 第一版直接读 TTF 调 `ttf-parser` 手写字形 cmap(代码 ~200 行),不行再换 crate |
| 隐形文本框在 Keynote 16 是否真可见 | A.1.3 验证 | 真机未测,需在 macOS 14 上开 Keynote 16 验证 |
| Windows SAPI 在 Win11 24H2 是否仍带 zh 神经语音 | B.4 降级路径 | 探测无 zh 语音时直接走 Silent,不卡 |
| jieba-rs 词库 5MB 进 TTS 模块 | B.2 体积 | 改 `lazy_static` + `OnceCell`,仅首次合成编译 |
| fx 26 个动效一晚全跑通单测 | C.2.4 | 优先核心 5 个(star burst / spring / 星河 / no-anim / 文本动效),其余插 TODO |

---

## H. 引用

- 既有代码: `src-tauri/src/forge_pptx.rs` `src-tauri/src/forge_tts.rs` `src-tauri/src/forge.rs`
- 既有记忆: `forge-engine-prd.md` `forge-crossplatform-foundation.md` `deck-export-overlap-noanim-bug.md` `forge-vs-externals-clarification.md`
- 外部调研(本轮):
  - `edge-tts` Python 官方 (github.com/microsoft/edge-tts) — 限流 403 行为
  - `Initsnow/edge-tts-rust` `MasterXD123/mini-edge-tts-rust` `yynag/edge-tts-rust` — Rust 端口**全 0 star / 2024-2026 个人维护**,生产慎用
  - `rust_xlsxwriter` (docs.rs) — **不写 .pptx**,只写 .xlsx
  - `pptx-rs` — 社区 crate,2024 后无大更新;**自写 OOXML 校验**优于依赖它
  - `windows-rs`/`windows-sys` (已加) — 暴露 `ISpVoice`,Win10/11 自带 SAPI 5.4,OneCore 神经语音部分系统自带 zh-CN(`Yaoyao`/`Kangkang`)

---

**Leads discovered / Gaps**
- **L1 edge-tts 缺稳态 Rust 实现**:三个 crate 全 0 star,改 MiniMax 重试链兜底
- **pptx-rs 维护停滞**:自写 OOXML 校验,零新依赖(zip + quick-xml 可选)
- **B 章节 linux espeak / windows SAPI 未实测**:真机未点测,坑未踩
- **C 章节 forge-codec 接口**:本轮只定义 trait 不实现,等 P2-B 同步
- **D 章节统一 error/telemetry 通用层**:本轮只列方案,代码 0 行,需另开任务
