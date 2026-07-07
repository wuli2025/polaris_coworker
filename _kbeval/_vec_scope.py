# -*- coding: utf-8 -*-
# 向量待办规模:local roots、kind='text'、size<=2MB(MAX_EMBED_FILE_BYTES)、ext∉{log,csv,tsv,ndjson}、chunked=0。
import sqlite3, os
db = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
c = sqlite3.connect(db, timeout=60)
def is_net(p): p=p.replace('/','\\'); return p.startswith('\\\\') or p.upper().startswith('UNC\\')
local = [rid for rid,p in c.execute("SELECT id,path FROM roots") if not is_net(p) and os.path.isdir(p)]
ph = ",".join("?"*len(local))
SKIP = ('log','csv','tsv','ndjson')
skip_ph = ",".join("?"*len(SKIP))
n, bytesum = c.execute(
  f"SELECT COUNT(*), COALESCE(SUM(size),0) FROM files "
  f"WHERE kind='text' AND size<=2000000 AND chunked=0 "
  f"AND lower(ext) NOT IN ({skip_ph}) AND root_id IN ({ph})", (*SKIP, *local)).fetchone()
already = c.execute("SELECT COUNT(DISTINCT file_id) FROM chunks WHERE model='BAAI/bge-m3' AND vec IS NOT NULL").fetchone()[0]
print(f"vector-pending local embeddable files : {n}")
print(f"  their total bytes                   : {bytesum/1e6:.0f} MB")
# 粗估 chunk 数:平均每 chunk ~1600 字,中文约 2-3 字节/字 → 每 chunk ~3000-4000 字节;按 3500 估。
est_chunks = int(bytesum/3500)
print(f"  est. chunks (~size/3500B)           : ~{est_chunks}")
print(f"already-vectored files (any root)     : {already}")
# 估时:云 BGE-M3 实测查询嵌入 ~21/s 单路;构建多路并发(~3-5路)+批量32 → 取 ~80 chunk/s 保守
for rate in (60, 100, 150):
    print(f"  @ {rate} chunk/s -> {est_chunks/rate/3600:.1f} h")
