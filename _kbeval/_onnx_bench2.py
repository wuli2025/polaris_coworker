# -*- coding: utf-8 -*-
# 干净本地吞吐(无云任务争用):真实长 chunk,threads=14/batch 64,并报单核基线。
import onnxruntime as ort, os, sqlite3, time, numpy as np
from tokenizers import Tokenizer
md = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'models', 'fastembed', 'bge-m3-int8')
DB = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'data', 'fable.db')
c = sqlite3.connect(DB)
texts = [t for (t,) in c.execute("SELECT text FROM chunks WHERE model='BAAI/bge-m3' AND length(text)>200 "
                                 "ORDER BY rowid DESC LIMIT 256")]
avgtok = sum(len(t) for t in texts)//len(texts)
print(f"{len(texts)} chunks, avg {avgtok} chars", flush=True)
tok = Tokenizer.from_file(os.path.join(md,'tokenizer.json'))
tok.enable_truncation(max_length=512); tok.enable_padding(length=None)
def run(threads, batch, n=256):
    so=ort.SessionOptions(); so.intra_op_num_threads=threads
    sess=ort.InferenceSession(os.path.join(md,'model_quantized.onnx'),so,providers=['CPUExecutionProvider'])
    e=tok.encode_batch(texts[:16])
    sess.run(['dense_vecs'],{'input_ids':np.array([x.ids for x in e],dtype=np.int64),'attention_mask':np.array([x.attention_mask for x in e],dtype=np.int64)})
    t0=time.time(); done=0
    for i in range(0,min(n,len(texts)),batch):
        ch=texts[i:i+batch]; e=tok.encode_batch(ch)
        sess.run(['dense_vecs'],{'input_ids':np.array([x.ids for x in e],dtype=np.int64),'attention_mask':np.array([x.attention_mask for x in e],dtype=np.int64)})
        done+=len(ch)
    dt=time.time()-t0; print(f"  threads={threads} batch={batch} -> {done/dt:.1f} chunk/s ({dt:.0f}s for {done})", flush=True)
run(14,64)
run(14,32)
