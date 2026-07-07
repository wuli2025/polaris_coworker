# -*- coding: utf-8 -*-
# AI 辅助检索实测:用 app 自带的 claude(headless)把查询**多路扩写**(同义/相关说法),
# 各变体并行召回 → 多查询 RRF 融合 → 真文件因「多变体同时投票」上浮 → 提升 recall@10 / MRR。
# 扩写结果缓存到 expansions.json(可复现、避免重复打模型)。零云余额依赖(嵌入走本地 ONNX)。
import os, sqlite3, time, random, re, json, subprocess, sys
import numpy as np, onnxruntime as ort
from tokenizers import Tokenizer
DB=os.path.join(os.environ["USERPROFILE"],"Polaris","data","fable.db")
MD=os.path.join(os.environ["USERPROFILE"],"Polaris","models","fastembed","bge-m3-int8")
HERE=os.path.dirname(__file__); EXP_CACHE=os.path.join(HERE,"expansions.json")
random.seed(42); K=10; N_QUERIES=int(os.environ.get("EVAL_N","150"))
NVAR=int(os.environ.get("EVAL_NVAR","3"))
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
    return np.vstack(out) if out else np.zeros((0,1024),np.float32)
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

# ── AI 扩写(batched headless claude),缓存到 expansions.json ──
def claude_expand(batch):
    prompt=("你是检索查询扩写器。把每条搜索查询扩写成 %d 个**用于全文检索**的等价说法"
            "(中文同义词/相关术语/英文对应词,保留关键专名)。只输出紧凑 JSON:对象,键=原查询字符串,"
            "值=%d 个字符串的数组。不要解释。查询:%s" % (NVAR,NVAR,json.dumps(batch,ensure_ascii=False)))
    try:
        p=subprocess.run(["claude","--print","--permission-mode=bypassPermissions","--allowedTools",""],
                         input=prompt.encode("utf-8"),capture_output=True,timeout=180)
        out=p.stdout.decode("utf-8","ignore")
        s=out.find("{"); e=out.rfind("}")
        if s>=0 and e>s: return json.loads(out[s:e+1])
    except Exception as ex:
        print("   expand batch err",str(ex)[:120],flush=True)
    return {}
exp_map={}
if os.path.exists(EXP_CACHE):
    exp_map=json.load(open(EXP_CACHE,encoding="utf-8"))
uniq=list(dict.fromkeys(q for _,q,_ in cases))
todo=[q for q in uniq if q not in exp_map]
print(f"[3] AI expand: {len(uniq)} uniq, {len(todo)} to fetch ...",flush=True)
BS=25
for i in range(0,len(todo),BS):
    batch=todo[i:i+BS]; t=time.time()
    m=claude_expand(batch)
    for q in batch: exp_map[q]=m.get(q,[]) if isinstance(m.get(q,[]),list) else []
    json.dump(exp_map,open(EXP_CACHE,"w",encoding="utf-8"),ensure_ascii=False)
    print(f"    {min(i+BS,len(todo))}/{len(todo)} ({time.time()-t:.0f}s, got {sum(1 for q in batch if exp_map.get(q))}/{len(batch)})",flush=True)

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
def rrf(lists_w,k=10,topn=60):
    sc={}
    for lst,wi in lists_w:
        for r,p in enumerate(lst): sc[p]=sc.get(p,0)+wi/(k+r+1)
    return [p for p,_ in sorted(sc.items(),key=lambda x:-x[1])][:topn]
# 覆盖加权(anchored 到原查询词)
fid_chunks={}
for i in range(n): fid_chunks.setdefault(int(chunk_file[i]),[]).append(i)
path_to_fid={v:k for k,v in fid_to_path.items()}
def q_terms(query):
    ts=[w.lower() for w in WORD.findall(query)]
    for seg in CJK.findall(query):
        ch=list(seg)
        if len(ch)==1: ts.append(seg)
        else:
            for w in range(len(ch)-1): ts.append("".join(ch[w:w+2]))
    return list(dict.fromkeys(ts))
def cov_and_phrase(path,terms,qfull):
    """返回 (覆盖率∈[0,1], 是否整句子串命中)。"""
    fid=path_to_fid.get(path)
    if fid is None: return (0.0,False)
    best=0; phrase=False
    for gi in fid_chunks.get(fid,[]):
        tl=chunk_text[gi].lower()
        if qfull and len(qfull)>=4 and qfull in tl: phrase=True
        if terms:
            cov=sum(1 for t in terms if t in tl)
            if cov>best: best=cov
        if best==len(terms) and phrase: break
    return (best/len(terms) if terms else 0.0, phrase)
def rrf_cov(lists_w,terms,k=10,topn=60,boost=0.25,qfull="",pboost=0.5):
    sc={}
    for lst,wi in lists_w:
        for r,p in enumerate(lst): sc[p]=sc.get(p,0)+wi/(k+r+1)
    for p in list(sc.keys()):
        cov,ph=cov_and_phrase(p,terms,qfull)
        sc[p]+=boost*cov + (pboost if ph else 0.0)
    return [p for p,_ in sorted(sc.items(),key=lambda x:-x[1])][:topn]

# ── 预嵌入:原查询 + 所有变体(本地 dense)──
print("[4] embedding originals+variants ...",flush=True)
all_texts=[]; idx_of={}
def reg(t):
    if t not in idx_of: idx_of[t]=len(all_texts); all_texts.append(t)
    return idx_of[t]
for _,q,_ in cases:
    reg(q)
    for v in exp_map.get(q,[])[:NVAR]:
        if isinstance(v,str) and v.strip(): reg(v.strip())
EMB=dense(all_texts)
print(f"    embedded {len(all_texts)} texts",flush=True)

def fr(files,exp):
    for r,p in enumerate(files):
        if p==exp: return r+1
    return None
def met(ranks):
    nn=len(ranks)
    if nn==0: return (0,0,0,0)
    hit=sum(1 for r in ranks if r); mrr=sum(1.0/r for r in ranks if r)/nn; ndcg=sum(1.0/np.log2(r+1) for r in ranks if r)/nn
    return (hit/nn,mrr,ndcg,nn)
print("[5] retrieval: baseline / +coverage / +expand-union+coverage ...",flush=True)
agg={"hybrid":[], "hyb+cov":[], "expand+cov":[]}; aggk={}
LEXK=60; VECK=60; VARW=float(os.environ.get("EVAL_VARW","0.4")); COVB=float(os.environ.get("EVAL_COVB","0.30")); PBOOST=float(os.environ.get("EVAL_PBOOST","0.8"))
poolrec={"base":[], "expand":[]}
for qi,(qk,q,fid) in enumerate(cases):
    exp=fid_to_path[fid]; qd=EMB[idx_of[q]]; terms=q_terms(q); qfull=q.strip().lower()
    lex=lex_search(q,LEXK); vec=vec_search(qd,VECK)
    base=rrf([(vec,0.85),(lex,1.0)],10,60)
    agg["hybrid"].append(fr(base[:K],exp)); aggk.setdefault(("hybrid",qk),[]).append(fr(base[:K],exp))
    # +覆盖加权 +整句精确命中加权(无扩写)
    covd=rrf_cov([(vec,0.85),(lex,1.0)],terms,10,60,COVB,qfull,PBOOST)
    agg["hyb+cov"].append(fr(covd[:K],exp)); aggk.setdefault(("hyb+cov",qk),[]).append(fr(covd[:K],exp))
    # 扩写并入候选池(变体只补召回,排序仍由原查询 RRF + 覆盖加权 anchored 到原词)
    variants=[v.strip() for v in exp_map.get(q,[])[:NVAR] if isinstance(v,str) and v.strip()]
    lists_w=[(vec,0.85),(lex,1.0)]
    for vv in variants:
        lists_w.append((lex_search(vv,LEXK),1.0*VARW)); lists_w.append((vec_search(EMB[idx_of[vv]],VECK),0.85*VARW))
    exu=rrf_cov(lists_w,terms,10,60,COVB,qfull,PBOOST)
    agg["expand+cov"].append(fr(exu[:K],exp)); aggk.setdefault(("expand+cov",qk),[]).append(fr(exu[:K],exp))
    poolrec["base"].append(1 if exp in base else 0); poolrec["expand"].append(1 if exp in exu else 0)
    if (qi+1)%50==0: print(f"    {qi+1}/{len(cases)}",flush=True)
print(f"    pool recall: base={np.mean(poolrec['base']):.3f}  expand={np.mean(poolrec['expand']):.3f}",flush=True)

print("\n========== AI EXPANSION RETRIEVAL ==========")
print(f"corpus {n} chunks/{len(fid_to_path)} files | N={len(cases)} | NVAR={NVAR}")
print(f"{'config':16}{'recall@10':10}{'MRR':8}{'nDCG':8}{'n':>5}")
res={}
order=["hybrid","hyb+cov","expand+cov"]
for name in order:
    rc,mrr,nd,nn=met(agg[name]); res[name]={"recall":rc,"mrr":mrr,"ndcg":nd,"n":nn}
    print(f"{name:16}{rc:8.3f}  {mrr:6.3f}  {nd:6.3f} {nn:5}")
print("\n--- by query kind ---")
kinds=sorted({k for (_,k) in aggk})
for name in order:
    for qk in kinds:
        rk=aggk.get((name,qk))
        if rk: rc,mrr,nd,nn=met(rk); print(f"  {name:12}{qk:13}recall@10={rc:.3f} MRR={mrr:.3f} nDCG={nd:.3f} n={nn}")
json.dump({"N":len(cases),"NVAR":NVAR,"results":res},open(os.path.join(HERE,"eval_ai_expand_result.json"),"w",encoding="utf-8"),ensure_ascii=False,indent=2)
print("\nwritten eval_ai_expand_result.json")
