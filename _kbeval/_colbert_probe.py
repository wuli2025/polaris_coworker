# -*- coding: utf-8 -*-
# 验证本地 ColBERT MaxSim 排序是否正确:给一段 chunk 的子串当查询,
# 该 chunk 文本应在一堆随机 chunk 里 MaxSim 最高。
import os, sqlite3, numpy as np, onnxruntime as ort
from tokenizers import Tokenizer
MD = os.path.join(os.environ['USERPROFILE'],'Polaris','models','fastembed','bge-m3-int8')
DB = os.path.join(os.environ['USERPROFILE'],'Polaris','data','fable.db')
so=ort.SessionOptions(); so.intra_op_num_threads=8
S=ort.InferenceSession(os.path.join(MD,'model_quantized.onnx'),so,providers=['CPUExecutionProvider'])
T=Tokenizer.from_file(os.path.join(MD,'tokenizer.json')); T.enable_padding(length=None)
def colb(texts, ml=256):
    T.enable_truncation(max_length=ml)
    enc=T.encode_batch([t if t.strip() else ' ' for t in texts])
    ids=np.array([e.ids for e in enc],dtype=np.int64); mask=np.array([e.attention_mask for e in enc],dtype=np.int64)
    cb=S.run(['colbert_vecs'],{'input_ids':ids,'attention_mask':mask})[0]
    out=[]
    L=cb.shape[1]
    for r in range(cb.shape[0]):
        m=mask[r].astype(bool)
        if L==m.shape[0]-1: m=m[1:]
        elif L<m.shape[0]: m=m[:L]
        v=cb[r][:m.shape[0]][m]; v=v/(np.linalg.norm(v,axis=1,keepdims=True)+1e-9); out.append(v.astype(np.float32))
    return out
def maxsim(q,d):
    if d.shape[0]==0: return -1e9
    return float((q@d.T).max(axis=1).sum())
c=sqlite3.connect(DB)
rows=c.execute("SELECT text FROM chunks WHERE model='BAAI/bge-m3' AND length(text)>400 ORDER BY rowid DESC LIMIT 40").fetchall()
texts=[r[0] for r in rows]
true=texts[0]
# 查询 = true 文本中段 12 字
import re
cj=re.findall(r'[一-鿿]{6,}',true)
q = (max(cj,key=len)[:12] if cj else true[20:34])
print("query:",q)
qc=colb([q],ml=64)[0]
dc=colb(texts,ml=256)
scores=[maxsim(qc,d) for d in dc]
order=np.argsort(-np.asarray(scores))
print("true chunk rank:", list(order).index(0)+1, "of", len(texts))
print("top5 scores idx:",[(int(i),round(scores[i],2)) for i in order[:5]])
print("true score:",round(scores[0],2),"max:",round(max(scores),2))
