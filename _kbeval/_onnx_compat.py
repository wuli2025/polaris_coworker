# -*- coding: utf-8 -*-
# 验证本地 INT8 ONNX 嵌入 与 已入库云 fp32 嵌入 同空间兼容(余弦应≥0.95)。
import onnxruntime as ort, os, sqlite3, numpy as np
from tokenizers import Tokenizer
md = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'models', 'fastembed', 'bge-m3-int8')
DB = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')

so = ort.SessionOptions(); so.intra_op_num_threads = 8
sess = ort.InferenceSession(os.path.join(md,'model_quantized.onnx'), so, providers=['CPUExecutionProvider'])
tok = Tokenizer.from_file(os.path.join(md,'tokenizer.json'))
tok.enable_truncation(max_length=512); tok.enable_padding(length=None)

def local_embed(texts):
    enc = tok.encode_batch(texts)
    ids  = np.array([e.ids for e in enc], dtype=np.int64)
    mask = np.array([e.attention_mask for e in enc], dtype=np.int64)
    dense = sess.run(['dense_vecs'], {'input_ids':ids,'attention_mask':mask})[0]
    out=[]
    for v in dense.astype(np.float32):
        n=float(np.linalg.norm(v));
        if n>1e-12: v=v/n
        out.append(v)
    return out

c = sqlite3.connect(DB)
rows = c.execute("SELECT text, vec FROM chunks WHERE model='BAAI/bge-m3' AND length(text)>300 "
                 "AND file_id IN (SELECT file_id FROM chunks WHERE model='BAAI/bge-m3' "
                 "GROUP BY file_id ORDER BY file_id*2654435761%100000 LIMIT 8) "
                 "GROUP BY file_id LIMIT 8").fetchall()
texts = [t for t,_ in rows]
cloud = [np.frombuffer(v,dtype=np.float32) for _,v in rows]
local = local_embed(texts)
print("text# | cos(local_int8, cloud_fp32) | self-norm")
cmin=1.0
for i,(lv,cv) in enumerate(zip(local,cloud)):
    cos=float(lv @ (cv/ (np.linalg.norm(cv) or 1)))
    cmin=min(cmin,cos)
    print(f"  {i}   {cos:.4f}   |local|={np.linalg.norm(lv):.4f}")
# 交叉验证:本地 query 对 cloud 库的「找回自己」是否仍命中(同空间的实证)
print(f"\nmin cross-space cosine = {cmin:.4f}  => {'COMPATIBLE' if cmin>0.95 else 'RISK: spaces differ'}")
