#!/usr/bin/env python3
# 容器内并发检索风暴 + recall 评测。纯 stdlib(urllib),命中 localhost:8080。
import json, time, threading, urllib.request, os, sys, statistics, random

PORT = os.environ.get("PORT", "8080")
URL = f"http://127.0.0.1:{PORT}/api/invoke"
BASE = "/root/Polaris/PolarisKB"

def invoke(cmd, args, timeout=60):
    body = json.dumps({"cmd": cmd, "args": args}).encode()
    req = urllib.request.Request(URL, data=body, headers={"Content-Type": "application/json"})
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as r:
        data = json.loads(r.read().decode())
    return data, (time.time()-t0)*1000

def search(q, mode="hybrid", topk=12):
    return invoke("fable_search", {"query": q, "topK": topk, "mode": mode})

# ---- recall 评测:地面真值里每条 query 应在 top-k 命中其 path ----
def eval_recall(topk=12):
    truth = json.load(open(os.path.join(BASE, "_ground_truth.json"), encoding="utf-8"))
    by_kind = {}
    lat = []
    for t in truth:
        try:
            res, ms = search(t["q"], "hybrid", topk)
        except Exception as e:
            res = {"hits": []}; ms = 0
        lat.append(ms)
        paths = [h.get("path","").replace("\\","/") for h in res.get("hits", [])]
        want = t["path"]
        hit_rank = next((i for i,p in enumerate(paths) if p.endswith(want) or want.endswith(p)), -1)
        k = t["kind"]
        d = by_kind.setdefault(k, {"n":0, "recall":0, "mrr":0.0})
        d["n"] += 1
        if hit_rank >= 0:
            d["recall"] += 1
            d["mrr"] += 1.0/(hit_rank+1)
    out = {}
    for k,d in by_kind.items():
        out[k] = {"n": d["n"], "recall@k": round(d["recall"]/d["n"],3), "mrr": round(d["mrr"]/d["n"],3)}
    out["_latency_ms"] = {"p50": round(statistics.median(lat),1), "p99": round(sorted(lat)[int(len(lat)*0.99)],1) if lat else 0, "max": round(max(lat),1) if lat else 0}
    return out

# ---- 并发检索风暴 ----
QUERIES = [
    ("退款政策","cjk2"), ("营业时间","cjk2"), ("知识库检索","cjk-long"),
    ("公司的退款政策是怎样的","cjk-nl"), ("我想了解模型索引是怎么做的","cjk-nl"),
    ("vector index","latin"), ("concurrent architecture","latin"),
    ("模型 索引","cjk-multi"), ("缓存策略","cjk2"), ("知识图谱 本体抽取","cjk-multi"),
    ("embedding rerank ontology","latin"), ("多核盘点","cjk2"),
    ("语义聚类","cjk2"), ("数据安全 用户画像","cjk-multi"), ("performance optimize","latin"),
]

def storm(duration_s, concurrency):
    stop = time.time() + duration_s
    counts = {"ok":0, "err":0, "zero":0}
    lats = []
    lock = threading.Lock()
    errs = []
    def worker():
        while time.time() < stop:
            q, _ = random.choice(QUERIES)
            mode = random.choice(["hybrid","grep","hybrid"])
            try:
                res, ms = search(q, mode)
                with lock:
                    counts["ok"] += 1
                    lats.append(ms)
                    if not res.get("hits"): counts["zero"] += 1
            except Exception as e:
                with lock:
                    counts["err"] += 1
                    if len(errs) < 5: errs.append(str(e))
    threads = [threading.Thread(target=worker) for _ in range(concurrency)]
    t0 = time.time()
    for t in threads: t.start()
    for t in threads: t.join()
    wall = time.time()-t0
    lats.sort()
    res = {
        "concurrency": concurrency, "wall_s": round(wall,1),
        "requests": counts["ok"]+counts["err"], "ok": counts["ok"], "err": counts["err"],
        "zero_hits": counts["zero"],
        "qps": round((counts["ok"]+counts["err"])/wall,1),
        "lat_p50": round(lats[len(lats)//2],1) if lats else 0,
        "lat_p95": round(lats[int(len(lats)*0.95)],1) if lats else 0,
        "lat_p99": round(lats[int(len(lats)*0.99)],1) if lats else 0,
        "lat_max": round(lats[-1],1) if lats else 0,
        "errs": errs,
    }
    return res

if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "recall"
    if mode == "recall":
        print(json.dumps(eval_recall(), ensure_ascii=False))
    elif mode == "storm":
        dur = int(sys.argv[2]) if len(sys.argv) > 2 else 30
        conc = int(sys.argv[3]) if len(sys.argv) > 3 else 16
        print(json.dumps(storm(dur, conc), ensure_ascii=False))
