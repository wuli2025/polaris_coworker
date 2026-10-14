use super::*;

// ───────────────────────── Internals ─────────────────────

/// token 级部分流(`--include-partial-messages`)开关,默认开。
/// `POLARIS_PARTIAL_STREAM=0` 关闭 —— 兼容不认识该 flag 的旧版 claude CLI(老 CLI 见到未知
/// 参数会直接拒跑);关掉后回到「整块 assistant 事件」粒度,除了不逐字外行为完全一致。
pub(crate) fn partial_stream_enabled() -> bool {
    std::env::var("POLARIS_PARTIAL_STREAM")
        .map(|v| v.trim() != "0")
        .unwrap_or(true)
}

/// 部分流状态:token 级 `stream_event` delta 与随后整块 `assistant` 事件之间的去重记账。
/// 落库口径始终是整块 assistant 事件(权威、含完整 content);delta 只负责「屏上逐字长出来」。
#[derive(Default)]
pub(crate) struct PartialStreamState {
    /// 当前消息已用 token delta 流出过文本 → 它的整块 assistant 事件只记账(进 accum)不再重复显示。
    pub(crate) msg_streamed: bool,
    /// 本请求曾流出过任何 token delta → result 兜底不再重复显示(正文已经在屏上)。
    pub(crate) ever_streamed: bool,
    /// 本请求曾收到过**整块 assistant 事件**的非空文本(权威记账口径)→ result 兜底不再补账。
    /// 不能复用「accum 非空」判断: image_notice 会把中文说明预置进 accum, 使它永远非空,
    /// 整块事件真缺席(进程中途崩)时 result 兜底被废、最终文本丢失 —— 预置 notice 不置本位。
    pub(crate) saw_assistant_text: bool,
}

/// delta 合批器: 纯 text_delta 文本先攒进缓冲, 距上次 emit ≥30ms 才发一条合并 delta
/// (payload 结构不变, 只是多条 delta 的 text 拼成一条); 任何非 delta 事件到达前、以及
/// 流结束时由调用方 flush —— 事件相对顺序与逐条 emit 完全一致, 只是 IPC 频率从
/// 「每 token 一次」降到「每 30ms 一次」。
pub(crate) struct DeltaBatcher {
    pub(crate) buf: String,
    pub(crate) last_emit: std::time::Instant,
}

/// 合批时间窗(毫秒): 距上次 emit 不足此值的 text_delta 先攒着。
const DELTA_BATCH_WINDOW_MS: u64 = 30;

impl DeltaBatcher {
    pub(crate) fn new() -> Self {
        Self {
            buf: String::new(),
            last_emit: std::time::Instant::now(),
        }
    }
    /// 累积一段 text_delta; 距上次 emit ≥ 时间窗即 flush。
    pub(crate) fn push(&mut self, app: &AppHandle, req_id: &str, conv_id: Option<&str>, txt: &str) {
        self.buf.push_str(txt);
        if self.last_emit.elapsed() >= std::time::Duration::from_millis(DELTA_BATCH_WINDOW_MS) {
            self.flush(app, req_id, conv_id);
        }
    }
    /// 把缓冲里的文本合成**一条** delta 事件发出(缓冲为空则什么都不发)。
    pub(crate) fn flush(&mut self, app: &AppHandle, req_id: &str, conv_id: Option<&str>) {
        if self.buf.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.buf);
        emit_event(
            app,
            ChatStreamEvent {
                call_id: None, parent_call_id: None,
                req_id: req_id.into(),
                kind: "delta".into(),
                text: Some(text),
                tool: None,
                conversation_id: conv_id.map(|s| s.to_string()),
            },
        );
        self.last_emit = std::time::Instant::now();
    }
}

/// 单轮 claude 事件流状态机: 逐行喂 stream-json → emit 前端事件 + 记账落库缓冲 +
/// 产物追踪, 最后统一收尾。旧「每消息 spawn」路径与常驻 agent 路径(session_pool)
/// 共用这一份实现 —— 事件契约(meta→delta/tool/artifact/error→done)只此一处, 不会分叉。
/// (逻辑从旧 stdout reader 闭包原样抽出, 行为零变化。)
pub(crate) struct TurnStream {
    pub(crate) app: AppHandle,
    pub(crate) req_id: String,
    pub(crate) conv_id: Option<String>,
    pub(crate) assistant_text: String,
    /// 落库缓冲封顶后改写 scrap(可丢弃): 实时 delta 仍照常 emit, 内存不无界增长。
    pub(crate) scrap: String,
    pub(crate) capped: bool,
    pub(crate) artifacts: Vec<String>,
    pub(crate) partial: PartialStreamState,
    pub(crate) batcher: DeltaBatcher,
}

impl TurnStream {
    pub(crate) fn new(
        app: AppHandle,
        req_id: String,
        conv_id: Option<String>,
        image_notice: Option<String>,
    ) -> Self {
        let mut ts = Self {
            app,
            req_id,
            conv_id,
            assistant_text: String::new(),
            scrap: String::new(),
            capped: false,
            artifacts: Vec::new(),
            partial: PartialStreamState::default(),
            batcher: DeltaBatcher::new(),
        };
        // 生图不支持时: 后端确定性地把中文说明作为**第一段**发出去并计入正文,
        // 不依赖模型遵守「开头摊牌」指令 → 用户一定先看到「当前模型不支持生图」。
        if let Some(notice) = image_notice {
            ts.assistant_text.push_str(&notice);
            emit_event(
                &ts.app,
                ChatStreamEvent {
                    call_id: None, parent_call_id: None,
                    req_id: ts.req_id.clone(),
                    kind: "delta".into(),
                    text: Some(notice),
                    tool: None,
                    conversation_id: ts.conv_id.clone(),
                },
            );
        }
        ts
    }

    pub(crate) fn record_backend_event(&mut self, event: &ChatStreamEvent) {
        if event.kind == "delta" && !self.capped {
            self.assistant_text.push_str(event.text.as_deref().unwrap_or_default());
            if self.assistant_text.len() > MAX_ASSISTANT_BYTES {
                self.assistant_text.push_str("\n\n[⚠️ 输出过长，后续内容已省略]");
                self.capped = true;
            }
        }
        if event.kind == "artifact" {
            if let Some(path) = &event.text {
                if !self.artifacts.contains(path) { self.artifacts.push(path.clone()); }
            }
        }
    }

    /// 喂一行 stdout。返回「这行是不是 result 收尾事件」—— 常驻路径以它为本轮界标
    /// (每轮末 CLI 必吐一条 {"type":"result",...}); 一次性进程路径忽略返回值, 以 EOF 收尾。
    pub(crate) fn feed_line(&mut self, line: &str) -> bool {
        let mut is_result = false;
        let target = if self.capped {
            &mut self.scrap
        } else {
            &mut self.assistant_text
        };
        match serde_json::from_str::<Value>(line) {
            Ok(v) => {
                is_result = v.get("type").and_then(|x| x.as_str()) == Some("result");
                handle_stream_event(
                    &self.app,
                    &self.req_id,
                    self.conv_id.as_deref(),
                    &v,
                    target,
                    &mut self.artifacts,
                    &mut self.partial,
                    &mut self.batcher,
                );
            }
            Err(_) => {
                // 非 JSON 行: 当作 delta 直接显示 (调试友好)。先 flush 挂起的合批
                // delta, 保证屏上文本顺序与到达顺序一致。
                self.batcher
                    .flush(&self.app, &self.req_id, self.conv_id.as_deref());
                // 正文已进 accum 且已上屏 → 置记账位, 否则后续 result 携带同一内容时
                // 会因 saw_assistant_text=false 再补一遍, 屏上/落库都成双份。
                if !line.trim().is_empty() {
                    self.partial.saw_assistant_text = true;
                }
                target.push_str(line);
                target.push('\n');
                emit_event(
                    &self.app,
                    ChatStreamEvent {
                        call_id: None, parent_call_id: None,
                        req_id: self.req_id.clone(),
                        kind: "delta".into(),
                        text: Some(line.to_string()),
                        tool: None,
                        conversation_id: self.conv_id.clone(),
                    },
                );
            }
        }
        if self.capped {
            self.scrap.clear(); // scrap 只为让上面 emit 继续工作, 不能自己变成无界
        } else if self.assistant_text.len() > MAX_ASSISTANT_BYTES {
            self.assistant_text
                .push_str("\n\n[⚠️ 输出过长，后续内容已省略]");
            self.capped = true;
        }
        is_result
    }

    /// 把挂起的合批 delta 冲出去(流结束/本轮收尾时必调, 保证正文先于终态事件落地)。
    pub(crate) fn flush(&mut self) {
        self.batcher
            .flush(&self.app, &self.req_id, self.conv_id.as_deref());
    }

}
