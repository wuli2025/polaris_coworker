import { defineStore } from "pinia";
import { ref, computed } from "vue";
import {
  chat as chatApi,
  convApi,
  listen,
  type ChatStreamEvent,
  type AttachedFile,
  type PermissionMode,
} from "../tauri";
import { useAppStore } from "./app";
import { parseArtifacts, receiveArtifact, previewDeliverable } from "./chatArtifacts";
export { parseArtifacts } from "./chatArtifacts";
import { expertRequestFields } from "../lib/expertSelection";
import { useSessionsStore } from "../features/coworker/stores/sessions";
import { humanizeError } from "../lib/humanizeError";
import type { Bubble, ChatRuntimeOptions } from "./chatTypes";
export type { Bubble } from "./chatTypes";
import { createDeltaBuffer } from "./chatDeltas";
import { uploadedImagePaths } from "../lib/chatImages";

/**
 * 对话运行时 store —— 多开的核心。
 *
 * 每个对话各自维护 bubbles / sending / reqId；流式事件在 app 级监听一次，
 * 按 `conversationId` 路由进各自缓冲。这样切到任意对话都能看到它的实时进度，
 * 多个任务可同时在后台流式推进（互不干扰），切走也不会"停"。
 */
export const useChatStore = defineStore("chatRuntime", () => {
  const byConv = ref<Record<string, Bubble[]>>({});
  const reqByConv = ref<Record<string, string>>({});
  const sendingByConv = ref<Record<string, boolean>>({});
  const loadedByConv = ref<Record<string, boolean>>({});
  // 本地「最近活跃时间」：发送/结束时打点。后端 updatedAt 在会话内不变，
  // 用它让刚交互过的对话在侧栏冒泡到最上（仿 Codex 最近对话置顶）。
  const activeAtByConv = ref<Record<string, number>>({});
  // 最近一轮注入的估算 input token（后端 meta 事件给出）。分批编排据此自适应批量。
  const tokensByConv = ref<Record<string, number>>({});
  // 等待某对话「本轮 done」的 resolver 队列（分批编排循环逐轮 await）。
  const doneWaiters: Record<string, Array<() => void>> = {};
  // 每对话一个「无声死亡」看门狗(setInterval id)。后端子进程崩溃 / done 事件丢失时,
  // done/cancel/fail 都不会触发 → await waitForDone 永久挂起 → 该对话进度条卡死。
  const watchdogs: Record<string, number> = {};
  // 持续静默(无任何流式心跳)超过此阈值即判本轮异常终止并熔断。设得足够宽,避免误杀
  // 「单个长工具(如几分钟的 ffmpeg 渲染)期间无 stdout」这类合法静默。
  const SILENCE_LIMIT_MS = 300_000; // 5 分钟完全无心跳
  // 流式监听的「就绪 promise」。缓存它(而非一个 started 布尔), 让所有调用方 await 的是
  // 「监听器真正挂上」这一刻 —— 而不是仅仅把标志位置真。否则首条消息的 delta 可能在
  // listen() 完成注册之前就到达 → 丢帧(现象: 第一次发消息看不到流式输出, 但后台照常运行)。
  let initPromise: Promise<void> | null = null;

  /** 等到指定对话「本轮跑完(done)」。当前不在发送态则立即兑现。
   *  挂起期间启动心跳看门狗:后端无声死亡时熔断,确保 await 不会永久挂起。 */
  function waitForDone(convId: string): Promise<void> {
    if (!sendingByConv.value[convId]) return Promise.resolve();
    armWatchdog(convId);
    return new Promise<void>((resolve) => {
      (doneWaiters[convId] ??= []).push(resolve);
    });
  }
  /** 唤醒并清空某对话的 done 等待队列。done / cancel / 发送失败都必须调用,
   *  否则正在 await waitForDone 的分批编排循环会永久挂起(进度条卡死)。 */
  function wakeWaiters(convId: string) {
    stopWatchdog(convId);
    const waiters = doneWaiters[convId];
    // 删键(而非仅清空数组):清空只回收数组元素,键本身仍永久残留 —— 后端崩溃/done 丢失时
    // 这些键会逐月累积成上百条死条目。先 delete 再调用 resolver,顺带防 resolver 内重入。
    if (waiters && waiters.length) {
      delete doneWaiters[convId];
      for (const w of waiters) w();
    } else {
      delete doneWaiters[convId];
    }
  }
  /** 启动某对话的无声死亡看门狗(幂等:已存在则不重复挂)。每 15s 巡检一次,
   *  仍在发送态且持续静默超阈值 → **先问后端这轮是否还在真实运行**:
   *  在跑 → 继续等(长工具零输出静默是常态;后端看门狗有进程树深检,判死交给它——
   *  前端定长超时单方面放弃会造成同会话重叠轮次:旧轮还在产出,分批编排已把下一批
   *  发进同一常驻进程);确认不在了(或查询本身失败=后端死透)才熔断收尾。 */
  function armWatchdog(convId: string) {
    if (watchdogs[convId]) return;
    let probing = false; // 防上一次 isRunning 未返回时叠加探测
    watchdogs[convId] = window.setInterval(async () => {
      if (!sendingByConv.value[convId]) {
        stopWatchdog(convId);
        return;
      }
      if (Date.now() - activityAt(convId) < SILENCE_LIMIT_MS || probing) return;
      const reqId = reqByConv.value[convId];
      if (!reqId) {
        failSilent(convId); // 连 reqId 都没有:本轮从未真正启动,直接熔断
        return;
      }
      probing = true;
      try {
        const alive = await chatApi.isRunning(reqId);
        // 熔断前必须复核「这轮还是探测发起时那轮」:await 期间旧轮可能正常收尾、
        // 分批编排已接棒发新轮(send 同步删 reqByConv → IPC 落地前有几百 ms 空窗)。
        // 拿旧 reqId 的 false 去熔断会误杀新轮:UI 置终态+再唤醒续批,与仍在跑的
        // 新轮重叠 —— 恰是本探测要消灭的事故。reqId 复核后:旧轮已收尾则条目为空
        // 或已是新值,天然不命中。
        if (
          !alive &&
          sendingByConv.value[convId] &&
          reqByConv.value[convId] === reqId
        )
          failSilent(convId);
      } catch {
        // 查询失败 = 后端进程/桥接已死,按无声死亡熔断(同样先复核 reqId 未换代)。
        if (sendingByConv.value[convId] && reqByConv.value[convId] === reqId)
          failSilent(convId);
      } finally {
        probing = false;
      }
    }, 15_000);
  }
  function stopWatchdog(convId: string) {
    const t = watchdogs[convId];
    if (t) {
      window.clearInterval(t);
      delete watchdogs[convId];
    }
  }
  /** 后端无声死亡的熔断收尾:合成超时气泡 + 置终态 + 唤醒等待者。
   *  镜像 done/cancel/fail 的终态处理,让分批编排循环得以收尾(可重发从断点续跑)。 */
  function failSilent(convId: string) {
    stopWatchdog(convId);
    if (!sendingByConv.value[convId]) return;
    const arr = ensureArr(convId);
    arr.push({
      role: "assistant",
      text: `[本轮超时] ${humanizeError(new Error("后端长时间无响应，已停止该轮；可重发并从断点续跑"))}`,
      at: Date.now(),
      err: true,
      severity: "error",
    });
    sendingByConv.value[convId] = false;
    delete reqByConv.value[convId];
    touchActivity(convId);
    try {
      useSessionsStore().finish(convId);
    } catch {
      /* ignore */
    }
    wakeWaiters(convId);
  }
  function inputTokens(convId: string | null): number {
    if (!convId) return 0;
    return tokensByConv.value[convId] ?? 0;
  }

  function bubblesFor(convId: string | null): Bubble[] {
    if (!convId) return [];
    return byConv.value[convId] ?? [];
  }
  function isSending(convId: string | null): boolean {
    return !!(convId && sendingByConv.value[convId]);
  }
  /** 当前所有「正在生成」的对话 id —— 全局任务中心据此把 AI 的后台生成
   *  (切走仍在跑的 PPT / 长任务等)挂到右下角浮层。 */
  const runningConvIds = computed(() =>
    Object.keys(sendingByConv.value).filter((id) => sendingByConv.value[id]),
  );
  function activityAt(convId: string | null): number {
    if (!convId) return 0;
    return activeAtByConv.value[convId] ?? 0;
  }
  function touchActivity(convId: string) {
    if (!convId) return;
    activeAtByConv.value[convId] = Date.now();
  }
  function ensureArr(convId: string): Bubble[] {
    if (!byConv.value[convId]) byConv.value[convId] = [];
    return byConv.value[convId];
  }
  function pushBubble(convId: string, b: Bubble) {
    ensureArr(convId).push(b);
  }

  // 内存治理:byConv 旧实现按 convId 无上限累积气泡,多开 / 长时间使用后会有几万~几十万
  // 气泡对象常驻(每个还被 Vue reactive Proxy 包一层,Mac WKWebView 回收又懒 → 额外放大),
  // 长期贡献数百 MB ~ GB。改对话级 LRU:只让最近活跃的 MAX_LIVE_CONVS 个对话的气泡留在
  // 内存,其余卸载(消息后端已持久化,切回时 loadHistory 会按需重取)。正在流式发送的对话
  // 恒受保护(必须留住实时气泡),刚被访问 / 收到事件的对话因 activeAt 最新而排在最前、不会被卸。
  const MAX_LIVE_CONVS = 16;
  function evictStaleConversations() {
    const ids = Object.keys(byConv.value);
    if (ids.length <= MAX_LIVE_CONVS) return;
    // 候选 = 非发送中的对话,按最近活跃时间降序(发送中的对话不参与淘汰、永远留住)。
    const ranked = ids
      .filter((id) => !sendingByConv.value[id])
      .sort((a, b) => (activeAtByConv.value[b] ?? 0) - (activeAtByConv.value[a] ?? 0));
    const sendingCount = ids.length - ranked.length;
    const keep = Math.max(0, MAX_LIVE_CONVS - sendingCount);
    for (const id of ranked.slice(keep)) {
      delete byConv.value[id]; // 释放重头:气泡数组
      loadedByConv.value[id] = false; // 下次切回触发 loadHistory 重取
      delete tokensByConv.value[id];
      delete historyErrorByConv.value[id];
      // 长跑泄漏收口:这些 per-conv 字典此前漏在 evict 之外 → 只增不减。被淘汰的对话
      // 必非发送中(ranked 已滤掉发送态),其活跃时间戳/请求 id/等待者均可安全回收。
      delete activeAtByConv.value[id];
      delete reqByConv.value[id];
      delete doneWaiters[id];
    }
  }

  // 历史加载失败的对话集合:别假装是空对话,对话区给「重试」入口
  const historyErrorByConv = ref<Record<string, string>>({});
  function historyError(convId: string | null): string | null {
    if (!convId) return null;
    return historyErrorByConv.value[convId] ?? null;
  }

  /** 标记一个**刚新建**的对话为「历史已加载(空)」——它本来就没有历史。
   *  关键用途:在「新建对话 → 立刻发首条消息」时,createConversation 会同步切换
   *  currentConvId、触发 ChatPanel 的 loadHistory;而那次 loadHistory 在 send 还没
   *  把 sending 置真之前就通过了守卫、随后用**空历史覆盖** byConv,把刚推入的用户
   *  气泡与流式回复一起抹掉(现象:第一次给对话发消息,消息经常被「吃掉」)。
   *  在创建后**同步**调用本函数占位,让那次 loadHistory 因 loaded 守卫直接早退。 */
  function markFresh(convId: string) {
    if (!convId) return;
    if (!byConv.value[convId]) byConv.value[convId] = [];
    loadedByConv.value[convId] = true;
  }

  async function loadHistory(convId: string | null, force = false) {
    if (!convId) return;
    // 访问即刷新最近活跃时间:让正在查看的对话排在 LRU 最前、绝不被卸载(放在守卫前,
    // 命中缓存的查看也算一次访问)。
    touchActivity(convId);
    // 正在运行的对话别用历史覆盖实时气泡
    if (sendingByConv.value[convId]) return;
    if (loadedByConv.value[convId] && !force) return;
    try {
      const msgs = await convApi.getMessages(convId);
      // 防御纵深:异步取历史的空档里,这条对话可能刚开始发送(首条消息竞态)——
      // 此刻本地气泡才是权威,别再用(多半是空的)历史覆盖它。
      if (sendingByConv.value[convId] && !force) return;
      byConv.value[convId] = msgs.map((m) => {
        const at = m.createdAt > 1e12 ? m.createdAt : m.createdAt * 1000;
        if (m.role === "assistant") {
          const { text, artifacts } = parseArtifacts(m.content);
          return { role: m.role, text, artifacts, at } as Bubble;
        }
        return { role: m.role, text: m.content, at } as Bubble;
      });
      loadedByConv.value[convId] = true;
      delete historyErrorByConv.value[convId];
      evictStaleConversations(); // 刚加载一个对话 → 顺手卸载最久未用的,封顶常驻内存
    } catch (e: any) {
      byConv.value[convId] = [];
      historyErrorByConv.value[convId] = e?.message ?? String(e);
    }
  }

  // ── 常驻 claude 进程预热 ──
  // claude CLI spawn 后立即自举(~6.4s), 不等首条消息 —— 在「打开/切换对话」「输入框
  // 聚焦/首个键入」时提前 spawn, 首条消息首响 ~10s → ~3s。fire-and-forget:
  // 后端 best-effort 永不报错, 这里再兜一层 catch。
  // 每对话 60s 去抖: 后端指纹一致本就 no-op, 去抖只为省无谓 IPC; 不做"永久一次"是
  // 因为常驻进程有 10 分钟空闲 TTL, 隔久了切回来仍值得再预热。
  const prewarmAt: Record<string, number> = {};
  function prewarm(
    convId: string | null | undefined,
    opts?: {
      permissionMode?: PermissionMode;
      workMode?: string;
      providerId?: string;
      dynamicWorkflow?: boolean;
    }
  ) {
    if (!convId) return;
    const now = Date.now();
    if (now - (prewarmAt[convId] ?? 0) < 60_000) return;
    prewarmAt[convId] = now;
    // 触发点(如切对话)拿不到输入区状态时, 从与 ChatComposer 同源的 localStorage 取
    // 工作模式/该对话钉的供应商 —— 指纹取值路径与真发送一致, 预热才不白做。
    let workMode = opts?.workMode;
    let providerId = opts?.providerId;
    try {
      if (!workMode)
        workMode = localStorage.getItem("polaris.workMode") === "work" ? "work" : "fast";
      if (!providerId) {
        const bind = JSON.parse(localStorage.getItem("polaris.convProvider.v1") || "{}") || {};
        providerId = bind[convId] || "auto";
      }
    } catch {
      /* localStorage 不可用就让后端按默认档预热 */
    }
    chatApi.prewarm({
      conversationId: convId,
      permissionMode: opts?.permissionMode,
      workMode,
      providerId,
      dynamicWorkflow: opts?.dynamicWorkflow,
    });
  }

  /** 发送一条消息：推入 user 气泡 + 调后端，记录 reqId/sending（不阻塞，多开） */
  async function send(
    convId: string,
    prompt: string,
    displayText: string,
    files: AttachedFile[] | undefined,
    opts: ChatRuntimeOptions
  ) {
    // 关键: 先确保流式监听已挂上, 否则本轮的 delta 可能早于监听器注册而丢失
    // —— 现象正是「第一次发消息看不到输出, 但后台仍在运行」。尤其是从「更多」各工坊
    // (Deck/Web/视频/自媒体/自动化)直接发起时, ChatPanel 尚未挂载、init 从未被调用。
    await init();
    const sessions = useSessionsStore();
    const arr = ensureArr(convId);
    arr.push({
      role: "user",
      text: displayText,
      files: files && files.length ? files : undefined,
      at: Date.now(),
    });
    sendingByConv.value[convId] = true;
    // 清掉上一轮可能残留的 reqId(若用户在 chat_send resolve 前就取消, send 可能晚于
    // cancel 删除条目后落地, 留下永不清理的孤儿 reqId)。新一轮开始前先抹掉, 关掉这个活锁竞态。
    delete reqByConv.value[convId];
    touchActivity(convId);
    sessions.start(convId, displayText.slice(0, 18));
    try {
      const reqId = await chatApi.send({
        imagePaths: uploadedImagePaths(files),
        prompt,
        permissionMode: opts.permissionMode,
        skillIds: opts.skillIds,
        goal: opts.goal,
        dynamicWorkflow: opts.dynamicWorkflow,
        useKb: opts.useKb,
        batchBuild: opts.batchBuild,
        batchSize: opts.batchSize,
        agentMode: opts.agentMode,
        ...expertRequestFields(opts),
        workMode: opts.workMode,
        providerId: opts.providerId,
        conversationId: convId,
      });
      reqByConv.value[convId] = reqId;
    } catch (e: any) {
      arr.push({
        role: "assistant",
        text: `[发送失败] ${humanizeError(e)}`,
        at: Date.now(),
        err: true,
        severity: "error",
      });
      sendingByConv.value[convId] = false;
      sessions.finish(convId);
      wakeWaiters(convId); // 否则分批循环 await 永挂
    }
  }

  /** 清空当前对话上下文:消息清零、对话保留;后端同时把旧内容后台沉淀进记忆库。
   *  返回清掉的消息数。生成中禁止清空(先停止),后端沉淀忙时会抛错、此处原样上抛。 */
  async function clearContext(convId: string | null): Promise<number> {
    if (!convId) return 0;
    if (sendingByConv.value[convId]) throw new Error("正在生成中，先停止再清空");
    const removed = await convApi.clearContext(convId);
    byConv.value[convId] = [];
    loadedByConv.value[convId] = true; // 清空后空历史就是权威,别再回读覆盖
    delete tokensByConv.value[convId];
    delete historyErrorByConv.value[convId];
    touchActivity(convId);
    return removed;
  }

  async function cancel(convId: string | null) {
    if (!convId) return;
    const sessions = useSessionsStore();
    const req = reqByConv.value[convId];
    if (req) {
      try {
        await chatApi.cancel(req);
      } catch {
        /* ignore */
      }
    }
    sendingByConv.value[convId] = false;
    delete reqByConv.value[convId];
    touchActivity(convId);
    sessions.finish(convId);
    wakeWaiters(convId); // 取消后唤醒分批循环, 让它看到 !isRunning 自行收尾
  }

  const deltas = createDeltaBuffer(ensureArr);

  /** app 级初始化：注册一次流式监听，按 conversationId 路由进各自缓冲。
   *  返回缓存的就绪 promise：重复调用只注册一次，且每个调用方都能 await 到「监听已挂上」。 */
  function init(): Promise<void> {
    if (initPromise) return initPromise;
    initPromise = listen<ChatStreamEvent>("chat:stream", (ev) => {
      const cid = ev.conversationId;
      if (!cid) return; // 无会话归属的事件无法路由（理论上不会出现）
      touchActivity(cid); // 任何流式事件都算心跳:喂给无声死亡看门狗,证明后端仍活着
      const arr = ensureArr(cid);
      if (ev.kind === "delta") {
        deltas.enqueue(cid, ev.text ?? "");
        return;
      }
      // 非 delta 事件:先把本会话挂起的增量落地,保证「文本 → 工具/错误/终态」顺序不乱。
      deltas.flush(cid);
      if (ev.kind === "tool") {
        arr.push({
          role: "tool",
          text: `调用工具:${ev.tool ?? "(unknown)"}`,
          tool: ev.tool,
          toolDetail: ev.text || undefined,
          at: Date.now(),
        });
      } else if (ev.kind === "artifact") {
        receiveArtifact(arr, cid, ev.text);
      } else if (ev.kind === "meta") {
        // 上下文预算自检：后端估算的本轮 input token 数（纯数字文本）
        const n = parseInt(ev.text ?? "", 10);
        if (!Number.isNaN(n)) tokensByConv.value[cid] = n;
      } else if (ev.kind === "error") {
        // Claude CLI 会把普通诊断写到 stderr；这不代表本轮失败，后续正文仍应正常展示。
        // 新 emitter 可直接给 severity；旧 emitter 继续用稳定的 [stderr] 前缀兼容判定。
        const raw = ev.text ?? "";
        const warning = ev.severity === "warning" || /^\[stderr\]/i.test(raw.trim());
        const detail = warning ? raw.replace(/^\[stderr\]\s*/i, "") : raw;
        arr.push({
          role: "assistant",
          text: warning
            ? `[警告] ${detail}`
            : `[错误] ${humanizeError(new Error(detail || "本轮执行失败"))}`,
          err: !warning,
          severity: warning ? "warning" : "error",
          at: Date.now(),
        });
      } else if (ev.kind === "done") {
        // 终态：结束运行态 + 工位会话；若用户不在看该对话则打墨蓝未读点
        sendingByConv.value[cid] = false;
        delete reqByConv.value[cid];
        touchActivity(cid);
        // 本轮的实时气泡(含 [错误] 等未持久化的合成气泡)即为该对话的权威视图,
        // 标记 loaded 防止之后切回时 loadHistory 用后端副本覆盖、丢掉这些气泡。
        loadedByConv.value[cid] = true;
        const app = useAppStore();
        const sessions = useSessionsStore();
        sessions.finish(cid);
        app.markUnread(cid);
        previewDeliverable(arr, cid);
        // 唤醒分批编排循环：本轮已结束，可读清单决定续不续
        wakeWaiters(cid);
        // 本轮结束 → 该对话不再受「发送中」保护,顺手做一次 LRU 卸载(封顶常驻气泡)。
        evictStaleConversations();
      }
    }).then(() => undefined);
    return initPromise;
  }

  /** 内存治理:外部(如 App 在后台化时)主动触发的轻量回收 —— 立刻卸载最久未用对话的气泡,
   *  把常驻内存收回到 LRU 上限。安全:正在发送 / 最近活跃的对话恒受保护,卸载的切回时重取。 */
  function trimMemory() {
    evictStaleConversations();
  }

  return {
    byConv,
    bubblesFor,
    isSending,
    runningConvIds,
    activityAt,
    pushBubble,
    loadHistory,
    markFresh,
    historyError,
    send,
    prewarm,
    cancel,
    clearContext,
    init,
    waitForDone,
    inputTokens,
    trimMemory,
  };
});
