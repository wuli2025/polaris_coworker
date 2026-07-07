#!/usr/bin/env python3
# 全功能混沌压测:持续检索风暴 + 周期性触发盘点/索引/回填/ontology。
# 验证「高强度下所有功能并发互不踩踏、无锁错、无崩溃」。
import json, time, threading, urllib.request, os, random, sys

PORT = os.environ.get("PORT", "8080")
URL = f"http://127.0.0.1:{PORT}/api/invoke"

def invoke(cmd, args, timeout=120):
    body = json.dumps({"cmd": cmd, "args": args}).encode()
    req = urllib.request.Request(URL, data=body, headers={"Content-Type": "application/json"})
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode()), (time.time()-t0)*1000

QUERIES = ["退款政策","营业时间","知识库检索","公司的退款政策是怎样的","我想了解模型索引是怎么做的",
    "vector index","concurrent architecture","模型 索引","缓存策略","知识图谱 本体抽取",
    "embedding rerank ontology","多核盘点","语义聚类","数据安全 用户画像","performance optimize"]

DUR = int(sys.argv[1]) if len(sys.argv) > 1 else 480
SEARCH_CONC = int(sys.argv[2]) if len(sys.argv) > 2 else 24

stop = time.time() + DUR
stats = {"search_ok":0,"search_err":0,"zero":0,"ops_ok":0,"ops_err":0}
lats = []
errs = []
lock = threading.Lock()

def searcher():
    while time.time() < stop:
        q = random.choice(QUERIES)
        mode = random.choice(["hybrid","grep","hybrid"])
        try:
            res, ms = invoke("fable_search", {"query": q, "topK": 12, "mode": mode})
            with lock:
                stats["search_ok"] += 1; lats.append(ms)
                if not res.get("hits"): stats["zero"] += 1
        except Exception as e:
            with lock:
                stats["search_err"] += 1
                if len(errs) < 8: errs.append("search:"+str(e))

def chaos_ops():
    # periodically fire heavy multi-core ops concurrently with the search storm
    schemas = ["finance","medical","ecommerce","legal","supplychain","general"]
    i = 0
    while time.time() < stop:
        time.sleep(8)
        i += 1
        op = i % 5
        try:
            if op == 0:
                invoke("fable_inventory_start", {"roots":["/root/Polaris/PolarisKB","/root/Polaris/big"],"exclude":None})
            elif op == 1:
                invoke("fable_index_start", {"maxChunks":60000})
            elif op == 2:
                invoke("fable_backfill_lang", {})
            elif op == 3:
                invoke("ontology_seed", {"schemaId": random.choice(schemas)})
            elif op == 4:
                invoke("file_overview", {"root":"/root/Polaris/PolarisKB"})
            with lock: stats["ops_ok"] += 1
        except Exception as e:
            with lock:
                stats["ops_err"] += 1
                if len(errs) < 8: errs.append("op%d:%s" % (op, e))

threads = [threading.Thread(target=searcher) for _ in range(SEARCH_CONC)]
threads.append(threading.Thread(target=chaos_ops))
t0 = time.time()
for t in threads: t.start()
# progress ticks
while time.time() < stop:
    time.sleep(30)
    with lock:
        el = time.time()-t0
        print(json.dumps({"t": round(el), "search_ok": stats["search_ok"], "search_err": stats["search_err"],
            "zero": stats["zero"], "ops_ok": stats["ops_ok"], "ops_err": stats["ops_err"]}), flush=True)
for t in threads: t.join()
lats.sort()
print(json.dumps({"FINAL": True, "wall_s": round(time.time()-t0,1), **stats,
    "search_qps": round(stats["search_ok"]/(time.time()-t0),1),
    "lat_p50": round(lats[len(lats)//2],1) if lats else 0,
    "lat_p99": round(lats[int(len(lats)*0.99)],1) if lats else 0,
    "lat_max": round(lats[-1],1) if lats else 0, "errs": errs}, ensure_ascii=False), flush=True)
