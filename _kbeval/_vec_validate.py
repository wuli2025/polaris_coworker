# -*- coding: utf-8 -*-
# 验证 _vec_build 写入的向量与后端 retrieve 完全兼容:
#  ① 维度/blob字节/bits长度/model  ② bits = 符号位(packbits little)  ③ 自检索余弦≈1(归一化+存储正确)
import sqlite3, os, time
import numpy as np, requests
DB = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
KEY = "sk-ulmygegatdtjdcyxzuscfcdzcogwoirijrcnjbtzsylphzgw"
URL = "https://api.siliconflow.cn/v1/embeddings"
H = {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"}
c = sqlite3.connect(DB, timeout=60)

# 取最近写入的若干 chunk(rowid 最大 = 最新)
rows = c.execute("SELECT file_id, seq, text, dim, vec, model, bits FROM chunks "
                 "WHERE model='BAAI/bge-m3' ORDER BY rowid DESC LIMIT 8").fetchall()
print(f"checking {len(rows)} recent chunks")
ok_struct = True
texts, stored = [], []
for fid, seq, text, dim, vec, model, bits in rows:
    v = np.frombuffer(vec, dtype=np.float32)
    bad = []
    if dim != 1024: bad.append(f"dim={dim}")
    if len(vec) != 1024*4: bad.append(f"vecbytes={len(vec)}")
    if len(bits) != 128: bad.append(f"bitslen={len(bits)}")
    if abs(float(np.linalg.norm(v))-1.0) > 1e-3: bad.append(f"norm={np.linalg.norm(v):.4f}")
    # bits 应等于 符号位 packbits little
    exp_bits = np.packbits((v>=0).astype(np.uint8), bitorder='little').tobytes()
    if exp_bits != bits: bad.append("bits_mismatch")
    prev = text[:40].encode('ascii', 'replace').decode('ascii')
    print(f"  fid={fid} seq={seq} {'OK' if not bad else 'BAD:'+','.join(bad)}  '{prev}'")
    if bad: ok_struct = False
    texts.append(text); stored.append(v)

# 自检索:把这些 chunk 文本重新嵌入,与库里存的向量算余弦,应 ≈1
print("\n[self-retrieval cosine] 重新嵌入 chunk 文本 vs 库存向量:")
r = requests.post(URL, headers=H, json={"model":"BAAI/bge-m3","input":texts}, timeout=120)
fresh = [np.asarray(d["embedding"], dtype=np.float32) for d in r.json()["data"]]
cmin = 1.0
for i,(fv,sv) in enumerate(zip(fresh,stored)):
    fv = fv/ (np.linalg.norm(fv) or 1)
    cos = float(fv @ sv)
    cmin = min(cmin, cos)
    print(f"  chunk {i}: cos={cos:.4f}")
print(f"\nstruct_ok={ok_struct}  min_self_cos={cmin:.4f}  "
      f"=> {'PASS' if ok_struct and cmin>0.98 else 'FAIL'}")
