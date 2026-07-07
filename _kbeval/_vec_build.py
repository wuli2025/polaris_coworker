# -*- coding: utf-8 -*-
# 全库向量嵌入 v2(跨文件凑批 + 读/嵌/写三级流水线)。忠实复刻 index.rs 向量管线:
#   chunk_text(1600/200/段落滑窗/≥24留/≤2000) → BGE-M3(硅基,免费) → normalize(L2)
#   → INSERT chunks(file_id,seq,text,dim,vec,model,bits) → files.chunked=1。幂等可续跑。
# 关键改进:不再「一文件一请求」(小文件浪费 batch),而是把多文件的 chunk 拼满 BATCH 再发,
# N 路并发嵌入,单写线程逐文件提交。只跑 local roots/text/<=2MB/ext∉skip/chunked=0。
import sqlite3, os, time, threading, queue
import numpy as np, requests

DB = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
KEY = "sk-ulmygegatdtjdcyxzuscfcdzcogwoirijrcnjbtzsylphzgw"
URL = "https://api.siliconflow.cn/v1/embeddings"
MODEL = "BAAI/bge-m3"
H = {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"}
BATCH = 64                 # 跨文件凑满 64 条 chunk 再发一请求
EMBED_WORKERS = 10         # 并发嵌入路数
TARGET, OVERLAP, MAXC = 1600, 200, 2000
SKIP = ('log', 'csv', 'tsv', 'ndjson')

def chunk_text(s):
    chunks, cur, cur_chars = [], [], 0
    def flush():
        nonlocal cur, cur_chars
        t = ''.join(cur).strip()
        if len(t) >= 24: chunks.append(t)
        cur, cur_chars = [], 0
    for para in s.split("\n\n"):
        plen = len(para)
        if plen > TARGET:
            flush(); start, L = 0, len(para)
            while start < L:
                end = min(start + TARGET, L); chunks.append(para[start:end].strip())
                if end == L: break
                start = max(0, end - OVERLAP)
            continue
        if cur_chars + plen > TARGET: flush()
        if cur: cur.append("\n\n")
        cur.append(para); cur_chars += plen + 2
        if len(chunks) >= MAXC: break
    flush()
    return [c for c in chunks if c][:MAXC]

def embed(texts):
    for attempt in range(5):
        try:
            r = requests.post(URL, headers=H, json={"model": MODEL, "input": texts}, timeout=180)
        except Exception as e:
            if attempt < 4: time.sleep(2 ** attempt); continue
            raise
        if r.status_code == 200:
            return [np.asarray(d["embedding"], dtype=np.float32) for d in r.json()["data"]]
        if r.status_code in (429, 503) and attempt < 4:
            time.sleep(2 ** attempt + 0.5); continue
        raise RuntimeError(f"HTTP {r.status_code}: {r.text[:160]}")
    raise RuntimeError("429 retries exhausted")

def pack_bits(v):
    return np.packbits((v >= 0).astype(np.uint8), bitorder='little').tobytes()

batch_q  = queue.Queue(maxsize=EMBED_WORKERS * 3)   # 待嵌入批: list of (file_id, seq, text)
result_q = queue.Queue(maxsize=EMBED_WORKERS * 6)   # 嵌入结果: list of (file_id, seq, text, vec) | ('FILE0', fid) | None(哨兵)
state = {"files": 0, "chunks": 0, "skip": 0, "err": 0, "t0": time.time(), "total": 0}
lock = threading.Lock()

def embed_worker():
    while True:
        item = batch_q.get()
        if item is None:
            batch_q.task_done(); break
        try:
            vs = embed([t for _, _, t in item])
            out = []
            for (fid, seq, t), v in zip(item, vs):
                n = float(np.linalg.norm(v))
                if n > 1e-12: v /= n
                out.append((fid, seq, t, v))
            result_q.put(out)
        except Exception as e:
            with lock:
                state["err"] += 1
                if state["err"] <= 20: print(f"    [embed-err] {type(e).__name__}: {str(e)[:120]}", flush=True)
            # 该批所属文件不会齐 → 留 chunked=0 下次重试;回报空让计数推进
            result_q.put([("__ERR__", fid) for fid, _, _ in item])
        batch_q.task_done()

def writer_thread(expect):
    # expect[file_id] = 该文件应有的 chunk 数;集齐即写库提交。
    c = sqlite3.connect(DB, timeout=300); c.execute("PRAGMA busy_timeout=300000")
    acc = {}      # file_id -> list[(seq,text,vec)]
    zero_buf = []
    errfiles = set()
    def commit_zero():
        if not zero_buf: return
        c.execute("BEGIN")
        for fid in zero_buf: c.execute("UPDATE files SET chunked=1 WHERE id=?", (fid,))
        c.execute("COMMIT"); zero_buf.clear()
    while True:
        item = result_q.get()
        if item is None:
            result_q.task_done(); break
        # 0-chunk 文件标记
        if isinstance(item, tuple) and item[0] == 'FILE0':
            zero_buf.append(item[1])
            if len(zero_buf) >= 200: commit_zero()
            result_q.task_done(); continue
        for rec in item:
            if rec[0] == "__ERR__":
                errfiles.add(rec[1]); continue
            fid, seq, t, v = rec
            acc.setdefault(fid, []).append((seq, t, v))
            if len(acc[fid]) == expect.get(fid, -1):
                rows = sorted(acc.pop(fid))
                c.execute("BEGIN")
                c.execute("DELETE FROM chunks WHERE file_id=?", (fid,))
                for seq, t, v in rows:
                    c.execute("INSERT OR REPLACE INTO chunks(file_id,seq,text,dim,vec,model,bits) "
                              "VALUES(?,?,?,?,?,?,?)",
                              (fid, seq, t, int(v.shape[0]), v.tobytes(), MODEL, pack_bits(v)))
                c.execute("UPDATE files SET chunked=1 WHERE id=?", (fid,))
                c.execute("COMMIT")
                with lock:
                    state["files"] += 1; state["chunks"] += len(rows)
                    if state["files"] % 200 == 0:
                        dt = time.time()-state["t0"]; cr = state["chunks"]/dt if dt else 0
                        fr = state["files"]/dt if dt else 0; tot = state["total"]
                        eta = (tot-state["files"])/fr if fr else 0
                        print(f"    files {state['files']}/{tot} ({100.0*state['files']/tot:.1f}%)  "
                              f"chunks={state['chunks']}  {cr:.0f} ch/s {fr:.1f} f/s  "
                              f"err={state['err']}  elapsed={dt/60:.1f}m eta={eta/60:.1f}m", flush=True)
        result_q.task_done()
    commit_zero(); c.close()

def main():
    c = sqlite3.connect(DB, timeout=120)
    def is_net(p): p=p.replace('/','\\'); return p.startswith('\\\\') or p.upper().startswith('UNC\\')
    local = [rid for rid,p in c.execute("SELECT id,path FROM roots") if not is_net(p) and os.path.isdir(p)]
    ph=",".join("?"*len(local)); skp=",".join("?"*len(SKIP))
    print("[1] querying vector-pending local files ...", flush=True)
    files = c.execute(
        f"SELECT f.id, r.path, f.relpath FROM files f JOIN roots r ON r.id=f.root_id "
        f"WHERE f.kind='text' AND f.size<=2000000 AND f.chunked=0 "
        f"AND lower(f.ext) NOT IN ({skp}) AND f.root_id IN ({ph}) "
        f"ORDER BY f.size ASC", (*SKIP, *local)).fetchall()
    c.close()
    lim = int(os.environ.get("POLARIS_VEC_LIMIT", "0"))
    if lim > 0: files = files[:lim]; print(f"    [TEST] limit {lim}", flush=True)
    state["total"] = len(files)
    print(f"    {len(files)} files. BATCH={BATCH} EMBED_WORKERS={EMBED_WORKERS}", flush=True)
    if not files: print("nothing to do."); return

    expect = {}
    workers = [threading.Thread(target=embed_worker, daemon=True) for _ in range(EMBED_WORKERS)]
    for w in workers: w.start()
    wr = threading.Thread(target=writer_thread, args=(expect,), daemon=True); wr.start()

    # 生产者:读盘+分块,跨文件凑批。读盘用小线程池重叠 I/O。
    from concurrent.futures import ThreadPoolExecutor
    def read_chunks(args):
        fid, root, rel = args
        try:
            with open(os.path.join(root, rel), 'rb') as f: data = f.read(2_000_001)
            text = "" if b'\x00' in data[:4096] else data.decode('utf-8','replace')
        except Exception: text = ""
        return fid, (chunk_text(text) if text else [])
    pending = []   # 跨文件 chunk 缓冲
    rp = ThreadPoolExecutor(max_workers=12)
    for fid, chs in rp.map(read_chunks, files):
        if not chs:
            with lock: state["skip"] += 1
            result_q.put(('FILE0', fid)); continue
        expect[fid] = len(chs)
        for seq, t in enumerate(chs):
            pending.append((fid, seq, t))
            if len(pending) >= BATCH:
                batch_q.put(pending); pending = []
    if pending: batch_q.put(pending)
    rp.shutdown()
    # 收尾:停嵌入 worker → 等结果排空 → 停 writer
    for _ in workers: batch_q.put(None)
    for w in workers: w.join()
    result_q.join()
    result_q.put(None); wr.join()
    dt = time.time()-state["t0"]
    print(f"\n[DONE-VEC] files={state['files']} chunks={state['chunks']} skip={state['skip']} "
          f"err={state['err']} time={dt/60:.1f}m", flush=True)

if __name__ == '__main__':
    main()
