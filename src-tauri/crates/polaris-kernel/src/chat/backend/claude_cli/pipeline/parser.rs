use super::*;

pub(crate) fn handle_stream_event(
    app: &AppHandle,
    req_id: &str,
    conv_id: Option<&str>,
    v: &Value,
    accum: &mut String,
    artifacts: &mut Vec<String>,
    ps: &mut PartialStreamState,
    batcher: &mut DeltaBatcher,
) {
    let t = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    match t {
        // --include-partial-messages 打开后 CLI 吐的 token 级增量:钻进 content_block_delta 的
        // text_delta 逐字上屏(豆包/ChatGPT 式)。**不进 accum** —— 完整文本稍后随整块 assistant
        // 事件到达并记账,这里只管显示;若整块事件缺席(进程中途崩),result 兜底仍按 accum 口径落库。
        // 纯 text_delta 走合批器(30ms 窗); 其它 stream_event 子类型(content_block_stop/
        // message_stop 等)先 flush, 保证顺序不变。
        "stream_event" => {
            let ev = v.get("event");
            let et = ev
                .and_then(|e| e.get("type"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let mut pushed = false;
            if et == "content_block_delta" {
                if let Some(d) = ev.and_then(|e| e.get("delta")) {
                    if d.get("type").and_then(|x| x.as_str()) == Some("text_delta") {
                        if let Some(txt) = d.get("text").and_then(|x| x.as_str()) {
                            if !txt.is_empty() {
                                ps.msg_streamed = true;
                                ps.ever_streamed = true;
                                batcher.push(app, req_id, conv_id, txt);
                                pushed = true;
                            }
                        }
                    }
                }
            }
            if !pushed {
                batcher.flush(app, req_id, conv_id);
            }
        }
        "assistant" => {
            // 整块事件(text/tool_use)到达: 先 flush 挂起的合批 delta, 保证事件顺序不变。
            batcher.flush(app, req_id, conv_id);
            if let Some(content) = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                for block in content {
                    let bt = block.get("type").and_then(|x| x.as_str()).unwrap_or("");
                    match bt {
                        "text" => {
                            if let Some(txt) = block.get("text").and_then(|x| x.as_str()) {
                                accum.push_str(txt);
                                // 非空整块文本已记账 → result 兜底不必再补(见 saw_assistant_text 注释)。
                                if !txt.is_empty() {
                                    ps.saw_assistant_text = true;
                                }
                                // 本消息的文本已经逐字流出过 → 整块事件只记账,不再重复上屏。
                                if !ps.msg_streamed {
                                    emit_event(
                                        app,
                                        ChatStreamEvent {
                                            call_id: None, parent_call_id: None,
                                            req_id: req_id.into(),
                                            kind: "delta".into(),
                                            text: Some(txt.to_string()),
                                            tool: None,
                                            conversation_id: conv_id.map(|s| s.to_string()),
                                        },
                                    );
                                }
                            }
                        }
                        "tool_use" => {
                            let name = block
                                .get("name")
                                .and_then(|x| x.as_str())
                                .unwrap_or("unknown");
                            // 输入摘要(命令/路径/检索词等一行) → 前端工具 pill 可展开看详情
                            let summary = block.get("input").and_then(tool_input_summary);
                            emit_event(
                                app,
                                ChatStreamEvent {
                                    call_id: None, parent_call_id: None,
                                    req_id: req_id.into(),
                                    kind: "tool".into(),
                                    text: summary,
                                    tool: Some(name.to_string()),
                                    conversation_id: conv_id.map(|s| s.to_string()),
                                },
                            );
                            // 写文件类工具 → 记一个成品文件 (实时反馈)
                            if matches!(name, "Write" | "Edit" | "MultiEdit" | "NotebookEdit") {
                                let fp = block
                                    .get("input")
                                    .and_then(|i| {
                                        i.get("file_path").or_else(|| i.get("notebook_path"))
                                    })
                                    .and_then(|x| x.as_str());
                                if let Some(fp) = fp {
                                    let norm = fp.replace('\\', "/");
                                    // 只展示常见成品格式; 应用文件夹内部文件不单独展示
                                    // (收尾快照统一归并成一个文件夹 chip), 防中间产物刷屏
                                    if is_displayable_artifact(&norm)
                                        && packaged_project_root(Path::new(&norm)).is_none()
                                        && !artifacts.contains(&norm)
                                    {
                                        artifacts.push(norm.clone());
                                        emit_event(
                                            app,
                                            ChatStreamEvent {
                                                call_id: None, parent_call_id: None,
                                                req_id: req_id.into(),
                                                kind: "artifact".into(),
                                                text: Some(norm),
                                                tool: None,
                                                conversation_id: conv_id.map(|s| s.to_string()),
                                            },
                                        );
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // 一条消息一个整块事件:处理完毕即复位,下一条消息的 delta 重新记账。
            ps.msg_streamed = false;
        }
        "result" => {
            // 收尾事件: 先 flush 挂起的合批 delta。
            batcher.flush(app, req_id, conv_id);
            // result 事件: claude --print 模式收尾, result 字段是最终文本
            if let Some(txt) = v.get("result").and_then(|x| x.as_str()) {
                // 若前面已经有整块 assistant text, result 通常是同一内容的最终版, 不重复记账。
                // 判据是 saw_assistant_text 而非「accum 非空」: image_notice 预置会让 accum
                // 永远非空, 把这条「整块事件缺席」的兜底彻底废掉(最终文本静默丢失)。
                if !ps.saw_assistant_text {
                    accum.push_str(txt);
                    // 曾逐字流出过 → 正文已在屏上(只是整块事件缺席没记上账),补账不补屏。
                    if !ps.ever_streamed {
                        emit_event(
                            app,
                            ChatStreamEvent {
                                call_id: None, parent_call_id: None,
                                req_id: req_id.into(),
                                kind: "delta".into(),
                                text: Some(txt.to_string()),
                                tool: None,
                                conversation_id: conv_id.map(|s| s.to_string()),
                            },
                        );
                    }
                }
            }
            // error subtype
            if let Some(subtype) = v.get("subtype").and_then(|x| x.as_str()) {
                if subtype.starts_with("error") {
                    let msg = v
                        .get("result")
                        .and_then(|x| x.as_str())
                        .unwrap_or("(unknown error)")
                        .to_string();
                    emit_event(
                        app,
                        ChatStreamEvent {
                            call_id: None, parent_call_id: None,
                            req_id: req_id.into(),
                            kind: "error".into(),
                            text: Some(format!("[result error: {}] {}", subtype, msg)),
                            tool: None,
                            conversation_id: conv_id.map(|s| s.to_string()),
                        },
                    );
                }
            }
        }
        // 其它事件类型(system/user 回显等): 也算「非 delta 事件」, flush 兜底防缓冲滞留。
        _ => {
            batcher.flush(app, req_id, conv_id);
        }
    }
}
fn tool_input_summary(input: &serde_json::Value) -> Option<String> {
    const KEYS: [&str; 10] = [
        "command",
        "file_path",
        "notebook_path",
        "pattern",
        "query",
        "url",
        "description",
        "prompt",
        "path",
        "skill",
    ];
    for k in KEYS {
        if let Some(s) = input.get(k).and_then(|x| x.as_str()) {
            let s = s.trim();
            if s.is_empty() {
                continue;
            }
            let one_line = s.lines().next().unwrap_or(s);
            let mut out: String = one_line.chars().take(120).collect();
            if one_line.chars().count() > 120 || s.lines().count() > 1 {
                out.push('…');
            }
            return Some(out);
        }
    }
    None
}

