# -*- coding: utf-8 -*-
# 召回诊断(快、零 colbert):按查询类型(snippet/keywords)拆解 lex/vector/hybrid 的
# recall@k / MRR / nDCG,并量「融合池 recall@40」天花板与「加宽车道」的增益。
# 目的:定位精度损失在哪个查询类型、哪条腿,据此决定 AI 辅助(查询扩写/HyDE)往哪打。
import os, sqlite3, time, random, re, json
import numpy as np, onnxruntime as ort
from tokenizers import Tokenizer
DB = os.path.join(os.environ["USERPROFILE"],"Polaris","data","fable.db")
MD = os.path.join(os.environ["USERPROFILE"],"Polaris","models","fastembed","bge-m3-int8")
random.seed(42)
K=10; N_QUERIES=int(os.environ.get("EVAL_N","150"))
CJK=re.compile(r'[㐀-䶿一-鿿]{1,}'); CJK4=re.compile(r'[㐀-䶿一-鿿]{4,}'); WORD=re.compile(r'[A-Za-z][A-Za-z0-9_]{3,}')
so=ort.SessionOptions(); so.intra_op_num_threads=12
SESS=ort.InferenceSession(os.path.join(MD,"model_quantized.onnx"),so,providers=["CPUExecutionProvider"])
TOK=Tokenizer.from_file(os.path.join(MD,"tokenizer.json")); TOK.enable_truncation(max_length=96); TOK.enable_padding(length=None)
def dense(texts,batch=32):
    out=[]
    for i in range(0,len(texts),batch):
        enc=TOK.encode_batch([t if t.strip() else " " for t in texts[i:i+batch]])
        ids=np.array([e.ids for e in enc],dtype=np.int64); mask=np.array([e.attention_mask for e in enc],dtype=np.int64)
        d=SESS.run(["dense_vecs"],{"input_ids":ids,"attention_mask":mask})[0]
        out.append((d/(np.linalg.norm(d,axis=1,keepdims=True)+1e-9)).astype(np.float32))
    return np.vstack(out)
print("[1] loading vectors ...",flush=True)
c=sqlite3.connect(DB); c.execute("PRAGMA temp_store=MEMORY")
rows=c.execute("SELECT ch.vec, ch.file_id, f.relpath, ch.text FROM chunks ch JOIN files f ON f.id=ch.file_id WHERE ch.model='BAAI/bge-m3' AND ch.vec IS NOT NULL").fetchall()
n=len(rows); dim=len(rows[0][0])//4
M=np.empty((n,dim),np.float32); chunk_file=np.empty(n,np.int64); fid_to_path={}; chunk_text=[None]*n
for i,(b,fid,rel,txt) in enumerate(rows):
    M[i]=np.frombuffer(b,np.float32,count=dim); chunk_file[i]=fid; fid_to_path[fid]=rel; chunk_text[i]=txt or ""
nrm=np.linalg.norm(M,axis=1,keepdims=True); nrm[nrm==0]=1; Mn=M/nrm
print(f"    {n} chunks / {len(fid_to_path)} files",flush=True)
file_text={}
for i in range(n):
    fid=int(chunk_file[i])
    if fid not in file_text and len(chunk_text[i])>=30: file_text[fid]=chunk_text[i]
def make_queries(txt):
    txt=re.sub(r'\s+',' ',txt).strip(); out=[]
    cjk=CJK4.findall(txt); words=WORD.findall(txt)
    if cjk:
        seg=max(cjk,key=len)
        if len(seg)>16: st=random.randint(0,len(seg)-12); seg=seg[st:st+random.randint(8,12)]
        out.append(("snippet_cjk",seg))
    elif len(txt)>20: st=random.randint(0,max(1,len(txt)-16)); out.append(("snippet",txt[st:st+14].strip()))
    pool=list(dict.fromkeys([w for w in cjk if 4<=len(w)<=8][:4]+words[:3]))
    if len(pool)>=2: out.append(("keywords"," ".join(pool[:4])))
    return out
cand=list(fid_to_path.keys()); random.shuffle(cand); cases=[]
for fid in cand:
    if fid not in file_text: continue
    for qk,q in make_queries(file_text[fid]):
        if len(q.strip())>=4: cases.append((qk,q.strip(),fid))
    if len({e[2] for e in cases})>=N_QUERIES: break
print(f"[2] {len(cases)} queries",flush=True)
def lex_search(query,k):
    terms=CJK.findall(query)+WORD.findall(query); trig=[]
    for t in terms:
        if re.match(r'[A-Za-z]',t):
            if len(t)>=3: trig.append(t.lower())
        else:
            ch=list(t)
            if len(ch)>=3:
                for w in range(len(ch)-2): trig.append("".join(ch[w:w+3]))
    trig=list(dict.fromkeys(trig))[:60]; files=[]
    if trig:
        expr=" OR ".join(f'"{t}"' for t in trig)
        try:
            for (rel,) in c.execute("SELECT f.relpath FROM lex l JOIN files f ON f.id=l.rowid WHERE l.body MATCH ? ORDER BY bm25(lex) LIMIT ?",(expr,k)): files.append(rel)
        except Exception: pass
    if not files:
        longest=max(terms+[query],key=len)
        try:
            for (rel,) in c.execute("SELECT f.relpath FROM lex l JOIN files f ON f.id=l.rowid WHERE l.body LIKE ? LIMIT ?",(f"%{longest}%",k)): files.append(rel)
        except Exception: pass
    return files
def vec_search(qd,k):
    sims=Mn@qd; topn=min(800,n-1); idx=np.argpartition(-sims,topn)[:topn]; idx=idx[np.argsort(-sims[idx])]
    seen=set(); files=[]
    for j in idx:
        fid=int(chunk_file[j])
        if fid in seen: continue
        seen.add(fid); files.append(fid_to_path[fid])
        if len(files)>=k: break
    return files
def rrf(lists,k=10,w=None,topn=40):
    sc={}
    for li,lst in enumerate(lists):
        wi=(w[li] if w else 1.0)
        for r,p in enumerate(lst): sc[p]=sc.get(p,0)+wi/(k+r+1)
    return [p for p,_ in sorted(sc.items(),key=lambda x:-x[1])][:topn]

# fid -> chunk 下标(全词覆盖加权用)
fid_chunks={}
for i in range(n): fid_chunks.setdefault(int(chunk_file[i]),[]).append(i)
def q_terms(query):
    ts=[w.lower() for w in WORD.findall(query)]
    for seg in CJK.findall(query):
        ch=list(seg)
        if len(ch)==1: ts.append(seg)
        else:
            for w in range(len(ch)-1): ts.append("".join(ch[w:w+2]))
    return list(dict.fromkeys(ts))
def term_coverage(path, terms):
    """该文件任一 chunk 命中的 distinct 查询词数 / 总词数 ∈ [0,1]。"""
    fid=path_to_fid.get(path)
    if fid is None or not terms: return 0.0
    best=0
    for gi in fid_chunks.get(fid,[]):
        tl=chunk_text[gi].lower()
        cov=sum(1 for t in terms if t in tl)
        if cov>best: best=cov
        if best==len(terms): break
    return best/len(terms)
path_to_fid={v:k for k,v in fid_to_path.items()}
def cand_covers(path, terms):
    """返回该候选命中的查询词集合(任一 chunk)。"""
    fid=path_to_fid.get(path); covered=set()
    if fid is None: return covered
    for gi in fid_chunks.get(fid,[]):
        tl=chunk_text[gi].lower()
        for t in terms:
            if t not in covered and t in tl: covered.add(t)
        if len(covered)==len(terms): break
    return covered
def rrf_andboost_idf(lists,q,k=10,w=None,topn=40,boost=0.3):
    """RRF + **池内 IDF 加权全词覆盖**:覆盖「池内稀有(更具区分度)的查询词」加分更多。
    池内 df 高的词(几乎所有候选都有 → 无区分度)权重低;只在少数候选出现的词权重高。"""
    sc={}
    for li,lst in enumerate(lists):
        wi=(w[li] if w else 1.0)
        for r,p in enumerate(lst): sc[p]=sc.get(p,0)+wi/(k+r+1)
    terms=q_terms(q)
    cands=list(sc.keys())
    if terms and cands:
        cov={p:cand_covers(p,terms) for p in cands}
        npool=len(cands)
        df={t:sum(1 for p in cands if t in cov[p]) for t in terms}
        import math
        idf={t:math.log((npool+1)/(df[t]+0.5)) for t in terms}
        maxw=sum(idf.values()) or 1.0
        for p in cands:
            s=sum(idf[t] for t in cov[p])
            sc[p]+=boost*(s/maxw)
    return [p for p,_ in sorted(sc.items(),key=lambda x:-x[1])][:topn]
def rrf_andboost(lists,q,k=10,w=None,topn=40,boost=0.06):
    """RRF + 全词覆盖加权:命中越多 distinct 查询词的文件越靠前(治关键词查询「含部分词的文件淹没含全词的」)。"""
    sc={}
    for li,lst in enumerate(lists):
        wi=(w[li] if w else 1.0)
        for r,p in enumerate(lst): sc[p]=sc.get(p,0)+wi/(k+r+1)
    terms=q_terms(q)
    for p in list(sc.keys()):
        sc[p]+=boost*term_coverage(p,terms)
    return [p for p,_ in sorted(sc.items(),key=lambda x:-x[1])][:topn]
print("[3] embedding queries (local dense)...",flush=True)
Qd=dense([q for _,q,_ in cases])
def fr(files,exp):
    for r,p in enumerate(files):
        if p==exp: return r+1
    return None
def met(ranks):
    nn=len(ranks);
    if nn==0: return (0,0,0,0)
    hit=sum(1 for r in ranks if r); mrr=sum(1.0/r for r in ranks if r)/nn; ndcg=sum(1.0/np.log2(r+1) for r in ranks if r)/nn
    return (hit/nn,mrr,ndcg,nn)
configs=["hyb(60/60)","ab.10","ab.20","ab.30","ab.50","abIDF.30","abIDF.50"]
agg={name:[] for name in configs}; aggk={}  # (name,kind)
poolrec={"hyb60@40":[], "wide@40":[], "lexonly@150":[], "veconly@120":[]}
for qi,(qk,q,fid) in enumerate(cases):
    exp=fid_to_path[fid]
    lex150=lex_search(q,150); vec120=vec_search(Qd[qi],120)
    lex60=lex150[:60]; vec60=vec120[:60]
    L=[vec120,lex150]; W=[0.85,1.0]
    res={
     "hyb(60/60)":rrf([vec60,lex60],10,W,40)[:K],
     "ab.10":rrf_andboost(L,q,10,W,40,0.10)[:K],
     "ab.20":rrf_andboost(L,q,10,W,40,0.20)[:K],
     "ab.30":rrf_andboost(L,q,10,W,40,0.30)[:K],
     "ab.50":rrf_andboost(L,q,10,W,40,0.50)[:K],
     "abIDF.30":rrf_andboost_idf(L,q,10,W,40,0.30)[:K],
     "abIDF.50":rrf_andboost_idf(L,q,10,W,40,0.50)[:K],
    }
    for name,files in res.items():
        rk=fr(files,exp); agg[name].append(rk); aggk.setdefault((name,qk),[]).append(rk)
    poolrec["hyb60@40"].append(1 if exp in rrf([vec60,lex60],10,[0.85,1.0],40) else 0)
    poolrec["wide@40"].append(1 if exp in rrf([vec120,lex150],10,[0.85,1.0],40) else 0)
    poolrec["lexonly@150"].append(1 if exp in lex150 else 0)
    poolrec["veconly@120"].append(1 if exp in vec120 else 0)
print("\n========== RECALL DIAGNOSIS ==========")
print(f"corpus {n} chunks/{len(fid_to_path)} files | N={len(cases)}")
print(f"{'config':20}{'recall@10':10}{'MRR':8}{'nDCG':8}{'n':>5}")
for name in configs:
    rc,mrr,nd,nn=met(agg[name]); print(f"{name:20}{rc:8.3f}  {mrr:6.3f}  {nd:6.3f} {nn:5}")
print("\n--- by query kind ---")
kinds=sorted({k for (_,k) in aggk})
for name in configs:
    for qk in kinds:
        rk=aggk.get((name,qk))
        if rk: rc,mrr,nd,nn=met(rk); print(f"  {name:20}{qk:13}recall@10={rc:.3f} MRR={mrr:.3f} nDCG={nd:.3f} n={nn}")
print("\n--- candidate-pool recall (rerank ceilings) ---")
for k,v in poolrec.items(): print(f"  {k:16}{np.mean(v):.3f}")
