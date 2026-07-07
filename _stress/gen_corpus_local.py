#!/usr/bin/env python3
# 本地沙箱语料：在 ~/Polaris/_stress_corpus 下造一堆中英混合 + 多语言文件，
# 给盘点/聚类/检索的并发洪流当靶子。纯 stdlib。零网络。
import os, random
random.seed(20260618)
home = os.path.expanduser("~")
base = os.path.join(home, "Polaris", "_stress_corpus")
os.makedirs(base, exist_ok=True)

cn = "退款 政策 检索 索引 向量 模型 缓存 并发 多核 盘点 语义 聚类 重排 嵌入 倒排 图谱 本体 抽取 安全 用户 架构 性能 优化 部署 容器 服务 接口".split()
en = "system data model search index vector policy refund hours security user architecture cache concurrent retrieval embedding rerank ontology knowledge graph performance optimize deploy".split()

n = 0
for d in range(40):
    dd = os.path.join(base, "dir%02d" % d)
    os.makedirs(dd, exist_ok=True)
    for f in range(15):
        ext = random.choice([".md", ".md", ".txt", ".py", ".rs", ".json", ".log"])
        lines = ["# doc %d-%d %s" % (d, f, random.choice(cn))]
        for _ in range(random.randint(8, 60)):
            if random.random() < 0.5:
                lines.append("本文讨论" + "".join(random.sample(cn, random.randint(3, 8))) + "等问题。")
            else:
                lines.append(" ".join(random.sample(en, random.randint(4, 10))))
        open(os.path.join(dd, "f%03d%s" % (f, ext)), "w", encoding="utf-8").write("\n".join(lines))
        n += 1
print("corpus files:", n, "at", base)
