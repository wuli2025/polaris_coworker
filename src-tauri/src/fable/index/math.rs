use super::*;

// ───────────────────────── chunker ─────────────────────────

/// 段落聚合式切块:按空行聚段到 ~1600 字符;超长段硬切(200 字符重叠)。
/// 全按 char 计数,杜绝多字节边界 panic。
pub(crate) fn chunk_text(s: &str) -> Vec<String> {
    const TARGET: usize = 1600;
    const OVERLAP: usize = 200;
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_chars = 0usize;
    let flush = |cur: &mut String, cur_chars: &mut usize, chunks: &mut Vec<String>| {
        let t = cur.trim();
        if t.chars().count() >= 24 {
            chunks.push(t.to_string());
        }
        cur.clear();
        *cur_chars = 0;
    };
    for para in s.split("\n\n") {
        let plen = para.chars().count();
        if plen > TARGET {
            flush(&mut cur, &mut cur_chars, &mut chunks);
            // 超长段:滑窗硬切
            let cs: Vec<char> = para.chars().collect();
            let mut start = 0usize;
            while start < cs.len() {
                let end = (start + TARGET).min(cs.len());
                chunks.push(cs[start..end].iter().collect::<String>().trim().to_string());
                if end == cs.len() {
                    break;
                }
                start = end.saturating_sub(OVERLAP);
            }
            continue;
        }
        if cur_chars + plen > TARGET {
            flush(&mut cur, &mut cur_chars, &mut chunks);
        }
        if !cur.is_empty() {
            cur.push_str("\n\n");
        }
        cur.push_str(para);
        cur_chars += plen + 2;
        if chunks.len() >= MAX_CHUNKS_PER_FILE {
            break;
        }
    }
    flush(&mut cur, &mut cur_chars, &mut chunks);
    chunks.retain(|c| !c.is_empty());
    chunks.truncate(MAX_CHUNKS_PER_FILE);
    chunks
}

pub(crate) fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

pub(crate) fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// 直接在 f32 小端字节上算点积(向量均已归一化 → 即余弦),省掉 [`blob_to_vec`] 的中间
/// `Vec<f32>` 堆分配 —— 检索精排每候选省一次分配(大库一次查询数百候选)。`blob` 字节数须
/// 为 `qv.len()*4`(维度/模型一致),否则 `None`(脏数据/旧维度向量,调用方跳过)。
pub(crate) fn dot_blob(qv: &[f32], blob: &[u8]) -> Option<f32> {
    if blob.len() != qv.len() * 4 {
        return None;
    }
    let mut s = 0f32;
    for (i, q) in qv.iter().enumerate() {
        let o = i * 4;
        s += q * f32::from_le_bytes([blob[o], blob[o + 1], blob[o + 2], blob[o + 3]]);
    }
    Some(s)
}

// ───────────────────────── 归一化 / 二值量化(P1-3 / P1-1)─────────────────────────

/// L2 归一化(就地)。入库前归一化一次 → 查询余弦退化成纯点积,省掉「每查询给每个向量现算模长」。
pub(crate) fn normalize(v: &mut [f32]) {
    let n = (v.iter().map(|x| x * x).sum::<f32>()).sqrt();
    if n > 1e-12 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

/// 符号位打包成二值码(dim 位 → ⌈dim/8⌉ 字节)。两段式 ANN 第一段用它算汉明距离做角度粗筛,
/// 读量只有 f32 的 1/32。归一化向量上,汉明距离与角度强相关 → 粗筛召回有保证。
pub(crate) fn bits_of(v: &[f32]) -> Vec<u8> {
    let mut out = vec![0u8; v.len().div_ceil(8)];
    for (i, &x) in v.iter().enumerate() {
        if x >= 0.0 {
            out[i / 8] |= 1 << (i % 8);
        }
    }
    out
}

/// 两个等长二值码的汉明距离(位不同的个数)。
pub(crate) fn hamming(a: &[u8], b: &[u8]) -> u32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x ^ y).count_ones()).sum()
}

/// 当前生效的嵌入模型标识(= provider.default_model)。用于 P2-2 版本隔离与查询缓存键。
pub fn active_embed_model() -> Option<String> {
    crate::sense::active_provider("embed").map(|p| p.default_model)
}

/// 是否**具备把文本变向量的能力** —— 本地开源嵌入(local-embed)**或**云 API 嵌入服务商。
/// `active_provider("embed")` 只认 `kind=api`+有 key 的云服务商,**不计本地档**;但本地档
/// (v1.4.2,bge-m3 ONNX)离线就能产向量。渐进式「智能归类」据此决定要不要跑「全量向量化 →
/// 按内容语义重聚」——只看云 key 会让纯本地用户永远停在结构归类、永远走不到「按意思」。
pub fn embed_capable() -> bool {
    #[cfg(feature = "local-embed")]
    if crate::fable::embed_local::enabled() {
        return true;
    }
    crate::sense::active_provider("embed").is_some()
}

// ───────────────────────── 查询嵌入缓存(P1-5)─────────────────────────

/// 极简 LRU:HashMap 存值 + VecDeque 记最近使用顺序。容量满时淘汰最久未用。
struct QueryCache {
    cap: usize,
    map: HashMap<String, Vec<f32>>,
    order: VecDeque<String>,
}
impl QueryCache {
    fn get(&mut self, k: &str) -> Option<Vec<f32>> {
        let v = self.map.get(k)?.clone();
        self.order.retain(|x| x != k);
        self.order.push_back(k.to_string());
        Some(v)
    }
    fn put(&mut self, k: String, v: Vec<f32>) {
        if self.map.insert(k.clone(), v).is_none() {
            self.order.push_back(k);
            while self.order.len() > self.cap {
                if let Some(old) = self.order.pop_front() {
                    self.map.remove(&old);
                }
            }
        } else {
            self.order.retain(|x| x != &k);
            self.order.push_back(k);
        }
    }
}
static QUERY_CACHE: Lazy<Mutex<QueryCache>> = Lazy::new(|| {
    Mutex::new(QueryCache { cap: 256, map: HashMap::new(), order: VecDeque::new() })
});

/// 查询嵌入(P1-5):LRU 缓存命中直接返回**归一化**向量(高并发下重复查询零接口开销);
/// 未命中才打一次嵌入接口。失败上抛 —— 调用方按可降级处理(向量腿静默退场,grep/FTS 腿照常)。
pub fn embed_query(query: &str) -> Result<Vec<f32>, String> {
    let model = active_embed_model().unwrap_or_default();
    let key = format!("{model}\u{0}{query}");
    if let Some(v) = QUERY_CACHE.lock().unwrap().get(&key) {
        return Ok(v);
    }
    let mut v = match embed_texts(&[query.to_string()]) {
        Ok(vs) => vs.into_iter().next().ok_or("查询嵌入为空")?,
        Err(e) => {
            // 云嵌入失败(断网/限速/服务挂)→ 若本地模型已下载就位且当前非本地档(本地档失败再退本地
            // 无意义),退回本地 BGE-M3 现算。同 1024 维空间,兼容云建的既有索引 → 查询韧性不靠云。
            #[cfg(feature = "local-embed")]
            {
                if !crate::fable::embed_local::enabled() && crate::fable::embed_local::ready() {
                    crate::fable::embed_local::embed(&[query.to_string()])?
                        .into_iter()
                        .next()
                        .ok_or("查询嵌入为空")?
                } else {
                    return Err(e);
                }
            }
            #[cfg(not(feature = "local-embed"))]
            {
                return Err(e);
            }
        }
    };
    normalize(&mut v);
    QUERY_CACHE.lock().unwrap().put(key, v.clone());
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_makes_unit_length() {
        let mut v = vec![3.0f32, 4.0];
        normalize(&mut v);
        let n = (v.iter().map(|x| x * x).sum::<f32>()).sqrt();
        assert!((n - 1.0).abs() < 1e-6);
        // 零向量不应除零崩溃,保持全零。
        let mut z = vec![0.0f32; 4];
        normalize(&mut z);
        assert!(z.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn bits_pack_and_hamming() {
        // 符号位:正/零 → 1,负 → 0。8 维正好 1 字节。
        let v = [1.0f32, -1.0, 2.0, -3.0, 0.0, -0.1, 5.0, -9.0];
        let b = bits_of(&v); // 位:1,0,1,0,1,0,1,0 → 0b01010101 = 0x55
        assert_eq!(b.len(), 1);
        assert_eq!(b[0], 0b0101_0101);
        // 自己跟自己汉明距离 0;翻转一位 → 距离 1。
        assert_eq!(hamming(&b, &b), 0);
        let mut b2 = b.clone();
        b2[0] ^= 0b0000_0001;
        assert_eq!(hamming(&b, &b2), 1);
        // 维度非 8 的整数倍:9 维 → 2 字节。
        assert_eq!(bits_of(&[0.0f32; 9]).len(), 2);
    }

    #[test]
    fn dot_blob_matches_blob_to_vec_path() {
        // dot_blob 必须与「blob_to_vec 后逐元素相乘求和」逐位一致(这是它替换的旧路径)。
        let qv = [0.1f32, -0.2, 0.3, 0.5, -0.7];
        let dv = [0.4f32, 0.4, -0.1, 0.2, 0.9];
        let blob = vec_to_blob(&dv);
        let want: f32 = blob_to_vec(&blob).iter().zip(qv.iter()).map(|(a, b)| a * b).sum();
        let got = dot_blob(&qv, &blob).expect("维度一致应返回 Some");
        assert!((got - want).abs() < 1e-6, "got={got} want={want}");
        // 维度不符(脏数据/旧维度向量)→ None,调用方跳过而非误算。
        assert!(dot_blob(&qv, &vec_to_blob(&[1.0f32, 2.0])).is_none());
        assert!(dot_blob(&qv, &blob[..blob.len() - 1]).is_none()); // 截断字节
    }

    #[test]
    fn query_cache_lru_evicts_oldest() {
        let mut c = QueryCache { cap: 2, map: HashMap::new(), order: VecDeque::new() };
        c.put("a".into(), vec![1.0]);
        c.put("b".into(), vec![2.0]);
        assert!(c.get("a").is_some()); // 访问 a → a 变最近
        c.put("c".into(), vec![3.0]); // 淘汰最久未用 = b
        assert!(c.get("b").is_none());
        assert!(c.get("a").is_some());
        assert!(c.get("c").is_some());
    }
}
