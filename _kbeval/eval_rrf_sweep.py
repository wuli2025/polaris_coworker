# -*- coding: utf-8 -*-
# RRF 调参考卷(配合 P1④ 可调化):向量腿/词法腿的候选列表与 RRF 无关 —— 只算一次,
# 然后对 (k, w_vec, w_grep) 网格做廉价融合扫参,找出 recall@10 / MRR 最优的 RRF 参数,
# 再用 POLARIS_RRF_K / POLARIS_RRF_W_VEC / POLARIS_RRF_W_GREP 固化进生产(零重嵌、零重建)。
#
# 这正面回答报告 §4 的发现「无重排时 RRF 融合反把排序拉低(hybrid MRR < 纯词法)」:
# 把弱腿(向量)降权,看 MRR 能否回到 ≥ 纯词法,同时尽量不掉 recall。
#
# 用法: python eval_rrf_sweep.py   (需本机 fable.db + 硅基流动 key,与 eval_accuracy.py 同源)
import os, sqlite3, time, random, re, json
import numpy as np, requests

DB  = os.path.join(os.environ["USERPROFILE"], "Polaris", "data", "fable.db")
KEY = os.environ.get("POLARIS_EVAL_KEY", "sk-ulmygegatdtjdcyxzuscfcdzcogwoirijrcnjbtzsylphzgw")
EMB_URL = "https://api.siliconflow.cn/v1/embeddings"
H = {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"}
random.seed(42)
K = 10
N_QUERIES = 120
# 扫参网格:k 越小头部越锐;w_vec<1 给弱向量腿降权(报告建议方向)。
GRID_K     = [10, 20, 30, 45, 60, 90]
GRID_WVEC  = [0.4, 0.55, 0.7, 0.85, 1.0]
W_GREP     = 1.0   # 强腿(词法)恒为 1.0 作基准

CJK  = re.compile(r'[㐀-䶿一-鿿]{4,}')
WORD = re.compile(r'[A-Za-z][A-Za-z0-9_]{3,}')

c = sqlite3.connect(DB); c.execute("PRAGMA temp_store=MEMORY")

print("[1] loading vectors ...")
rows = c.execute("""
  SELECT ch.vec, ch.file_id, f.relpath, ch.text
  FROM chunks ch JOIN files f ON f.id=ch.file_id
  WHERE ch.model='BAAI/bge-m3' AND ch.vec IS NOT NULL
""").fetchall()
n = len(rows); dim = len(rows[0][0]) // 4
M = np.empty((n, dim), dtype=np.float32)
chunk_file = np.empty(n, dtype=np.int64); fid_to_path = {}; chunk_text = [None]*n
for i,(b,fid,rel,txt) in enumerate(rows):
    M[i] = np.frombuffer(b, dtype=np.float32, count=dim)
    chunk_file[i] = fid; fid_to_path[fid] = rel; chunk_text[i] = txt or ""
nrm = np.linalg.norm(M,axis=1,keepdims=True); nrm[nrm==0]=1; Mn = M/nrm
print(f"    {n} chunks / {len(fid_to_path)} files / dim={dim}")

def make_queries(txt):
    txt = re.sub(r'\s+',' ', txt).strip(); out=[]
    cjk = CJK.findall(txt); words = WORD.findall(txt)
    if cjk:
        seg = max(cjk,key=len)
        if len(seg)>16:
            st=random.randint(0,len(seg)-12); seg=seg[st:st+random.randint(8,12)]
        out.append(("snippet_cjk",seg))
    pool=list(dict.fromkeys([w for w in cjk if 4<=len(w)<=8][:4]+words[:3]))
    if len(pool)>=2: out.append(("keywords"," ".join(pool[:4])))
    return out

file_text={}
for i in range(n):
    fid=int(chunk_file[i])
    if fid not in file_text and len(chunk_text[i])>=30: file_text[fid]=chunk_text[i]
cand=list(fid_to_path.keys()); random.shuffle(cand)
cases=[]
for fid in cand:
    if fid not in file_text: continue
    for qk,q in make_queries(file_text[fid]):
        if len(q.strip())>=4: cases.append((qk,q.strip(),fid))
    if len({e[2] for e in cases})>=N_QUERIES: break
print(f"[2] {len(cases)} queries")

print("[3] embedding queries (live) ...")
qtexts=[q for _,q,_ in cases]; qvecs=[]
for i in range(0,len(qtexts),48):
    r=requests.post(EMB_URL,headers=H,json={"model":"BAAI/bge-m3","input":qtexts[i:i+48]},timeout=120)
    if r.status_code!=200: print("embed FAIL",r.status_code,r.text[:200]); raise SystemExit
    for d in r.json()["data"]:
        v=np.asarray(d["embedding"],dtype=np.float32); v/=(np.linalg.norm(v) or 1); qvecs.append(v)
    time.sleep(0.2)
Q=np.vstack(qvecs)

def lex_search(query,k=50):
    terms=[t for t in CJK.findall(query)+WORD.findall(query) if len(t)>=3]; files=[]
    if terms:
        expr=" OR ".join(f'"{t}"' for t in terms)
        try:
            for (rel,) in c.execute("SELECT f.relpath FROM lex l JOIN files f ON f.id=l.rowid "
                "WHERE l.body MATCH ? ORDER BY bm25(lex) LIMIT ?", (expr,k)): files.append(rel)
        except Exception: pass
    if not files:
        longest=max(terms+[query],key=len)
        try:
            for (rel,) in c.execute("SELECT f.relpath FROM lex l JOIN files f ON f.id=l.rowid "
                "WHERE l.body LIKE ? LIMIT ?", (f"%{longest}%",k)): files.append(rel)
        except Exception: pass
    return files

# 条件融合阈值网格(POLARIS_VEC_MIN_SCORE):只让余弦≥阈值的向量命中参与融合,门掉噪声。
# 0.0=关闭(零回归);BGE-M3 归一化余弦经验区间 0.3~0.5。
GRID_VMIN = [0.0, 0.3, 0.35, 0.4, 0.45]

def vec_search(qi,k=50):
    # 返回 (file, cosine):cosine 用于条件融合阈值门控(对齐后端 POLARIS_VEC_MIN_SCORE)。
    sims=Mn@Q[qi]; idx=np.argpartition(-sims,min(400,n-1))[:400]; idx=idx[np.argsort(-sims[idx])]
    seen=set(); files=[]
    for j in idx:
        fid=int(chunk_file[j])
        if fid in seen: continue
        seen.add(fid); files.append((fid_to_path[fid], float(sims[j])))
        if len(files)>=k: break
    return files

# 每题只算一次两腿列表(与 RRF 参数无关);带 query 供路径加权(对齐后端 path_boost)。
print("[4] computing lane lists once ...")
PBOOST = float(os.environ.get("POLARIS_PATH_BOOST", "0.01"))   # 后端默认 0.01,设 0 关
per=[]   # (expect_path, vec_list[(path,cos)], lex_list[path], q)
for qi,(qk,q,fid) in enumerate(cases):
    per.append((fid_to_path[fid], vec_search(qi,50), lex_search(q,50), q))

def path_boost(path,q_full,terms,w=PBOOST):
    # 对齐后端 retrieve.rs::path_boost:文件名整句命中 ×3 / 路径整句 ×2 / 逐内容词 ×1,× 权重。
    if w<=0: return 0.0
    p=path.lower().replace('\\','/'); base=p.rsplit('/',1)[-1]; sig=0.0
    if len(q_full)>=2:
        if q_full in base: sig+=3
        elif q_full in p: sig+=2
    for t in terms:
        if len(t)>=2 and t in p: sig+=1
    return w*sig

def wrrf(vec,lex,k,wv,wg,vmin=0.0,q="",topn=K):
    # 条件融合:余弦 < vmin 的向量命中不注入(对齐后端 retrieve.rs vec_min_score 闸)。
    score={}
    rank=0
    for p,cos in vec:
        if cos < vmin: continue
        score[p]=score.get(p,0)+wv/(k+rank); rank+=1
    for rank,p in enumerate(lex): score[p]=score.get(p,0)+wg/(k+rank)
    # 路径/文件名命中加权(融合层,覆盖两腿)。
    if PBOOST>0 and q:
        q_full=q.lower(); terms=[t for t in CJK.findall(q)+WORD.findall(q) if len(t)>=2]
        for p in list(score.keys()): score[p]+=path_boost(p,q_full,terms)
    return [p for p,_ in sorted(score.items(),key=lambda x:-x[1])][:topn]

def metrics(lists):
    # recall@K / MRR / nDCG@K(二值单期望:nDCG=1/log2(rank+1))。
    rec=mrr=ndcg=0; nn=len(lists)
    for files,exp in lists:
        r=next((i+1 for i,p in enumerate(files) if p==exp),None)
        if r: rec+=1; mrr+=1.0/r; ndcg+=1.0/np.log2(r+1)
    return rec/nn, mrr/nn, ndcg/nn

# 基准:纯词法 / 纯向量(向量列表此时是 (path,cos),取 path)
lex_only=[(p[2][:K],p[0]) for p in per]
vec_only=[([pp for pp,_ in p[1][:K]],p[0]) for p in per]
lex_rc,lex_mr,lex_nd = metrics(lex_only)
vec_rc,vec_mr,vec_nd = metrics(vec_only)
print("\n=========== RRF SWEEP (recall@10 / MRR / nDCG@10) ===========")
print(f"baseline lexical : recall={lex_rc:.3f} MRR={lex_mr:.3f} nDCG={lex_nd:.3f}")
print(f"baseline vector  : recall={vec_rc:.3f} MRR={vec_mr:.3f} nDCG={vec_nd:.3f}")
print(f"\n{'k':>4} {'w_vec':>6} {'vmin':>5} {'recall@10':>10} {'MRR':>7} {'nDCG':>7}")
best=None; table=[]
for k in GRID_K:
    for wv in GRID_WVEC:
        for vmin in GRID_VMIN:
            lists=[(wrrf(p[1],p[2],k,wv,W_GREP,vmin,p[3]),p[0]) for p in per]
            rc,mr,nd=metrics(lists)
            table.append({"k":k,"w_vec":wv,"vmin":vmin,"recall":round(rc,4),"mrr":round(mr,4),"ndcg":round(nd,4)})
            print(f"{k:>4} {wv:>6.2f} {vmin:>5.2f} {rc:>10.3f} {mr:>7.3f} {nd:>7.3f}")
            # 选优先 MRR(报告痛点是排序),recall 不低于纯词法 - 0.01 作护栏
            if rc>=lex_rc-0.01:
                if best is None or mr>best["mrr"]:
                    best={"k":k,"w_vec":wv,"vmin":vmin,"recall":round(rc,4),"mrr":round(mr,4),"ndcg":round(nd,4)}
print("\n>>> 推荐(MRR 最优且 recall 不低于纯词法):", best)
if best:
    print(">>> 固化: $env:POLARIS_RRF_K=\"%s\"; $env:POLARIS_RRF_W_VEC=\"%s\"; $env:POLARIS_VEC_MIN_SCORE=\"%s\""
          % (best['k'], best['w_vec'], best['vmin']))
    # 诚实提示:融合到底有没有打赢纯词法?没赢就别强行融合。
    verdict = "[OK] 融合优于纯词法" if best["mrr"]>lex_mr+1e-6 else "[WARN] 融合未超纯词法 → 建议向量腿仅作召回补充/或调 vmin 更严"
    print(">>>", verdict)
out={"baseline":{"lexical":{"recall":round(lex_rc,4),"mrr":round(lex_mr,4),"ndcg":round(lex_nd,4)},
                 "vector":{"recall":round(vec_rc,4),"mrr":round(vec_mr,4),"ndcg":round(vec_nd,4)}},
     "grid":table,"recommend":best}
open(os.path.join(os.path.dirname(__file__),"eval_rrf_sweep.json"),"w",encoding="utf-8").write(
    json.dumps(out,ensure_ascii=False,indent=2,default=lambda o:round(float(o),4)))
print("\nwritten eval_rrf_sweep.json")
