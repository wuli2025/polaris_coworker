//! Shared device-label character policy; callers retain their length/trim/default rules.
/// Reject control characters and HTML/attribute delimiters before storing a device label.
pub fn validate_characters(value: &str) -> Result<(), String> {
    if value.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | '`')) {
        return Err("设备名称不能包含控制字符或 < > \" ' `".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
