// 参数提取器（前端 invoke 走 camelCase 键）
fn req_str(a: &Args, k: &str) -> Result<String, String> {
    a.get(k)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("缺少字符串参数 `{k}`"))
}
fn opt_str(a: &Args, k: &str) -> Option<String> {
    a.get(k).and_then(|v| {
        if v.is_null() {
            None
        } else {
            v.as_str().map(|s| s.to_string())
        }
    })
}
fn opt_usize(a: &Args, k: &str) -> Option<usize> {
    a.get(k).and_then(|v| v.as_u64()).map(|n| n as usize)
}
// 下列取参 helper 只被 dispatch_sync 用,与它同门控,否则 desktop 编译时报 dead_code。
#[cfg(not(feature = "desktop"))]
fn opt_bool(a: &Args, k: &str) -> Option<bool> {
    a.get(k).and_then(|v| v.as_bool())
}
#[cfg(not(feature = "desktop"))]
fn opt_f64(a: &Args, k: &str) -> Option<f64> {
    a.get(k).and_then(|v| v.as_f64())
}
#[cfg(not(feature = "desktop"))]
fn opt_u8(a: &Args, k: &str) -> Option<u8> {
    a.get(k).and_then(|v| v.as_u64()).map(|n| n.min(255) as u8)
}
#[cfg(not(feature = "desktop"))]
fn bool_def(a: &Args, k: &str, d: bool) -> bool {
    a.get(k).and_then(|v| v.as_bool()).unwrap_or(d)
}
/// 可选的对象数组(共享清单 `[{path, write}]` 这类)。缺失/非数组 → 空 vec。
fn vec_val(a: &Args, k: &str) -> Vec<serde_json::Value> {
    a.get(k)
        .and_then(|v| v.as_array())
        .map(|arr| arr.to_vec())
        .unwrap_or_default()
}

fn vec_str(a: &Args, k: &str) -> Vec<String> {
    a.get(k)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}
/// 必填字符串数组:缺失/非数组/元素非字符串都报 400,避免参数错被伪装成「空结果」
#[cfg(not(feature = "desktop"))]
fn req_vec_str(a: &Args, k: &str) -> Result<Vec<String>, String> {
    let arr = a
        .get(k)
        .ok_or_else(|| format!("缺少数组参数 `{k}`"))?
        .as_array()
        .ok_or_else(|| format!("参数 `{k}` 无效:必须是字符串数组"))?;
    arr.iter()
        .map(|x| {
            x.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| format!("参数 `{k}` 无效:数组元素必须是字符串"))
        })
        .collect()
}

// ───────────────────── 容器自更新（Docker 壳）─────────────────────
