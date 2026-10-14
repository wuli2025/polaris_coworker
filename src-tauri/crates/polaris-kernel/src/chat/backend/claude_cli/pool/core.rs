use super::*;

/// LRU 上限: 每个常驻 claude ~200-400MB, 2 个封顶 → 最多 ~800MB, 桌面可接受。
const MAX_SESSIONS: usize = 2;
/// 空闲 TTL: 10 分钟没消息就收割(惰性检查, 在每次发消息时扫)。
/// 注意: 「正在跑轮次」的会话不论多久都豁免(见 sweep_expired 的 busy 谓词)。
const IDLE_TTL_MS: u64 = 10 * 60 * 1000;

pub(crate) struct Entry<T> {
    pub(crate) conv: String,
    pub(crate) fp: Fingerprint,
    pub(crate) last_used: u64,
    pub(crate) payload: T,
}

/// 池的纯记账核: 指纹比较/LRU/TTL 全在这里, 不碰真进程 → 单测用假 payload +
/// 注入的毫秒时钟即可覆盖, 不 sleep 不 spawn。真池 [`POOL`] 以 SessionProc 为 payload。
pub(crate) struct PoolCore<T> {
    pub(crate) entries: Vec<Entry<T>>,
}

impl<T> PoolCore<T> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// TTL 收割: 摘出所有空闲超时条目(杀进程由调用方在锁外做)。
    /// busy 谓词豁免「正在跑轮次」的会话: last_used 只在轮次**开始**时刷新, 一轮跑得
    /// 比 TTL 久的长任务(几十分钟编译/整夜脚本)绝不能被当闲置误杀 —— 否则任何其它
    /// 对话一发消息/预热, 这里的惰性清扫就会把进行中的长任务整树杀掉且静默收尾。
    pub(crate) fn sweep_expired(&mut self, now: u64, busy: impl Fn(&T) -> bool) -> Vec<T> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.entries.len() {
            if now.saturating_sub(self.entries[i].last_used) >= IDLE_TTL_MS
                && !busy(&self.entries[i].payload)
            {
                out.push(self.entries.remove(i).payload);
            } else {
                i += 1;
            }
        }
        out
    }

    /// 取条目(指纹 + payload)。
    pub(crate) fn get_mut(&mut self, conv: &str) -> Option<(&Fingerprint, &mut T)> {
        self.entries
            .iter_mut()
            .find(|e| e.conv == conv)
            .map(|e| (&e.fp, &mut e.payload))
    }

    pub(crate) fn touch(&mut self, conv: &str, now: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.conv == conv) {
            e.last_used = now;
        }
    }

    pub(crate) fn remove(&mut self, conv: &str) -> Option<T> {
        let i = self.entries.iter().position(|e| e.conv == conv)?;
        Some(self.entries.remove(i).payload)
    }

    /// 插入新会话; 超过 LRU 上限时摘出最久未用的旧会话返回给调用方收割。
    /// busy 谓词与 sweep_expired 同义: 逐出只在**非忙碌**条目里挑, 全员在跑轮次时
    /// 宁可临时超上限(多占一个进程的内存, 轮次结束后由 TTL 收回), 也不杀进行中的任务。
    pub(crate) fn insert(
        &mut self,
        conv: String,
        fp: Fingerprint,
        payload: T,
        now: u64,
        busy: impl Fn(&T) -> bool,
    ) -> Vec<T> {
        // 同 conv 旧条目先摘(调用方应已处理, 这里兜底防泄漏)
        let mut evicted: Vec<T> = self.remove(&conv).into_iter().collect();
        self.entries.push(Entry {
            conv,
            fp,
            last_used: now,
            payload,
        });
        while self.entries.len() > MAX_SESSIONS {
            // 新插入者恒在末位(只会移除它前面的条目), 不列为逐出候选 —— 否则「其余
            // 全员忙碌」时会把刚建好的会话自己逐出去, 新对话的消息就发不出去了。
            let new_idx = self.entries.len() - 1;
            let candidate = self
                .entries
                .iter()
                .enumerate()
                .filter(|(i, e)| *i != new_idx && !busy(&e.payload))
                .min_by_key(|(_, e)| e.last_used)
                .map(|(i, _)| i);
            match candidate {
                Some(idx) => evicted.push(self.entries.remove(idx).payload),
                None => break, // 其余全员忙碌: 不杀活轮, 容忍暂时超上限
            }
        }
        evicted
    }

    /// 超编收敛:insert 在「其余全员忙碌」时容忍超上限(不杀活轮),这里在**检出路径**
    /// 把编制收回来 —— 轮次结束后条目不再忙碌,下一次任何对话发消息/预热即逐出
    /// 「最久未用 · 非忙碌 · 非当前 conv」者。没有它,超编会因每次使用都 touch 续命
    /// 而永久滞留(N 个常驻进程 × 200-400MB,违背上限 2 的资源承诺)。
    pub(crate) fn trim_over_cap(&mut self, keep_conv: &str, busy: impl Fn(&T) -> bool) -> Vec<T> {
        let mut evicted = Vec::new();
        while self.entries.len() > MAX_SESSIONS {
            let candidate = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.conv != keep_conv && !busy(&e.payload))
                .min_by_key(|(_, e)| e.last_used)
                .map(|(i, _)| i);
            match candidate {
                Some(idx) => evicted.push(self.entries.remove(idx).payload),
                None => break, // 仍全员忙碌(或只剩当前 conv):等下次检出再收
            }
        }
        evicted
    }

    pub(crate) fn drain_all(&mut self) -> Vec<T> {
        self.entries.drain(..).map(|e| e.payload).collect()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut T)> {
        self.entries
            .iter_mut()
            .map(|e| (e.conv.as_str(), &mut e.payload))
    }
}

// ───────────────────────── 真池 ─────────────────────────
