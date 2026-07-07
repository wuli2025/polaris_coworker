# -*- coding: utf-8 -*-
# 词法专扫腿 · NAS 网络盘版(只扫可达的网络 root;走 Tailscale SMB,慢但零成本可续跑)。
# 与本地版同语义,但:① 只选网络路径 root;② 每文件读超时更长(15s);③ 读线程更少(网络);
# ④ 批量更小(200)更频繁提交,卡了少丢。幂等:ftsed=0 才处理。
import sqlite3, os, time
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FTimeout

DB = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
MAXB = 4_000_000
BATCH = 200
READERS = 12
READ_TIMEOUT = 15
LOG_EVERY = 2000
CKPT_EVERY_BATCHES = 50

def is_net_reachable(path):
    p = path.replace('/', '\\')
    if not (p.startswith('\\\\')):          # 只要真·UNC 网络路径(失联的 "UNC\..." 旧格式跳过)
        return False
    try:
        return os.path.isdir(path)
    except Exception:
        return False

def read_body(args):
    file_id, root, rel = args
    try:
        with open(os.path.join(root, rel), 'rb') as f:
            data = f.read(MAXB + 1)
        if b'\x00' in data[:4096]:
            return (file_id, None)
        return (file_id, data.decode('utf-8', 'replace'))
    except Exception:
        return (file_id, None)

def main():
    t0 = time.time()
    c = sqlite3.connect(DB, timeout=180)
    c.execute("PRAGMA busy_timeout=180000")
    roots = c.execute("SELECT id, path FROM roots ORDER BY id").fetchall()
    net_ids = [rid for rid, path in roots if is_net_reachable(path)]
    print("[0] reachable network roots:", net_ids, flush=True)
    if not net_ids:
        print("no reachable network roots, nothing to do."); return
    ph = ",".join("?" * len(net_ids))
    rows = c.execute(
        f"SELECT f.id, r.path, f.relpath FROM files f JOIN roots r ON r.id=f.root_id "
        f"WHERE f.kind='text' AND f.size<=? AND f.ftsed=0 AND f.root_id IN ({ph}) "
        f"ORDER BY f.mtime DESC", (MAXB, *net_ids)).fetchall()
    total = len(rows)
    print(f"[1] {total} NAS files pending (BATCH={BATCH}, READERS={READERS})", flush=True)
    if total == 0:
        print("nothing to do."); return
    done = bodied = timeouts = 0; nb = 0
    pool = ThreadPoolExecutor(max_workers=READERS)
    for i in range(0, total, BATCH):
        chunk = rows[i:i+BATCH]
        futs = [pool.submit(read_body, a) for a in chunk]
        results = []
        for a, fut in zip(chunk, futs):
            try: results.append(fut.result(timeout=READ_TIMEOUT))
            except FTimeout: timeouts += 1; results.append((a[0], None))
        c.execute("BEGIN")
        for file_id, body in results:
            c.execute("DELETE FROM lex WHERE rowid=?", (file_id,))
            if body:
                c.execute("INSERT INTO lex(rowid, body) VALUES(?, ?)", (file_id, body)); bodied += 1
            c.execute("UPDATE files SET ftsed=1 WHERE id=?", (file_id,))
        c.execute("COMMIT")
        done += len(chunk); nb += 1
        if nb % CKPT_EVERY_BATCHES == 0:
            try: c.execute("PRAGMA wal_checkpoint(TRUNCATE)")
            except Exception: pass
        if done % LOG_EVERY < BATCH or done == total:
            dt = time.time()-t0; rate = done/dt if dt else 0; eta=(total-done)/rate if rate else 0
            print(f"    {done}/{total} ({100.0*done/total:.1f}%)  {rate:.0f} f/s  "
                  f"bodied={bodied} timeouts={timeouts}  elapsed={dt/60:.1f}m eta={eta/60:.1f}m", flush=True)
    pool.shutdown(wait=False)
    try: c.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    except Exception: pass
    print(f"\n[DONE-NAS] processed={done} bodied={bodied} timeouts={timeouts} time={(time.time()-t0)/60:.1f}m", flush=True)

if __name__ == '__main__':
    main()
