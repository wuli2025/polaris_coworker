# -*- coding: utf-8 -*-
# 嵌入吞吐基准:量化「批量可调(EMBED_BATCH 16→32)减网络往返」对云嵌入吞吐的真实提升。
# 对硅基流动 BGE-M3 真打,分别用 batch=16/32/64,串行(隔离批量效果)+ 3 路并发(贴生产)各测。
import os, time, math, statistics, requests, concurrent.futures as cf

KEY = os.environ.get("POLARIS_EVAL_KEY", "sk-ulmygegatdtjdcyxzuscfcdzcogwoirijrcnjbtzsylphzgw")
URL = "https://api.siliconflow.cn/v1/embeddings"
H = {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"}
N = 192  # 总样本 chunk 数
# 造一批长度像样的中文 chunk(贴近真实知识库文本)
BASE = ("北极星知识库的混合检索把关键词倒排与向量语义两条腿并行编排，"
        "再用 RRF 融合与重排专家取证。本段用于嵌入吞吐基准测试，第 {} 条。")
DOCS = [BASE.format(i) * 3 for i in range(N)]

def embed_batch(texts):
    r = requests.post(URL, headers=H, json={"model": "BAAI/bge-m3", "input": texts}, timeout=120)
    if r.status_code != 200:
        raise RuntimeError(f"HTTP {r.status_code}: {r.text[:160]}")
    return len(r.json()["data"])

def run(batch, concurrency):
    groups = [DOCS[i:i+batch] for i in range(0, N, batch)]
    t0 = time.time()
    if concurrency <= 1:
        for g in groups:
            embed_batch(g)
    else:
        with cf.ThreadPoolExecutor(max_workers=concurrency) as ex:
            list(ex.map(embed_batch, groups))
    dt = time.time() - t0
    return dt, len(groups), N / dt

print(f"样本 {N} chunk · 硅基 BGE-M3 真打\n")
print(f"{'batch':>6} {'并发':>4} {'请求数':>6} {'耗时s':>8} {'吞吐/s':>8}")
print("-" * 40)
results = {}
for conc in (1, 3):
    for batch in (16, 32, 64):
        # 取 2 次取较优,抹平网络抖动
        best = None
        for _ in range(2):
            try:
                dt, nreq, tput = run(batch, conc)
            except Exception as e:
                print(f"{batch:>6} {conc:>4}  FAIL {e}"); dt, nreq, tput = float('inf'), 0, 0; break
            if best is None or tput > best[2]:
                best = (dt, nreq, tput)
            time.sleep(0.5)
        results[(conc, batch)] = best[2]
        print(f"{batch:>6} {conc:>4} {best[1]:>6} {best[0]:>8.2f} {best[2]:>8.1f}")
    print()

# 结论:并发=3(贴生产)下 batch16→32 的提升
b16 = results.get((3, 16), 0); b32 = results.get((3, 32), 0); b64 = results.get((3, 64), 0)
if b16 > 0:
    print(f">>> 生产口径(并发3): batch 16→32 吞吐 {b16:.1f}→{b32:.1f}/s "
          f"(+{(b32/b16-1)*100:.0f}%);→64 = {b64:.1f}/s (+{(b64/b16-1)*100:.0f}%)")
