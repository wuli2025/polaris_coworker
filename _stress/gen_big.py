import os, random
random.seed(7)
base = "/root/Polaris/big"
words = "退款 政策 检索 索引 向量 模型 缓存 并发 多核 盘点 语义 system data model vector index cache".split()
n = 0
for d in range(200):
    dd = os.path.join(base, "dir%03d" % d)
    os.makedirs(dd, exist_ok=True)
    for f in range(200):
        lines = ["# document %d-%d" % (d, f)]
        for _ in range(random.randint(5, 40)):
            lines.append(" ".join(random.sample(words, random.randint(3, 8))))
        open(os.path.join(dd, "f%03d.md" % f), "w", encoding="utf-8").write("\n".join(lines))
        n += 1
print("generated", n)
