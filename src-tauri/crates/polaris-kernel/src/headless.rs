//! Read-only no-UI calls use the same selected backend as chat.
use std::{path::Path, time::Duration};
use crate::chat::backend::{self, TurnRequest};
pub fn run_agent_readonly<F: FnMut(&str, &str)>(root: &Path, prompt: &str, on_event: F) -> Result<String, String> {
    run(root, prompt, on_event, None)
}
pub fn run_agent_readonly_timeout<F: FnMut(&str, &str)>(root: &Path, prompt: &str, on_event: F, timeout: Duration) -> Result<String, String> {
    run(root, prompt, on_event, Some(timeout))
}
/// Compatibility for one migration release; use the backend-neutral entry point.
#[deprecated(note = "use run_agent_readonly")]
pub fn run_claude_readonly<F: FnMut(&str, &str)>(root: &Path, prompt: &str, on_event: F) -> Result<String, String> {
    run_agent_readonly(root, prompt, on_event)
}
#[deprecated(note = "use run_agent_readonly_timeout")]
pub fn run_claude_readonly_timeout<F: FnMut(&str, &str)>(
    root: &Path, prompt: &str, on_event: F, timeout: Duration,
) -> Result<String, String> {
    run_agent_readonly_timeout(root, prompt, on_event, timeout)
}

fn run<F: FnMut(&str, &str)>(root: &Path, prompt: &str, mut on_event: F, timeout: Option<Duration>) -> Result<String, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let req = format!("headless-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let selected = backend::selected().map_err(|e| e.to_string())?;
    let input = TurnRequest {
        req_id: req.clone(), session_id: req.clone(), conversation_id: None,
        prompt: prompt.into(), permission: crate::chat::PermissionMode::Deny,
        model: None, thinking_level: None, cwd: root.into(), attachments: vec![], account_id: None, timeout, legacy: None,
    };
    let legacy = selected.status().backend == "claude-cli";
    let result = backend::collect(selected.as_ref(), input, |kind, text| {
        on_event(kind, if legacy && kind == "delta" { text.trim() } else { text });
    }).map_err(|e| e.to_string());
    let _ = selected.close(&req);
    result
}

/// 从一段文本里抽出第一个**平衡**的 JSON (对象 `{...}` 或数组 `[...]`), 容忍前后包裹的
/// markdown 代码围栏与说明文字 (借鉴 llm_wiki 对 LLM 输出格式宽松解析)。
pub fn extract_balanced_json(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let start = s.find(['{', '['])?;
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            x if x == open => depth += 1,
            x if x == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_tolerates_fences_and_prose() {
        let s = "好的, 结果如下:\n```json\n[{\"a\":1},{\"b\":\"]x\"}]\n```\n完毕";
        let j = extract_balanced_json(s).unwrap();
        assert_eq!(j, "[{\"a\":1},{\"b\":\"]x\"}]");
        let obj = extract_balanced_json("noise {\"k\": \"v}v\"} tail").unwrap();
        assert_eq!(obj, "{\"k\": \"v}v\"}");
    }
}
