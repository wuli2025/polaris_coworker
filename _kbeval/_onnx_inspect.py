import onnxruntime as ort, os, numpy as np
from tokenizers import Tokenizer
md = os.path.join(os.environ['USERPROFILE'], 'Polaris', 'models', 'fastembed', 'bge-m3-int8')
so = ort.SessionOptions(); so.intra_op_num_threads = 4
sess = ort.InferenceSession(os.path.join(md, 'model_quantized.onnx'), so, providers=['CPUExecutionProvider'])
print("INPUTS:")
for i in sess.get_inputs(): print("  ", i.name, i.shape, i.type)
print("OUTPUTS:")
for o in sess.get_outputs(): print("  ", o.name, o.shape, o.type)

tok = Tokenizer.from_file(os.path.join(md, 'tokenizer.json'))
tok.enable_truncation(max_length=512)
tok.enable_padding(length=None)
enc = tok.encode_batch(["北极星混合检索把关键词与向量两腿并行", "hello world retrieval test"])
ids = np.array([e.ids for e in enc], dtype=np.int64)
mask = np.array([e.attention_mask for e in enc], dtype=np.int64)
print("ids shape", ids.shape)
feed = {}
names = {i.name for i in sess.get_inputs()}
if 'input_ids' in names: feed['input_ids'] = ids
if 'attention_mask' in names: feed['attention_mask'] = mask
if 'token_type_ids' in names: feed['token_type_ids'] = np.zeros_like(ids)
outs = sess.run(None, feed)
for o, arr in zip(sess.get_outputs(), outs):
    a = np.asarray(arr)
    print(f"  out {o.name}: shape={a.shape} dtype={a.dtype}")
