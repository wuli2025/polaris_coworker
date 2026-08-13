/**
 * connect-code-test.mjs —— 连接码编解码的回归测试。
 *
 * 连接码这次多带了三项账号信息(email / uid / hostName),而 hostName 会是中文。
 * `btoa` 只吃 latin1 —— 少了 UTF-8 逃逸这一步,**机器名一带中文整串码就生成不出来**
 * (静默 catch 成空串,用户看到「本机主机就绪中」永远不变)。这个测试就是钉死这条,
 * 外加钉死「老版本的码仍然解得开」。
 *
 * 直接跑真实现:Node 24 能剥 TS 类型,所以 import 的就是前端那份 `parseConnectCode`,
 * 不是抄一份到测试里 —— 抄一份的话,漂了也测不出来。
 *
 * 用法:node scripts/connect-code-test.mjs
 */
import { parseConnectCode } from "../src/features/collab/api.ts";

let bad = 0;
const need = (what, ok, detail = "") => {
  console.log(`  ${ok ? "✓" : "✗"} ${what}${ok ? "" : " —— " + detail}`);
  if (!ok) bad++;
};

/** 与 InterconnectView 的 connectCode 同一套编码(UTF-8 逃逸 → base64url)。 */
function encode(payload) {
  const b64 = btoa(
    encodeURIComponent(JSON.stringify(payload)).replace(/%([0-9A-F]{2})/g, (_, h) =>
      String.fromCharCode(parseInt(h, 16))
    )
  )
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
  return "PLRK1-" + b64;
}

console.log("① 新码:带账号信息 + 中文机器名");
const full = {
  t: "tok_abc123",
  a: ["http://192.168.1.5:8080", "http://100.78.103.101:8080"],
  n: "nodeidnodeidnodeid",
  u: "http://43.139.209.127:8080",
  e: "wuli@example.com",
  i: "acct_Q5dhZBwtb8Iba",
  m: "书房台式机",
};
const code = encode(full);
need("中文机器名不会让编码抛异常", code.startsWith("PLRK1-") && code.length > 20, code.slice(0, 40));
const got = parseConnectCode(code);
need("解得开", !!got, "");
if (got) {
  need("令牌还原", got.token === full.t, got.token);
  need("地址还原", JSON.stringify(got.addrs) === JSON.stringify(full.a), got.addrs);
  need("NodeId 还原", got.nodeId === full.n, got.nodeId);
  need("账号中心还原", got.authority === full.u, got.authority);
  need("邮箱还原(认出「这是谁的设备网」靠它)", got.email === full.e, got.email);
  need("uid 还原(认出「这是我自己的」靠它)", got.uid === full.i, got.uid);
  need("中文机器名原样还原,不是乱码", got.hostName === full.m, got.hostName);
}

console.log("② 老码:没有新字段,必须照样解得开");
const legacy = encode({ t: "tok_old", a: ["http://10.0.0.2:8080"], n: "oldnode" });
const g2 = parseConnectCode(legacy);
need("老码解得开", !!g2, "");
if (g2) {
  need("老码令牌还原", g2.token === "tok_old", g2.token);
  need("老码新字段为 undefined(不编造)", !g2.email && !g2.uid && !g2.hostName, g2);
}

console.log("③ 垃圾输入不能崩");
need("空串 → null", parseConnectCode("") === null, "");
need("邀请码(PLRS1)→ null", parseConnectCode("PLRS1-abc") === null, "");
need("坏 base64 → null", parseConnectCode("PLRK1-!!!!") === null, "");
need("缺 token → null", parseConnectCode(encode({ a: [] })) === null, "");
need("缺地址数组 → null", parseConnectCode(encode({ t: "x" })) === null, "");
need("空白容忍", parseConnectCode(`  ${legacy}  `)?.token === "tok_old", "");

console.log(bad ? `\n${bad} 条未过` : "\nALL PASS");
process.exit(bad ? 1 : 0);
