import os
b = "/root/Polaris/PolarisKB/langmix"
os.makedirs(b, exist_ok=True)
for i in range(300):
    open(os.path.join(b, "doc_en_%d.md" % i), "w").write("This is an English document about retrieval systems and vector indexing. " * 30)
for i in range(200):
    open(os.path.join(b, "mod_%d.py" % i), "w").write("def search(query):\n    return index.lookup(query)\n" * 10)
for i in range(150):
    open(os.path.join(b, "mod_%d.rs" % i), "w").write('fn main() { println!("polaris"); }\n' * 10)
for i in range(100):
    open(os.path.join(b, "data_%d.json" % i), "w").write('{"k":"v","n":1}\n' * 5)
for i in range(80):
    open(os.path.join(b, "conf_%d.toml" % i), "w").write("[section]\nkey = 1\n" * 5)
print("lang-diverse files added: 300 en-md, 200 py, 150 rs, 100 json, 80 toml")
