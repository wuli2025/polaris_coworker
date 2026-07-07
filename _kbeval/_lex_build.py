# -*- coding: utf-8 -*-
# 词法专扫腿 v2(忠实复刻 build_lexical_index;只扫可达的本地盘,NAS/失联 root 跳过)。
#   写 FTS5 倒排、零网络、零嵌入。每文件读加超时兜底(防个别锁定文件卡死整批);
#   定期 wal_checkpoint(TRUNCATE) 防 WAL 膨胀;批量事务不长持写锁,与运行中的 app 共存。
import sqlite3, os, sys, time
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FTimeout

DB = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
MAXB = 4_000_000
BATCH = 500
READERS = 24
READ_TIMEOUT = 8        # 单文件读超时(s):本地盘几乎不会触发,只防个别被独占锁的文件
LOG_EVERY = 5000
CKPT_EVERY_BATCHES = 40 # 每 40 批(~2万文件)截断一次 WAL

# 只扫:目录可达 且 非网络路径(不以 \\ 开头、不以 UNC 开头)。NAS/失联留待单独处理。
def is_local_reachable(path):
    p = path.replace('/', '\\')
    if p.startswith('\\\\') or p.upper().startswith('UNC\\'):
        return False
    try:
        return os.path.isdir(path)
    except Exception:
        return False

def read_body(args):
    file_id, root, rel = args
    abs_path = os.path.join(root, rel)
    try:
        with open(abs_path, 'rb') as f:
            data = f.read(MAXB + 1)
        if b'\x00' in data[:4096]:
            return (file_id, None)
        return (file_id, data.decode('utf-8', 'replace'))
    except Exception:
        return (file_id, None)

def main():
    t0 = time.time()
    c = sqlite3.connect(DB, timeout=120)
    c.execute("PRAGMA busy_timeout=120000")

    roots = c.execute("SELECT id, path FROM roots ORDER BY id").fetchall()
    local_ids = [rid for rid, path in roots if is_local_reachable(path)]
    print("[0] local reachable roots:", local_ids, flush=True)

    ph = ",".join("?" * len(local_ids))
    print("[1] querying pending text files (local roots, ftsed=0) ...", flush=True)
    rows = c.execute(
        f"SELECT f.id, r.path, f.relpath "
        f"FROM files f JOIN roots r ON r.id=f.root_id "
        f"WHERE f.kind='text' AND f.size<=? AND f.ftsed=0 AND f.root_id IN ({ph}) "
        f"ORDER BY f.mtime DESC", (MAXB, *local_ids)).fetchall()
    total = len(rows)
    print(f"    {total} local files pending (BATCH={BATCH}, READERS={READERS})", flush=True)
    if total == 0:
        print("nothing to do."); return

    done = bodied = timeouts = 0
    pool = ThreadPoolExecutor(max_workers=READERS)
    nb = 0
    for i in range(0, total, BATCH):
        chunk = rows[i:i+BATCH]
        futs = [pool.submit(read_body, a) for a in chunk]
        results = []
        for a, fut in zip(chunk, futs):
            try:
                results.append(fut.result(timeout=READ_TIMEOUT))
            except FTimeout:
                timeouts += 1
                results.append((a[0], None))   # 超时:正文置空,仍标完成(同 Rust 对 Err 的处理)
        c.execute("BEGIN")
        for file_id, body in results:
            c.execute("DELETE FROM lex WHERE rowid=?", (file_id,))
            if body:
                c.execute("INSERT INTO lex(rowid, body) VALUES(?, ?)", (file_id, body))
                bodied += 1
            c.execute("UPDATE files SET ftsed=1 WHERE id=?", (file_id,))
        c.execute("COMMIT")
        done += len(chunk); nb += 1
        if nb % CKPT_EVERY_BATCHES == 0:
            try: c.execute("PRAGMA wal_checkpoint(TRUNCATE)")
            except Exception: pass
        if done % LOG_EVERY < BATCH or done == total:
            dt = time.time() - t0; rate = done/dt if dt else 0
            eta = (total-done)/rate if rate else 0
            print(f"    {done}/{total} ({100.0*done/total:.1f}%)  {rate:.0f} f/s  "
                  f"bodied={bodied} timeouts={timeouts}  elapsed={dt/60:.1f}m eta={eta/60:.1f}m", flush=True)
    pool.shutdown(wait=False)

    try: c.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    except Exception: pass
    pend_local = c.execute(
        f"SELECT COUNT(*) FROM files WHERE kind='text' AND size<=? AND ftsed=0 AND root_id IN ({ph})",
        (MAXB, *local_ids)).fetchone()[0]
    lexrows = c.execute("SELECT COUNT(*) FROM lex").fetchone()[0]
    c.close()
    print(f"\n[DONE] processed={done} bodied={bodied} timeouts={timeouts} "
          f"local_pending_left={pend_local} lex_rows={lexrows} time={(time.time()-t0)/60:.1f}m", flush=True)

if __name__ == '__main__':
    main()
