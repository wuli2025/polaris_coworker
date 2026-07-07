# -*- coding: utf-8 -*-
# AI 辅助检索精度实测(全本地,零 API 余额依赖):
#   词法(FTS5) ∥ 向量(库内 dense 余弦) → RRF 融合 → **BGE-M3 ColBERT MaxSim 本地重排**。
# 查询嵌入(dense 用于向量腿、colbert 用于重排)全部用磁盘上的 bge-m3-int8 ONNX 本地算,不打云。
# 目标:量出「混检 + AI 重排(+ 候选池放宽)」能把 MRR / nDCG 推到多少,定位到 0.86 的路径。
import os, sqlite3, time, random, re, json, sys
import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

DB = os.path.join(os.environ["USERPROFILE"], "Polaris", "data", "fable.db")
MD = os.path.join(os.environ["USERPROFILE"], "Polaris", "models", "fastembed", "bge-m3-int8")
random.seed(42)
K = 10
N_QUERIES = int(os.environ.get("EVAL_N", "170"))
RERANK_N = int(os.environ.get("EVAL_RN", "40"))   # 融合后取前 N 送重排
LEX_K = int(os.environ.get("EVAL_LEXK", "60"))
VEC_K = int(os.environ.get("EVAL_VECK", "60"))
RRF_K = float(os.environ.get("EVAL_RRFK", "10"))
W_VEC = float(os.environ.get("EVAL_WVEC", "0.85"))

CJK = re.compile(r'[㐀-䶿一-鿿]{1,}')
CJK4 = re.compile(r'[㐀-䶿一-鿿]{4,}')
WORD = re.compile(r'[A-Za-z][A-Za-z0-9_]{3,}')

# ───────── ONNX 本地嵌入(dense + colbert)─────────
_so = ort.SessionOptions(); _so.intra_op_num_threads = int(os.environ.get("EVAL_THREADS", "12"))
SESS = ort.InferenceSession(os.path.join(MD, "model_quantized.onnx"), _so, providers=["CPUExecutionProvider"])
TOK = Tokenizer.from_file(os.path.join(MD, "tokenizer.json"))
TOK.enable_padding(length=None)
RR_MAXLEN = int(os.environ.get("EVAL_RR_MAXLEN", "256"))  # 重排 colbert 截断(256 token 足够给排序信号、比 512 快 ~2x)
_cur_maxlen = [0]
def _set_maxlen(ml):
    if _cur_maxlen[0] != ml:
        TOK.enable_truncation(max_length=ml); _cur_maxlen[0] = ml

def encode(texts, want_colbert=False, batch=24, maxlen=256):
    _set_maxlen(maxlen)
    """返回 (dense[N,1024] 归一化, colbert_list[N] each [Li,1024] 归一化掩码后)。"""
    dense_out = []
    colb_out = []
    outs_wanted = ["dense_vecs"] + (["colbert_vecs"] if want_colbert else [])
    for i in range(0, len(texts), batch):
        chunk = [t if t.strip() else " " for t in texts[i:i+batch]]
        enc = TOK.encode_batch(chunk)
        ids = np.array([e.ids for e in enc], dtype=np.int64)
        mask = np.array([e.attention_mask for e in enc], dtype=np.int64)
        res = SESS.run(outs_wanted, {"input_ids": ids, "attention_mask": mask})
        d = res[0]
        d = d / (np.linalg.norm(d, axis=1, keepdims=True) + 1e-9)
        dense_out.append(d.astype(np.float32))
        if want_colbert:
            cb = res[1]  # [b, token, 1024];该 ONNX 已内部去掉 CLS → 长度 = 输入 - 1
            cb_len = cb.shape[1]
            for r in range(cb.shape[0]):
                m = mask[r].astype(bool)
                if cb_len == m.shape[0] - 1:
                    m = m[1:]            # 模型已删 pos0(CLS),用 mask[1:] 对齐内容 token
                elif cb_len < m.shape[0]:
                    m = m[:cb_len]
                vecs = cb[r][:m.shape[0]][m]
                vecs = vecs / (np.linalg.norm(vecs, axis=1, keepdims=True) + 1e-9)
                colb_out.append(vecs.astype(np.float32))
    dense = np.vstack(dense_out) if dense_out else np.zeros((0,1024),np.float32)
    return dense, colb_out

RR_TAU = float(os.environ.get("EVAL_RR_TAU", "0.5"))  # 阈值化 MaxSim:弱匹配(filler)不计,治长文档偏置
def colbert_maxsim(q_colb, d_colb):
    """阈值化 ColBERT 晚交互:Σ_i [max_j(q_i·d_j) if ≥τ]。向量已归一化。
    长候选会让每个 query token 都凑到一个偏高的 max → 长度偏置;阈值把「真 token 命中(~1.0)」
    与「filler 凑数(<0.5)」分开,只累计强匹配,消除长度偏置。"""
    if d_colb.shape[0] == 0 or q_colb.shape[0] == 0:
        return -1e9
    sim = q_colb @ d_colb.T          # [Lq, Ld]
    mx = sim.max(axis=1)
    return float(mx[mx >= RR_TAU].sum())

# ───────── 载入库内 dense 向量 + chunk→file 映射 ─────────
print("[1] loading stored dense vectors ...", flush=True)
t0 = time.time()
c = sqlite3.connect(DB); c.execute("PRAGMA temp_store=MEMORY")
rows = c.execute("""
  SELECT ch.vec, ch.file_id, f.relpath, ch.text
  FROM chunks ch JOIN files f ON f.id=ch.file_id
  WHERE ch.model='BAAI/bge-m3' AND ch.vec IS NOT NULL
""").fetchall()
n = len(rows); dim = len(rows[0][0])//4
M = np.empty((n, dim), dtype=np.float32)
chunk_file = np.empty(n, dtype=np.int64)
fid_to_path = {}
chunk_text = [None]*n
for i,(b,fid,rel,txt) in enumerate(rows):
    M[i] = np.frombuffer(b, dtype=np.float32, count=dim)
    chunk_file[i] = fid; fid_to_path[fid] = rel; chunk_text[i] = txt or ""
nrm = np.linalg.norm(M,axis=1,keepdims=True); nrm[nrm==0]=1; Mn = M/nrm
# fid -> 该文件的全部 chunk 全局下标(重排时按查询挑「真正匹配的那个 chunk」,与 retrieve.rs 一致)
fid_chunks = {}
for i in range(n):
    fid = int(chunk_file[i]); fid_chunks.setdefault(fid, []).append(i)

def q_terms(query):
    """切词:拉丁≥2 + CJK 重叠二元组(供 chunk 选择按词重合打分)。"""
    ts=[]
    for w in WORD.findall(query):
        ts.append(w.lower())
    for seg in CJK.findall(query):
        ch=list(seg)
        if len(ch)==1: ts.append(seg)
        else:
            for w in range(len(ch)-1): ts.append("".join(ch[w:w+2]))
    return list(dict.fromkeys(ts))

def best_chunk_text(fid, q_full, terms):
    """在该文件的 chunk 里挑与查询最匹配的一个(整句子串命中最强,否则词重合最多,兜底最长)。"""
    idxs = fid_chunks.get(fid, [])
    if not idxs: return ""
    best=None; best_s=-1
    for gi in idxs:
        t = chunk_text[gi]; tl=t.lower()
        s = (5 if q_full and q_full in tl else 0) + sum(1 for x in terms if x in tl)
        if s > best_s or (s==best_s and best is not None and len(t)>len(chunk_text[best])):
            best_s=s; best=gi
    return chunk_text[best] if best is not None else ""
print(f"    {n} chunks / {len(fid_to_path)} files / dim={dim} / {time.time()-t0:.1f}s", flush=True)

# ───────── 构造考卷(复刻 eval_accuracy.py)─────────
def make_queries(txt):
    txt = re.sub(r'\s+',' ', txt).strip(); out=[]
    cjk = CJK4.findall(txt); words = WORD.findall(txt)
    if cjk:
        seg = max(cjk, key=len)
        if len(seg) > 16:
            st = random.randint(0, len(seg)-12); seg = seg[st:st+random.randint(8,12)]
        out.append(("snippet_cjk", seg))
    elif len(txt) > 20:
        st = random.randint(0, max(1,len(txt)-16)); out.append(("snippet", txt[st:st+14].strip()))
    pool = list(dict.fromkeys([w for w in cjk if 4<=len(w)<=8][:4] + words[:3]))
    if len(pool) >= 2:
        out.append(("keywords", " ".join(pool[:4])))
    return out

print("[2] building eval set ...", flush=True)
cand_fids = list(fid_to_path.keys()); random.shuffle(cand_fids)
file_text = {}
for i in range(n):
    fid = int(chunk_file[i])
    if fid not in file_text and len(chunk_text[i]) >= 30:
        file_text[fid] = chunk_text[i]
eval_cases = []
for fid in cand_fids:
    if fid not in file_text: continue
    for qk, q in make_queries(file_text[fid]):
        if len(q.strip()) >= 4: eval_cases.append((qk, q.strip(), fid))
    if len({e[2] for e in eval_cases}) >= N_QUERIES: break
print(f"    {len(eval_cases)} queries / {len({e[2] for e in eval_cases})} target files", flush=True)

# ───────── 词法腿(FTS5,复刻 retrieve.rs)─────────
def lex_search(query, k=LEX_K):
    terms = CJK.findall(query) + WORD.findall(query)
    trig = []
    for t in terms:
        if re.match(r'[A-Za-z]', t):
            if len(t) >= 3: trig.append(t)
        else:
            ch = list(t)
            if len(ch) >= 3:
                for w in range(len(ch)-2): trig.append("".join(ch[w:w+3]))
            # 2 字 CJK 单独留给 LIKE 兜底
    trig = list(dict.fromkeys(trig))[:60]
    files=[]
    if trig:
        expr = " OR ".join(f'"{t}"' for t in trig)
        try:
            for (rel,) in c.execute(
                "SELECT f.relpath FROM lex l JOIN files f ON f.id=l.rowid "
                "WHERE l.body MATCH ? ORDER BY bm25(lex) LIMIT ?", (expr,k)):
                files.append(rel)
        except Exception: pass
    if not files:
        longest = max(terms+[query], key=len)
        try:
            for (rel,) in c.execute(
                "SELECT f.relpath FROM lex l JOIN files f ON f.id=l.rowid "
                "WHERE l.body LIKE ? LIMIT ?", (f"%{longest}%",k)):
                files.append(rel)
        except Exception: pass
    return files

def vec_search_dense(qd, k=VEC_K):
    sims = Mn @ qd
    topn = min(600, n-1)
    idx = np.argpartition(-sims, topn)[:topn]
    idx = idx[np.argsort(-sims[idx])]
    seen=set(); files=[]
    for j in idx:
        fid=int(chunk_file[j])
        if fid in seen: continue
        seen.add(fid); files.append(fid_to_path[fid])
        if len(files)>=k: break
    return files

def rrf(lists, k=RRF_K, w=None, topn=RERANK_N):
    score={}
    for li,lst in enumerate(lists):
        wi = (w[li] if w else 1.0)
        for rank,p in enumerate(lst):
            score[p]=score.get(p,0)+wi/(k+rank+1)
    return [p for p,_ in sorted(score.items(), key=lambda x:-x[1])][:topn]

# ───────── 本地 embed:所有查询 dense + colbert ─────────
print("[3] local-embedding queries (dense+colbert) ...", flush=True)
t0=time.time()
qtexts=[q for _,q,_ in eval_cases]
Qd, Qcolb = encode(qtexts, want_colbert=True, batch=32, maxlen=96)
print(f"    embedded {len(qtexts)} queries in {time.time()-t0:.1f}s", flush=True)

# path -> fid(重排要按候选文件取文本)
path_to_fid = {v:k for k,v in fid_to_path.items()}

def metrics(ranks):
    nn=len(ranks)
    if nn==0: return (0,0,0,0)
    hit=sum(1 for r in ranks if r)
    mrr=sum(1.0/r for r in ranks if r)/nn
    ndcg=sum(1.0/np.log2(r+1) for r in ranks if r)/nn  # 单相关文档 nDCG@K
    return (hit/nn, mrr, ndcg, nn)

def first_rank(files, expect):
    for r,p in enumerate(files):
        if p==expect: return r+1
    return None

print("[4] running lanes + RRF + ColBERT rerank (tau sweep) ...", flush=True)
TAUS = [0.0, 0.4, 0.5, 0.6, 0.7]
def maxsim_tau(q_colb, d_colb, tau):
    if d_colb.shape[0]==0 or q_colb.shape[0]==0: return -1e9
    mx = (q_colb @ d_colb.T).max(axis=1)
    return float(mx[mx>=tau].sum())
agg={"lexical":[], "vector":[], "hybrid":[], "rr_dense":[]}
for tv in TAUS: agg[f"rr_tau{tv}"]=[]
pool_recall40=[]
rr_time=0.0
for qi,(qk,q,fid) in enumerate(eval_cases):
    expect = fid_to_path[fid]
    lex = lex_search(q, LEX_K)
    vec = vec_search_dense(Qd[qi], VEC_K)
    fused = rrf([vec, lex], k=RRF_K, w=[W_VEC, 1.0], topn=RERANK_N)
    agg["lexical"].append(first_rank(lex[:K], expect))
    agg["vector"].append(first_rank(vec[:K], expect))
    agg["hybrid"].append(first_rank(fused[:K], expect))
    pool_recall40.append(1 if expect in fused else 0)
    cand_paths = fused[:RERANK_N]
    ql = q.lower(); qts = q_terms(q)
    docs=[]; cand_dense=[]
    for p in cand_paths:
        f2 = path_to_fid.get(p)
        if f2 is not None:
            gi_list = fid_chunks.get(f2, [])
            # 选匹配 chunk(子串/词重合),并取其库内 dense 做 dense-rerank 对照
            best=None; best_s=-1
            for gi in gi_list:
                tl=chunk_text[gi].lower()
                s=(5 if ql and ql in tl else 0)+sum(1 for x in qts if x in tl)
                if s>best_s: best_s=s; best=gi
            docs.append((chunk_text[best] if best is not None else p)[:900])
            cand_dense.append(Mn[best] if best is not None else None)
        else:
            docs.append(p); cand_dense.append(None)
    t=time.time()
    _, Dcolb = encode(docs, want_colbert=True, batch=32, maxlen=RR_MAXLEN)
    rr_time += time.time()-t
    for tv in TAUS:
        sc=[maxsim_tau(Qcolb[qi], dc, tv) for dc in Dcolb]
        order=np.argsort(-np.asarray(sc))
        agg[f"rr_tau{tv}"].append(first_rank([cand_paths[j] for j in order][:K], expect))
    # dense-rerank 对照:用库内 dense 余弦对候选重排
    dsc=[float(Qd[qi]@cd) if cd is not None else -1e9 for cd in cand_dense]
    dorder=np.argsort(-np.asarray(dsc))
    agg["rr_dense"].append(first_rank([cand_paths[j] for j in dorder][:K], expect))
    if (qi+1)%40==0: print(f"    {qi+1}/{len(eval_cases)} (rerank {rr_time:.1f}s)", flush=True)

print("\n================ AI-ASSISTED RETRIEVAL ACCURACY ================")
print(f"corpus: {n} chunks / {len(fid_to_path)} files | N={len(eval_cases)} | RN={RERANK_N} LEXK={LEX_K} VECK={VEC_K} RRFK={RRF_K} WVEC={W_VEC}")
print(f"{'lane':16} {'recall@%d'%K:10} {'MRR':8} {'nDCG@%d'%K:8} {'n':>5}")
res={}
lanes_show=["lexical","vector","hybrid","rr_dense"]+[f"rr_tau{tv}" for tv in TAUS]
for lane in lanes_show:
    rc,mrr,ndcg,nn = metrics(agg[lane]); res[lane]={"recall":rc,"mrr":mrr,"ndcg":ndcg,"n":nn}
    print(f"{lane:16} {rc:8.3f}   {mrr:6.3f}  {ndcg:6.3f}  {nn:5}")
print(f"\nfused-pool recall@{RERANK_N} (rerank ceiling): {np.mean(pool_recall40):.3f}")
print(f"rerank wall: {rr_time:.1f}s total / {rr_time/len(eval_cases)*1000:.0f}ms per query")

out={"k":K,"n":len(eval_cases),"corpus":{"chunks":n,"files":len(fid_to_path)},
     "config":{"RN":RERANK_N,"LEXK":LEX_K,"VECK":VEC_K,"RRFK":RRF_K,"WVEC":W_VEC},
     "results":res,"pool_recall":float(np.mean(pool_recall40)),
     "rerank_ms_per_q":round(rr_time/len(eval_cases)*1000,0)}
open(os.path.join(os.path.dirname(__file__),"eval_ai_rerank.json"),"w",encoding="utf-8").write(json.dumps(out,ensure_ascii=False,indent=2))
print("\nwritten eval_ai_rerank.json")
