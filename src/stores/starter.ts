import { defineStore } from "pinia";
import { ref, computed } from "vue";

/**
 * 「上手五件事」的状态枢纽(见桌面《北极星 · 搭子式引导方案》)。
 *
 * 引导本体是**贴着真实按钮的一句话**(StarterGuide.vue),这里只管进度:哪件做了、
 * 哪件今天不想做。为什么不做全屏分步教学:北极星的核心动作全是耗时真活(盘点几分钟、
 * 建索引几小时),遮罩式 next→next 假定每步秒完成,在这儿必然卡在第二步。
 *
 * 落盘键 polaris.starter.v1:
 *   done      已完成的那几件
 *   snooze    key → 「明天再说」那天的本地日期串,换一天自动回来
 *   dismissed 整套不再提示(设置里「重看所有引导」可复位)
 *   cur       光标停在第几件 —— 「下一步 / 上一步」翻页用,关掉应用再开还停在原处
 *
 * 关于「下一步」:翻页**不等于做完**。跳过去只是挪光标,那件仍留在未完成里,
 * 转一圈还会回来;只有真按下那颗按钮(或后台任务真跑起来)才会勾掉。
 * 否则用户点五下「下一步」就把整套引导消掉了,却一件也没做。
 */
export type StarterKey = "provider" | "inventory" | "index" | "ask" | "auto";

export const STARTER_KEYS: StarterKey[] = ["provider", "inventory", "index", "ask", "auto"];

const KEY = "polaris.starter.v1";
/** L0 首屏问到的「你主要想让它帮你干哪类活」,第 4 件替用户填的问题据此换一句。 */
export const INTENT_KEY = "polaris.starterIntent.v1";
export type StarterIntent = "files" | "write" | "code" | "all";

interface Persisted {
  v: 1;
  done: StarterKey[];
  snooze: Partial<Record<StarterKey, string>>;
  dismissed: boolean;
  cur: number;
}

/** 本地日(不用 toISOString:那是 UTC,东八区晚上八点后会提前跳到"明天")。 */
function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`;
}

function load(): Persisted {
  const empty: Persisted = { v: 1, done: [], snooze: {}, dismissed: false, cur: 0 };
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return empty;
    const p = JSON.parse(raw) as Partial<Persisted>;
    // 光标夹回合法区间:旧记录没有这个字段,条目删减后也可能越界
    const cur = Number.isInteger(p.cur) ? Math.min(Math.max(0, p.cur as number), STARTER_KEYS.length - 1) : 0;
    return {
      v: 1,
      // 过滤未知 key:增删条目后旧记录不会留下幽灵项
      done: (p.done ?? []).filter((k): k is StarterKey => STARTER_KEYS.includes(k as StarterKey)),
      snooze: p.snooze ?? {},
      dismissed: !!p.dismissed,
      cur,
    };
  } catch {
    return empty;
  }
}

export const useStarterStore = defineStore("starter", () => {
  const init = load();
  const done = ref<Set<StarterKey>>(new Set(init.done));
  const snooze = ref<Partial<Record<StarterKey, string>>>(init.snooze);
  const dismissed = ref(init.dismissed);
  const cur = ref(init.cur);

  function persist() {
    try {
      localStorage.setItem(
        KEY,
        JSON.stringify({
          v: 1,
          done: [...done.value],
          snooze: snooze.value,
          dismissed: dismissed.value,
          cur: cur.value,
        } satisfies Persisted)
      );
    } catch {
      /* 隐私模式/配额满:引导进度丢了顶多重来一遍,不值得打断用户 */
    }
  }

  const allDone = computed(() => STARTER_KEYS.every((k) => done.value.has(k)));
  /** 全做完 / 主动关掉 → 整套不再出现。 */
  const visible = computed(() => !dismissed.value && !allDone.value);
  const doneCount = computed(() => done.value.size);

  function isSnoozed(k: StarterKey): boolean {
    return snooze.value[k] === today();
  }
  /** 从第 start 件起往后找第一件「没做 && 今天没被推迟」的,绕一圈找不到就是 null。 */
  function pickFrom(start: number, dir: 1 | -1 = 1): StarterKey | null {
    const n = STARTER_KEYS.length;
    for (let i = 0; i < n; i++) {
      const k = STARTER_KEYS[(((start + dir * i) % n) + n) % n];
      if (!done.value.has(k) && !isSnoozed(k)) return k;
    }
    return null;
  }

  /**
   * 当前该指哪一件:光标停的那件;它已完成 / 今天被推迟了就顺势往后挪一件。
   * 光标本身不在 computed 里改(那会让读取产生副作用),只在 next/prev/markDone 里动。
   */
  const activeKey = computed<StarterKey | null>(() => pickFrom(cur.value));

  /** 当前是第几件(1 起,按固定编号,不因跳过而重排)。 */
  const stepNo = computed(() => {
    const k = activeKey.value;
    return k ? STARTER_KEYS.indexOf(k) + 1 : 0;
  });
  const total = STARTER_KEYS.length;
  /** 还剩几件能翻 —— 只剩当前这一件时,上一步 / 下一步都该禁掉。 */
  const pendingCount = computed(
    () => STARTER_KEYS.filter((k) => !done.value.has(k) && !isSnoozed(k)).length
  );
  const canFlip = computed(() => pendingCount.value > 1);

  /** 翻页 = 只挪光标,不勾掉:跳过的那件转一圈还会回来。 */
  function step(dir: 1 | -1) {
    const k = activeKey.value;
    if (!k || !canFlip.value) return;
    const nk = pickFrom(STARTER_KEYS.indexOf(k) + dir, dir);
    if (!nk || nk === k) return;
    cur.value = STARTER_KEYS.indexOf(nk);
    persist();
  }
  const next = () => step(1);
  const prev = () => step(-1);

  function markDone(k: StarterKey) {
    if (done.value.has(k)) return;
    done.value = new Set(done.value).add(k);
    // 勾掉的正是光标停的这件 → 光标跟着往后走一格,别停在已完成项上空转
    if (STARTER_KEYS[cur.value] === k) cur.value = (cur.value + 1) % STARTER_KEYS.length;
    persist();
  }
  function snoozeToday(k: StarterKey) {
    snooze.value = { ...snooze.value, [k]: today() };
    if (STARTER_KEYS[cur.value] === k) cur.value = (cur.value + 1) % STARTER_KEYS.length;
    persist();
  }
  function dismiss() {
    dismissed.value = true;
    persist();
  }
  /** 设置页「重看所有引导」:清空进度,从第一件重来。 */
  function resetAll() {
    done.value = new Set();
    snooze.value = {};
    dismissed.value = false;
    cur.value = 0;
    persist();
  }

  const intent = ref<StarterIntent | null>(
    (localStorage.getItem(INTENT_KEY) as StarterIntent | null) ?? null
  );
  function setIntent(v: StarterIntent) {
    intent.value = v;
    try {
      localStorage.setItem(INTENT_KEY, v);
    } catch {
      /* 同上 */
    }
  }

  return {
    done,
    dismissed,
    visible,
    allDone,
    doneCount,
    activeKey,
    stepNo,
    total,
    pendingCount,
    canFlip,
    next,
    prev,
    isSnoozed,
    markDone,
    snoozeToday,
    dismiss,
    resetAll,
    intent,
    setIntent,
  };
});
