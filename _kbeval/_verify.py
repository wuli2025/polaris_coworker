# -*- coding: utf-8 -*-
# 验证词法专扫成效:① 覆盖率(本地/NAS 分拆) ② 端到端——挑「无向量的盲区文件」证明现在能靠倒排搜到。
import sqlite3, os, re
db = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
c = sqlite3.connect(db, timeout=60)
g = lambda q,*a: c.execute(q,*([a] if a else [])).fetchone()[0]
MAXB = 4_000_000

def is_net(p): p=p.replace('/','\\'); return p.startswith('\\\\') or p.upper().startswith('UNC\\')
local_ids = [rid for rid,p in c.execute("SELECT id,path FROM roots") if not is_net(p) and os.path.isdir(p)]
ph = ",".join("?"*len(local_ids))

text_total = g("SELECT COUNT(*) FROM files WHERE kind='text' AND size<=?", MAXB)
text_local = g(f"SELECT COUNT(*) FROM files WHERE kind='text' AND size<=? AND root_id IN ({ph})", MAXB, *local_ids)
ftsed_local = g(f"SELECT COUNT(*) FROM files WHERE kind='text' AND size<=? AND ftsed=1 AND root_id IN ({ph})", MAXB, *local_ids)
ftsed_total = g("SELECT COUNT(*) FROM files WHERE kind='text' AND size<=? AND ftsed=1", MAXB)
vec_files = g("SELECT COUNT(DISTINCT file_id) FROM chunks WHERE model='BAAI/bge-m3' AND vec IS NOT NULL")
lexrows = g("SELECT COUNT(*) FROM lex")
allfiles = g("SELECT COUNT(*) FROM files")

print("=== COVERAGE (text files <=4MB) ===")
print(f"all files in db           : {allfiles}")
print(f"text files (<=4MB) total  : {text_total}")
print(f"  lexical(ftsed=1) total  : {ftsed_total}  ({100.0*ftsed_total/text_total:.1f}%)")
print(f"  vector(bge-m3) files    : {vec_files}  ({100.0*vec_files/text_total:.1f}%)")
print(f"lex(FTS5)倒排行           : {lexrows}")
print(f"\n--- LOCAL roots only {local_ids} ---")
print(f"text files local          : {text_local}")
print(f"  lexical(ftsed=1) local  : {ftsed_local}  ({100.0*ftsed_local/text_local:.1f}%)")

# 端到端:抽 5 个「已建倒排(ftsed=1) 但无 bge-m3 向量」的盲区文件,各取一个内容词,验证 lex MATCH 能命中。
print("\n=== END-TO-END: 盲区文件(无向量)现在能否被关键词搜到 ===")
CJK = re.compile(r'[一-鿿]{4,}'); WORD = re.compile(r'[A-Za-z][A-Za-z0-9_]{4,}')
rows = c.execute(f"""
  SELECT f.id, r.path, f.relpath FROM files f JOIN roots r ON r.id=f.root_id
  WHERE f.kind='text' AND f.size BETWEEN 200 AND 200000 AND f.ftsed=1
    AND f.root_id IN ({ph})
    AND f.id NOT IN (SELECT file_id FROM chunks WHERE model='BAAI/bge-m3' AND vec IS NOT NULL)
  ORDER BY f.mtime DESC LIMIT 400""", (*local_ids,)).fetchall()
import random; random.seed(7); random.shuffle(rows)
hits=0; tried=0
for fid, root, rel in rows:
    if tried>=5: break
    try:
        with open(os.path.join(root,rel),'rb') as f: data=f.read(60000)
        if b'\x00' in data[:4096]: continue
        txt=data.decode('utf-8','replace')
    except Exception: continue
    terms = [t for t in (CJK.findall(txt)+WORD.findall(txt)) if 4<=len(t)<=10]
    if not terms: continue
    term = max(set(terms), key=lambda t:(len(t), terms.count(t)))
    tried+=1
    found = c.execute("SELECT EXISTS(SELECT 1 FROM lex WHERE rowid=? AND body MATCH ?)", (fid, f'"{term}"')).fetchone()[0]
    # 也验证全库 MATCH 能召回它(rank 不保证,只验可召回)
    intop = any(r[0]==fid for r in c.execute("SELECT rowid FROM lex WHERE body MATCH ? LIMIT 5000", (f'"{term}"',)))
    hits += 1 if (found and intop) else 0
    print(f"  [{'HIT ' if found and intop else 'MISS'}] fid={fid} term='{term}'  {rel[:60]}")
print(f"\n命中 {hits}/{tried}  (盲区文件靠新倒排可被关键词检索)")
