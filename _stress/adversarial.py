#!/usr/bin/env python3
# ════════════════════════════════════════════════════════════════════════
# Polaris server 抗高压 / 抗崩溃 对抗性压测harness。纯 stdlib。
#
# 与既有 storm.py / chaos.py(都是"合法输入"的吞吐压测)互补：本脚本专打
# 极端/畸形/恶意输入与并发竞态，目标是让进程崩溃(panic 逃逸/栈溢出/OOM/死锁/挂死)。
#
# 安全护栏：只对 ALLOWLIST(只读 or 本地增量、不 spawn 子进程、不改持久配置、
#   不触发付费/网络副作用)里的命令传任意畸形参数。破坏性命令一律不碰。
#
# 用法: python adversarial.py <phase> [args]
#   phases: fuzz | bomb | traversal | unicode | nested | flood | conn | liveness
# ════════════════════════════════════════════════════════════════════════
import json, time, threading, urllib.request, urllib.error, os, sys, socket, random, statistics

PORT = os.environ.get("PORT", "8799")
HOST = "127.0.0.1"
URL = f"http://{HOST}:{PORT}/api/invoke"
HEALTH = f"http://{HOST}:{PORT}/api/health"
HOME = os.path.expanduser("~")
CORPUS = os.path.join(HOME, "Polaris", "_stress_corpus")

# ── 只读 / 本地增量安全命令（可安全地灌任意畸形参数）──────────────────────
ALLOW = [
    # KB 只读
    "kb_root", "kb_default_root", "kb_list", "kb_read", "kb_search", "kb_graph",
    "kb_lint", "kb_pack_list", "kb_overview_get",
    # 扫描/资源（只读）
    "scan_roots", "scan_resources",
    # fable 检索/状态（只读 + 本地）
    "fable_status", "fable_folder_size", "fable_scan_folders",
    "fable_scan_folder_children", "fable_search", "fable_eval_template",
    # 文件中心（只读/本地计算）
    "file_overview", "file_grid", "file_gist", "file_graph", "file_profile_html",
    "file_cluster_model_get", "file_thumb",
    # 专家/团队（只读）
    "expert_list", "expert_get", "expert_groups", "expert_teams",
    "expert_list_by_group", "expert_route_debug", "expert_match_auto", "expert_route",
    "expert_team_get", "expert_avatar_slots", "expert_export", "team_export",
    "expert_agents_status",
    # 杂项只读
    "persona_list", "project_list", "claude_md_list_projects", "claude_md_read",
    "conv_list_projects", "conv_list_conversations", "conv_get_messages",
    "ontology_overview", "ontology_schemas", "ontology_triples",
    "usage_summary", "claude_oauth_status", "codex_status", "echo_status",
    "echo_briefing_today", "updater_get_state", "feishu_get_config",
    "feishu_gateway_status", "media_accounts_status", "list_skills", "get_skill",
    "artifact_list", "artifact_read", "artifact_search", "forge_preflight",
    "sense_list", "voice_config_get", "voice_lexicon_get", "cube_config_get",
    "cube_status", "sandbox_status", "infer_status",
]

lock = threading.Lock()
RESULT = {"calls": 0, "http_ok": 0, "http_err": 0, "exc": 0, "5xx": 0,
          "timeouts": 0, "downtime_events": 0, "max_ms": 0.0}
SAMPLES = []  # noteworthy responses


def alive():
    try:
        with urllib.request.urlopen(HEALTH, timeout=5) as r:
            return r.read().decode().strip() == "ok"
    except Exception:
        return False


def note(tag, msg):
    with lock:
        if len(SAMPLES) < 80:
            SAMPLES.append(f"{tag}: {msg}")


def call(cmd, args, timeout=30):
    body = json.dumps({"cmd": cmd, "args": args}).encode("utf-8", "surrogatepass")
    req = urllib.request.Request(URL, data=body, headers={"Content-Type": "application/json"})
    t0 = time.time()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            r.read()
            ms = (time.time() - t0) * 1000
            with lock:
                RESULT["calls"] += 1; RESULT["http_ok"] += 1
                RESULT["max_ms"] = max(RESULT["max_ms"], ms)
            return r.status, ms
    except urllib.error.HTTPError as e:
        ms = (time.time() - t0) * 1000
        with lock:
            RESULT["calls"] += 1; RESULT["http_err"] += 1
            if e.code >= 500: RESULT["5xx"] += 1
            RESULT["max_ms"] = max(RESULT["max_ms"], ms)
        return e.code, ms  # 500 with JSON error == GOOD (graceful)
    except socket.timeout:
        with lock:
            RESULT["calls"] += 1; RESULT["timeouts"] += 1
        note("TIMEOUT", f"{cmd} (>{timeout}s) — possible hang/deadlock")
        return -1, timeout * 1000
    except Exception as e:
        with lock:
            RESULT["calls"] += 1; RESULT["exc"] += 1
        note("CONN-EXC", f"{cmd}: {e}")
        return -2, (time.time() - t0) * 1000


# 畸形参数生成器：覆盖参数提取器(req_str/opt_usize/opt_bool/opt_f64/vec_str)的所有歧路
def malformed_variants():
    big = "毒" * 200000                      # 200k CJK chars
    return [
        None, {}, [], "not-an-object", 12345, True,
        {"query": None}, {"query": 123}, {"query": []}, {"query": {}},
        {"query": True}, {"query": ""}, {"query": big},
        {"topK": -1}, {"topK": "abc"}, {"topK": 1e18}, {"topK": 999999999999},
        {"topK": -999999999}, {"topK": 0}, {"topK": 1.5},
        {"relPath": "../" * 300 + "etc/passwd"}, {"relPath": ""},
        {"relPath": "C:\\Windows\\System32\\config\\SAM"},
        {"paths": "should-be-array"}, {"paths": [None, 1, True, {}, []]},
        {"paths": [big]}, {"roots": [None, 1, {}]},
        {"root": big}, {"root": None}, {"id": big}, {"subdir": "../../.."},
        {"path": "\\\\?\\C:\\Windows"}, {"max": -5}, {"max": 1e308},
        {"enabled": "yes"}, {"budgetMonthlyCny": "free"},
        {str(i): i for i in range(5000)},          # 5000-key object
        {"nested": {"a": {"b": {"c": list(range(10000))}}}},
        {"query": "退", "topK": 2**31}, {"query": "a" * 100000, "mode": "hybrid"},
        {"query": "\ud800"},                        # lone surrogate
        {"query": "💥" * 50000},                     # emoji bomb
    ]


def phase_fuzz():
    variants = malformed_variants()
    for cmd in ALLOW:
        for v in variants:
            st, ms = call(cmd, v, timeout=20)
            if st in (-1, -2):
                note("FUZZ", f"{cmd} args={str(v)[:60]} -> st={st}")
    print(json.dumps(_snapshot("fuzz"), ensure_ascii=False))


def phase_bomb():
    # 超大 / 超深 / 超多 —— 测无界分配与递归
    bombs = [
        ("fable_search", {"query": "退款" * 5000000, "mode": "grep", "topK": 12}),   # ~30MB query
        ("fable_search", {"query": "x" * 40000000, "mode": "grep"}),                  # 40MB latin
        ("kb_search", {"query": "退" * 3000000}),
        ("kb_read", {"relPath": "z" * 5000000}),
        ("scan_resources", {"roots": [CORPUS] * 5000, "max": 2**40}),
        ("file_overview", {"root": CORPUS, "topK": 2**40}),
        ("fable_search", {"query": "data", "topK": 2**40, "mode": "hybrid"}),
        ("artifact_search", {"query": "x" * 10000000}),
        ("expert_route", {"text": "性能" * 2000000}),
    ]
    for cmd, args in bombs:
        st, ms = call(cmd, args, timeout=60)
        note("BOMB", f"{cmd} -> st={st} {ms:.0f}ms alive={alive()}")
    print(json.dumps(_snapshot("bomb"), ensure_ascii=False))


def phase_nested():
    # 深层嵌套 JSON：测 serde 递归限 / 自有递归栈溢出
    for depth in (50, 120, 200, 500, 2000, 20000):
        s = "{\"a\":" * depth + "1" + "}" * depth
        try:
            body = ("{\"cmd\":\"file_overview\",\"args\":" + s + "}").encode()
            req = urllib.request.Request(URL, data=body, headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req, timeout=20) as r:
                r.read(); code = r.status
        except urllib.error.HTTPError as e:
            code = e.code
        except Exception as e:
            code = str(e)
        note("NESTED", f"depth={depth} -> {code} alive={alive()}")
        with lock: RESULT["calls"] += 1
    # 超大数组
    arr = "[" + ",".join(["1"] * 2000000) + "]"
    try:
        body = ("{\"cmd\":\"scan_resources\",\"args\":{\"roots\":" + arr + "}}").encode()
        req = urllib.request.Request(URL, data=body, headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=30) as r: r.read(); code = r.status
    except urllib.error.HTTPError as e: code = e.code
    except Exception as e: code = str(e)
    note("NESTED", f"2M-array -> {code} alive={alive()}")
    print(json.dumps(_snapshot("nested"), ensure_ascii=False))


def phase_traversal():
    payloads = [
        "../../../../../../etc/passwd", "..\\..\\..\\..\\Windows\\win.ini",
        "/etc/shadow", "C:\\Windows\\System32\\drivers\\etc\\hosts",
        "%2e%2e%2f%2e%2e%2f", "....//....//", "/etc/passwd",
        "\\\\?\\C:\\Windows\\System32\\config\\SAM", "NUL", "CON", "PRN",
        "/proc/self/environ", "file:///etc/passwd", "..%c0%af..%c0%af",
        "~/.ssh/id_rsa", "$HOME/.bashrc", ".‮/dangerous",
        "a" * 5000 + "/passwd",
    ]
    targets = ["kb_read", "kb_delete?SKIP", "file_gist", "artifact_read",
               "claude_md_read", "kb_list", "file_overview", "get_skill"]
    targets = [t for t in targets if "SKIP" not in t]
    for p in payloads:
        for t in targets:
            keymap = {"kb_read": "relPath", "file_gist": "path", "artifact_read": "path",
                      "claude_md_read": "project", "kb_list": "subdir",
                      "file_overview": "root", "get_skill": "id"}
            st, ms = call(t, {keymap[t]: p}, timeout=15)
            # st==200 on a traversal payload may indicate a leak — flag for manual check
            if st == 200 and t in ("kb_read", "file_gist", "artifact_read", "claude_md_read"):
                note("TRAVERSAL-200", f"{t} returned 200 for {p!r} (verify not a leak)")
    # also probe the raw /api/file static endpoint
    for p in ["../../../../etc/passwd", "C:\\Windows\\win.ini", "/proc/self/maps"]:
        try:
            u = f"http://{HOST}:{PORT}/api/file?path=" + urllib.parse.quote(p)
            with urllib.request.urlopen(u, timeout=10) as r:
                data = r.read(200)
                note("FILE-EP", f"{p!r} -> {r.status} ({len(data)}b)")
        except urllib.error.HTTPError as e:
            note("FILE-EP", f"{p!r} -> {e.code}")
        except Exception as e:
            note("FILE-EP", f"{p!r} -> {e}")
    print(json.dumps(_snapshot("traversal"), ensure_ascii=False))


def phase_unicode():
    weird = [
        "‮‭‮", "​‌‍" * 1000, "﻿" * 5000,
        "💥🔥👹" * 10000, "ɐ̲̅ɟ̲̅ʇ̲̅" * 5000, "𒀀𒀁𒀂" * 5000,
        "\\x00\\x01\\x02", "within", "a\rb\nc\td", "<script>alert(1)</script>",
        "'; DROP TABLE chunks; --", "${jndi:ldap://x}", "{{7*7}}",
        "%n%n%n%s%s%s", "\\u" * 5000, "．。｡" * 5000,
        "(a+)+$" + "a" * 5000, "(.*)*" * 100,  # ReDoS-ish
        "退" * 1 + "x" * 1, "ﷺ" * 5000,
    ]
    for w in weird:
        for cmd in ("fable_search", "kb_search", "expert_route", "scan_resources"):
            a = {"query": w} if cmd in ("fable_search", "kb_search") else \
                ({"text": w} if cmd == "expert_route" else {"roots": [w]})
            if cmd == "fable_search": a["mode"] = "grep"
            call(cmd, a, timeout=20)
    print(json.dumps(_snapshot("unicode"), ensure_ascii=False))


def phase_flood(dur=60, conc=40):
    # 高并发：检索风暴 + 周期性触发本地重活(盘点/聚类/总览/回填) + 快速 start/cancel 竞态
    stop = time.time() + dur
    queries = ["退款政策", "营业时间", "知识库检索", "vector index", "模型 索引",
               "缓存策略", "知识图谱 本体抽取", "concurrent architecture", "语义聚类",
               "数据安全 用户画像", "performance optimize", "多核盘点"]

    def searcher():
        while time.time() < stop:
            q = random.choice(queries)
            call("fable_search", {"query": q, "topK": 12, "mode": "grep"}, timeout=30)

    def reader():
        cmds = ["file_overview", "fable_status", "scan_roots", "expert_list",
                "fable_scan_folders", "kb_overview_get", "sense_list", "infer_status"]
        while time.time() < stop:
            call(random.choice(cmds), {"root": CORPUS}, timeout=30)

    def heavy():
        # 本地增量重活 + start/cancel 竞态。盘点对沙箱语料；index 偶尔且小(避免狂刷云嵌入)
        i = 0
        while time.time() < stop:
            i += 1
            op = i % 4
            if op == 0:
                call("fable_inventory_start", {"roots": [CORPUS], "exclude": None}, timeout=30)
            elif op == 1:
                call("file_cluster_build", {"root": CORPUS}, timeout=40)
            elif op == 2:
                call("fable_backfill_lang", {}, timeout=30)
            else:
                call("fable_cancel", {}, timeout=10)   # 竞态：在重活进行中取消
            time.sleep(0.4)

    threads = [threading.Thread(target=searcher) for _ in range(conc)]
    threads += [threading.Thread(target=reader) for _ in range(conc // 2)]
    threads += [threading.Thread(target=heavy) for _ in range(3)]
    # 后台 liveness 看门狗
    downtime = [0]
    def watchdog():
        while time.time() < stop:
            if not alive():
                downtime[0] += 1
                with lock: RESULT["downtime_events"] += 1
                note("DOWN", f"health failed at t={time.time()-(stop-dur):.0f}s")
            time.sleep(2)
    threads.append(threading.Thread(target=watchdog))
    for t in threads: t.start()
    for t in threads: t.join()
    print(json.dumps(_snapshot("flood"), ensure_ascii=False))


def phase_conn(dur=20):
    # 连接层攻击：慢速 body + 大量并发半开连接 + 畸形 HTTP
    results = {"opened": 0, "refused": 0}
    def slowloris():
        try:
            s = socket.create_connection((HOST, int(PORT)), timeout=5)
            s.sendall(b"POST /api/invoke HTTP/1.1\r\nHost: x\r\nContent-Length: 1000000\r\n\r\n")
            for _ in range(5):
                if time.time() > time.time() + dur: break
                s.sendall(b"x"); time.sleep(1)
            results["opened"] += 1
            s.close()
        except Exception:
            results["refused"] += 1
    def junk():
        try:
            s = socket.create_connection((HOST, int(PORT)), timeout=5)
            s.sendall(os.urandom(4096))
            s.close(); results["opened"] += 1
        except Exception:
            results["refused"] += 1
    threads = [threading.Thread(target=slowloris) for _ in range(60)]
    threads += [threading.Thread(target=junk) for _ in range(60)]
    for t in threads: t.start()
    for t in threads: t.join()
    note("CONN", f"opened={results['opened']} refused={results['refused']} alive_after={alive()}")
    print(json.dumps(_snapshot("conn"), ensure_ascii=False))


def phase_servefile(fpath, conc=6):
    # 验证 serve_file (/api/file) 是否无上限地把整文件读进内存：并发拉同一大文件，
    # 由外部监控采样服务端 WorkingSet。客户端按 1MB 分块丢弃，自身内存恒定。
    import urllib.parse
    u = f"http://{HOST}:{PORT}/api/file?path=" + urllib.parse.quote(fpath)

    def get():
        try:
            with urllib.request.urlopen(u, timeout=180) as r:
                n = 0
                while True:
                    b = r.read(1 << 20)
                    if not b:
                        break
                    n += len(b)
                note("SERVEFILE", f"status={r.status} read={n}")
        except Exception as e:
            note("SERVEFILE", f"err {e}")
    ts = [threading.Thread(target=get) for _ in range(conc)]
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    print(json.dumps(_snapshot("servefile"), ensure_ascii=False))


def _snapshot(phase):
    with lock:
        return {"phase": phase, "alive": alive(), **dict(RESULT),
                "samples": SAMPLES[:40]}


if __name__ == "__main__":
    ph = sys.argv[1] if len(sys.argv) > 1 else "fuzz"
    import urllib.parse  # for traversal file endpoint
    if not alive():
        print(json.dumps({"FATAL": "server not alive at start", "url": HEALTH}))
        sys.exit(2)
    {
        "fuzz": phase_fuzz, "bomb": phase_bomb, "nested": phase_nested,
        "traversal": phase_traversal, "unicode": phase_unicode,
        "conn": phase_conn,
        "flood": lambda: phase_flood(int(sys.argv[2]) if len(sys.argv) > 2 else 60,
                                     int(sys.argv[3]) if len(sys.argv) > 3 else 40),
        "servefile": lambda: phase_servefile(sys.argv[2],
                                             int(sys.argv[3]) if len(sys.argv) > 3 else 6),
        "liveness": lambda: print(json.dumps({"alive": alive()})),
    }[ph]()
