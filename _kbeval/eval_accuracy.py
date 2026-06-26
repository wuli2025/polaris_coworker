# -*- coding: utf-8 -*-
# 真机知识库准确率实测:从真实语料(fable.db)采样已索引文件 → 构造查询 →
# 向量车道 / 词法车道(FTS5) / 混合(RRF) 三路检索 → 算 recall@10 + MRR。
# 忠实复刻 retrieve.rs 的车道语义。仅在「已索引语料」上评测(覆盖缺口另算)。
import os, sqlite3, time, random, re, json
import numpy as np, requests

DB = os.path.join(os.environ["USERPROFILE"], "Polaris", "data", "fable.db")
KEY = "sk-ulmygegatdtjdcyxzuscfcdzcogwoirijrcnjbtzsylphzgw"
EMB_URL = "https://api.siliconflow.cn/v1/embeddings"
H = {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"}
random.seed(42)
K = 10                 # recall@K / MRR 截断
RRF_K = 60             # 与 retrieve.rs 一致
N_QUERIES = 120

c = sqlite3.connect(DB)
c.execute("PRAGMA temp_store=MEMORY")

# ── 1. 载入向量矩阵(model=bge-m3 的已嵌入 chunk),并保留 chunk→file 映射 ──
print("[1] loading vectors + chunk→file map ...")
t0 = time.time()
rows = c.execute("""
  SELECT ch.vec, ch.file_id, f.relpath, ch.text
  FROM chunks ch JOIN files f ON f.id=ch.file_id
  WHERE ch.model='BAAI/bge-m3' AND ch.vec IS NOT NULL
""").fetchall()
n = len(rows)
dim = len(rows[0][0])//4
M = np.empty((n, dim), dtype=np.float32)
chunk_file = np.empty(n, dtype=np.int64)
fid_to_path = {}
chunk_text = [None]*n
for i,(b,fid,rel,txt) in enumerate(rows):
    M[i] = np.frombuffer(b, dtype=np.float32, count=dim)
    chunk_file[i] = fid
    fid_to_path[fid] = rel
    chunk_text[i] = txt or ""
nrm = np.linalg.norm(M,axis=1,keepdims=True); nrm[nrm==0]=1; Mn = M/nrm
print(f"    {n} chunks, {len(fid_to_path)} files, dim={dim}, {time.time()-t0:.1f}s, mat={M.nbytes/1e6:.0f}MB")

# ── 2. 构造考卷:采样已索引文件,从其 chunk 文本里抽「真实可问」的查询 ──
# 两类查询(分别量不同能力):
#  A) snippet:文中一段连续中文/词(测「索引完整性」——内容在不在索引里、找不找得回)
#  B) keywords:抽 2-4 个显著词(测「关键词检索」更贴近真人输入)
CJK = re.compile(r'[㐀-䶿一-鿿]{4,}')
WORD = re.compile(r'[A-Za-z][A-Za-z0-9_]{3,}')
def make_queries(txt):
    txt = re.sub(r'\s+',' ', txt).strip()
    out = []
    cjk = CJK.findall(txt)
    words = WORD.findall(txt)
    # A snippet: 取一段 8-16 字的中文片段(优先文中部,避开标题/路径泄露)
    if cjk:
        seg = max(cjk, key=len)
        if len(seg) > 16:
            st = random.randint(0, len(seg)-12); seg = seg[st:st+random.randint(8,12)]
        out.append(("snippet_cjk", seg))
    elif len(txt) > 20:
        st = random.randint(0, max(1,len(txt)-16)); out.append(("snippet", txt[st:st+14].strip()))
    # B keywords: 2-4 个去重显著词
    pool = list(dict.fromkeys([w for w in cjk if 4<=len(w)<=8][:4] + words[:3]))
    if len(pool) >= 2:
        out.append(("keywords", " ".join(pool[:4])))
    return out

print("[2] building eval set from real corpus ...")
# 采样有实质文本的已索引文件
cand_fids = list(fid_to_path.keys())
random.shuffle(cand_fids)
eval_cases = []   # (qkind, query, expect_fid)
# 按 file 聚合其首个较长 chunk 文本
file_text = {}
for i in range(n):
    fid = int(chunk_file[i])
    if fid not in file_text and len(chunk_text[i]) >= 30:
        file_text[fid] = chunk_text[i]
for fid in cand_fids:
    if fid not in file_text: continue
    for qk, q in make_queries(file_text[fid]):
        if len(q.strip()) >= 4:
            eval_cases.append((qk, q.strip(), fid))
    if len({e[2] for e in eval_cases}) >= N_QUERIES: break
print(f"    {len(eval_cases)} queries over {len({e[2] for e in eval_cases})} target files")

# ── 3. 批量嵌入所有查询(live API),记录吞吐 ──
print("[3] embedding queries (live BGE-M3) ...")
qtexts = [q for _,q,_ in eval_cases]
qvecs = []
emb_t = 0.0
BS = 48
for i in range(0, len(qtexts), BS):
    batch = qtexts[i:i+BS]
    t=time.time()
    r = requests.post(EMB_URL, headers=H, json={"model":"BAAI/bge-m3","input":batch}, timeout=120)
    emb_t += time.time()-t
    if r.status_code!=200:
        print("    embed FAIL", r.status_code, r.text[:200]); raise SystemExit
    for d in r.json()["data"]:
        v=np.asarray(d["embedding"],dtype=np.float32); v/=(np.linalg.norm(v) or 1); qvecs.append(v)
    time.sleep(0.2)
Q = np.vstack(qvecs)
print(f"    embedded {len(qtexts)} queries in {emb_t:.1f}s ({len(qtexts)/emb_t:.1f}/s)")

# ── 4. 词法车道(FTS5,忠实复刻 retrieve.rs:body MATCH + bm25,短词 LIKE 兜底) ──
def lex_search(query, k=50):
    terms = CJK.findall(query) + WORD.findall(query)
    terms = [t for t in terms if len(t)>=3]
    files=[]
    if terms:
        expr = " OR ".join(f'"{t}"' for t in terms)
        try:
            for (rel,) in c.execute(
                "SELECT f.relpath FROM lex l JOIN files f ON f.id=l.rowid "
                "WHERE l.body MATCH ? ORDER BY bm25(lex) LIMIT ?", (expr,k)):
                files.append(rel)
        except Exception:
            pass
    if not files:  # 短词/无 trigram 命中 → LIKE 兜底(取最长词)
        longest = max(terms+[query], key=len)
        try:
            for (rel,) in c.execute(
                "SELECT f.relpath FROM lex l JOIN files f ON f.id=l.rowid "
                "WHERE l.body LIKE ? LIMIT ?", (f"%{longest}%",k)):
                files.append(rel)
        except Exception:
            pass
    return files

# ── 5. 向量车道:cosine 全扫 → top chunks → 去重到 file 级 ──
def vec_search(qi, k=50):
    sims = Mn @ Q[qi]
    idx = np.argpartition(-sims, min(400,n-1))[:400]
    idx = idx[np.argsort(-sims[idx])]
    seen=set(); files=[]
    for j in idx:
        fid=int(chunk_file[j])
        if fid in seen: continue
        seen.add(fid); files.append(fid_to_path[fid])
        if len(files)>=k: break
    return files

def rrf(*lists, k=RRF_K, topn=K):
    score={}
    for lst in lists:
        for rank,p in enumerate(lst):
            score[p]=score.get(p,0)+1.0/(k+rank+1)
    return [p for p,_ in sorted(score.items(), key=lambda x:-x[1])][:topn]

# ── 6. 跑全卷,算 recall@K + MRR(按车道 & 按查询类型) ──
print("[4] running retrieval on all lanes ...")
def first_rank(files, expect_path):
    for r,p in enumerate(files):
        if p==expect_path: return r+1
    return None

agg = {}  # lane -> list of ranks
agg_by_kind = {}
lat = {"vector":[], "lexical":[]}
for qi,(qk,q,fid) in enumerate(eval_cases):
    expect = fid_to_path[fid]
    t=time.time(); vec = vec_search(qi, 50); lat["vector"].append(time.time()-t)
    t=time.time(); lex = lex_search(q, 50); lat["lexical"].append(time.time()-t)
    hyb = rrf(vec, lex)
    for lane,files in [("vector",vec[:K]),("lexical",lex[:K]),("hybrid",hyb)]:
        rank = first_rank(files, expect)
        agg.setdefault(lane,[]).append(rank)
        agg_by_kind.setdefault((lane,qk),[]).append(rank)

def metrics(ranks):
    nn=len(ranks)
    if nn==0: return (0,0,0)
    hit=sum(1 for r in ranks if r)
    mrr=sum(1.0/r for r in ranks if r)/nn
    return (hit/nn, mrr, nn)

print("\n================ ACCURACY (on indexed corpus) ================")
print(f"{'lane':10} {'recall@%d'%K:10} {'MRR':8} {'n':>5}")
res={}
for lane in ["lexical","vector","hybrid"]:
    rc,mrr,nn = metrics(agg[lane]); res[lane]={"recall":rc,"mrr":mrr,"n":nn}
    print(f"{lane:10} {rc:8.3f}   {mrr:6.3f}  {nn:5}")
print("\n--- by query kind ---")
kinds=sorted({k for (_,k) in agg_by_kind})
for lane in ["lexical","vector","hybrid"]:
    for qk in kinds:
        rk=agg_by_kind.get((lane,qk))
        if rk:
            rc,mrr,nn=metrics(rk)
            print(f"  {lane:9} {qk:13} recall@{K}={rc:.3f} MRR={mrr:.3f} n={nn}")
print("\n--- lane compute latency (local, excl. network embed) ---")
print(f"  vector cosine scan: mean {np.mean(lat['vector'])*1000:.1f}ms  p95 {np.percentile(lat['vector'],95)*1000:.1f}ms")
print(f"  lexical FTS5 query: mean {np.mean(lat['lexical'])*1000:.1f}ms  p95 {np.percentile(lat['lexical'],95)*1000:.1f}ms")

out={"k":K,"n_queries":len(eval_cases),"corpus":{"chunks":n,"files":len(fid_to_path)},
     "query_embed_throughput_per_s":round(len(qtexts)/emb_t,1),
     "results":res,
     "lane_latency_ms":{"vector_mean":round(np.mean(lat['vector'])*1000,1),
                        "lexical_mean":round(np.mean(lat['lexical'])*1000,1)}}
open(os.path.join(os.path.dirname(__file__),"eval_result.json"),"w",encoding="utf-8").write(json.dumps(out,ensure_ascii=False,indent=2))
print("\nwritten eval_result.json")
