import sqlite3, os, time
db = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
c = sqlite3.connect(db, timeout=30)
print("--- roots: id | reachable | pending(ftsed=0,text,<=4MB) | path ---")
rows = c.execute("SELECT id, path FROM roots ORDER BY id").fetchall()
for rid, path in rows:
    pend = c.execute("SELECT COUNT(*) FROM files WHERE root_id=? AND kind='text' AND size<=4000000 AND ftsed=0", (rid,)).fetchone()[0]
    t0 = time.time()
    try:
        ok = os.path.isdir(path)
    except Exception:
        ok = False
    dt = (time.time()-t0)*1000
    print(f"  {rid:>3}  {'OK ' if ok else 'DEAD'}  pend={pend:>8}  probe={dt:.0f}ms  {path}")
