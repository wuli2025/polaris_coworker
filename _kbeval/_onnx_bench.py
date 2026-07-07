# -*- coding: utf-8 -*-
# 本地 ONNX 嵌入吞吐基准:用真实 chunk 文本,试不同 intra_op 线程 / batch,报 chunk/s。
import onnxruntime as ort, os, sqlite3, time, numpy as np
from tokenizers import Tokenizer
md = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'models', 'fastembed', 'bge-m3-int8')
DB = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
c = sqlite3.connect(DB)
texts = [t for (t,) in c.execute("SELECT text FROM chunks WHERE model='BAAI/bge-m3' AND length(text)>200 "
                                 "ORDER BY rowid DESC LIMIT 512")]
print(f"loaded {len(texts)} real chunk texts (avg len {sum(len(t) for t in texts)//len(texts)} chars)")
tok = Tokenizer.from_file(os.path.join(md,'tokenizer.json'))
tok.enable_truncation(max_length=512); tok.enable_padding(length=None)

def run(threads, batch):
    so = ort.SessionOptions(); so.intra_op_num_threads = threads
    sess = ort.InferenceSession(os.path.join(md,'model_quantized.onnx'), so, providers=['CPUExecutionProvider'])
    # warmup
    e = tok.encode_batch(texts[:16])
    sess.run(['dense_vecs'], {'input_ids':np.array([x.ids for x in e],dtype=np.int64),
                              'attention_mask':np.array([x.attention_mask for x in e],dtype=np.int64)})
    t0=time.time(); n=0
    for i in range(0, len(texts), batch):
        chunk = texts[i:i+batch]
        e = tok.encode_batch(chunk)
        ids = np.array([x.ids for x in e],dtype=np.int64); mask=np.array([x.attention_mask for x in e],dtype=np.int64)
        sess.run(['dense_vecs'], {'input_ids':ids,'attention_mask':mask})
        n += len(chunk)
    dt=time.time()-t0
    print(f"  threads={threads:>2} batch={batch:>3} -> {n/dt:6.1f} chunk/s  ({dt:.1f}s)")

for th, bs in [(14,32),(14,64),(8,32),(8,64)]:
    run(th, bs)
