import sqlite3, os
db = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
c = sqlite3.connect(db, timeout=30)
g = lambda q, *a: c.execute(q, a).fetchone()[0]

# 0) FTS5 能力探测:能否在 Python 的 sqlite3 里建并写 FTS5 表
fts5_ok = False
try:
    c.execute("CREATE VIRTUAL TABLE IF NOT EXISTS temp._fts5probe USING fts5(x)")
    c.execute("INSERT INTO temp._fts5probe(x) VALUES('hello world')")
    n = c.execute("SELECT count(*) FROM temp._fts5probe WHERE _fts5probe MATCH 'hello'").fetchone()[0]
    fts5_ok = (n == 1)
    c.execute("DROP TABLE temp._fts5probe")
except Exception as e:
    fts5_ok = False
    print("FTS5 probe error:", e)
print("FTS5 available in this Python sqlite3 :", fts5_ok)
print("sqlite lib version                    :", sqlite3.sqlite_version)

MAXB = 4_000_000  # 对齐 Rust 常量 MAX_LEX_FILE_BYTES
total = g('SELECT COUNT(*) FROM files')
text_total = g("SELECT COUNT(*) FROM files WHERE kind='text'")
text_small = g("SELECT COUNT(*) FROM files WHERE kind='text' AND size<=?", MAXB)
pending = g("SELECT COUNT(*) FROM files WHERE kind='text' AND size<=? AND ftsed=0", MAXB)
done = g("SELECT COUNT(*) FROM files WHERE kind='text' AND size<=? AND ftsed=1", MAXB)
lexrows = g("SELECT COUNT(*) FROM lex")
print("\n--- scope (kind='text', size<=4MiB) ---")
print("total files in db        :", total)
print("text files total         :", text_total)
print("text files <=4MB         :", text_small)
print("  already ftsed=1        :", done)
print("  pending ftsed=0        :", pending)
print("current lex(FTS5) rows   :", lexrows)

# 类型分布(看 kind 都有啥)
print("\n--- kind distribution ---")
for k, n in c.execute("SELECT kind, COUNT(*) FROM files GROUP BY kind ORDER BY 2 DESC LIMIT 12"):
    print("  %-10s %d" % (k, n))
