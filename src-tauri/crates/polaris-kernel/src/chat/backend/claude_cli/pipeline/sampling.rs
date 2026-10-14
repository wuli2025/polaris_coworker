
/// 看门狗深检采样: root 进程树(含 root)的子孙数 + 整树累计 CPU 时间。
/// cpu 单位各平台不同(Windows 100ns / Linux jiffies / mac 秒), 只用于跨采样单调比较。
pub(crate) struct TreeSample {
    pub(crate) descendants: usize,
    pub(crate) cpu: u64,
}

/// 从 (pid, ppid) 全表收出以 root 为根的进程树(含 root)。`contains` 防 PID 复用造出的环。
pub(super) fn collect_tree(root: u32, pairs: &[(u32, u32)]) -> Vec<u32> {
    let mut tree = vec![root];
    let mut i = 0;
    while i < tree.len() {
        let parent = tree[i];
        for &(pid, ppid) in pairs {
            if ppid == parent && pid != parent && !tree.contains(&pid) {
                tree.push(pid);
            }
        }
        i += 1;
    }
    tree
}

/// Windows: toolhelp 快照收 (pid, ppid), 再只对树内成员查 GetProcessTimes。
/// None = 快照失败(调用方退回空闲即杀)。
#[cfg(windows)]
pub(crate) fn sample_tree(root: u32) -> Option<TreeSample> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
        if Process32First(snap, &mut entry) != 0 {
            loop {
                pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));
                if Process32Next(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    let tree = collect_tree(root, &pairs);
    let mut cpu: u64 = 0;
    for &pid in &tree {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                continue; // 拿不到句柄(权限/刚退出)记 0: 只影响该成员的 CPU 票, 不影响单调性
            }
            let mut t: [FILETIME; 4] = std::mem::zeroed();
            if GetProcessTimes(h, &mut t[0], &mut t[1], &mut t[2], &mut t[3]) != 0 {
                for ft in [&t[2], &t[3]] {
                    cpu += ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
                }
            }
            CloseHandle(h);
        }
    }
    Some(TreeSample {
        descendants: tree.len().saturating_sub(1),
        cpu,
    })
}

/// Linux(容器/server): 单遍扫 /proc/<pid>/stat 同时拿 ppid 与 utime+stime。
/// stat 第 2 字段(comm)可含空格/括号, 一律从最后一个 ')' 之后再按空格切。
#[cfg(target_os = "linux")]
pub(crate) fn sample_tree(root: u32) -> Option<TreeSample> {
    let mut procs: Vec<(u32, u32, u64)> = Vec::new(); // (pid, ppid, cpu)
    for ent in std::fs::read_dir("/proc").ok()? {
        let Ok(ent) = ent else { continue };
        let name = ent.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(ent.path().join("stat")) else {
            continue;
        };
        let Some(rest) = stat.rfind(')').map(|i| &stat[i + 1..]) else {
            continue;
        };
        let f: Vec<&str> = rest.split_whitespace().collect();
        // rest 内 0-based: state=0, ppid=1, …, utime=11, stime=12
        let (Some(ppid), Some(ut), Some(st)) = (
            f.get(1).and_then(|s| s.parse::<u32>().ok()),
            f.get(11).and_then(|s| s.parse::<u64>().ok()),
            f.get(12).and_then(|s| s.parse::<u64>().ok()),
        ) else {
            continue;
        };
        procs.push((pid, ppid, ut + st));
    }
    let pairs: Vec<(u32, u32)> = procs.iter().map(|&(p, pp, _)| (p, pp)).collect();
    let tree = collect_tree(root, &pairs);
    let cpu = procs
        .iter()
        .filter(|(p, _, _)| tree.contains(p))
        .map(|&(_, _, c)| c)
        .sum();
    Some(TreeSample {
        descendants: tree.len().saturating_sub(1),
        cpu,
    })
}

/// macOS 及其它 unix: 一次 `ps -axo pid=,ppid=,cputime=` 全表。cputime 形如
/// "0:00.12" / "1:02:03" / "1-02:03:04", 解析成秒(只求单调可比)。
#[cfg(all(unix, not(target_os = "linux")))]
pub(crate) fn sample_tree(root: u32) -> Option<TreeSample> {
    fn parse_cputime(s: &str) -> u64 {
        let (days, rest) = match s.split_once('-') {
            Some((d, r)) => (d.parse::<u64>().unwrap_or(0), r),
            None => (0, s),
        };
        let mut secs = 0f64;
        for part in rest.split(':') {
            secs = secs * 60.0 + part.parse::<f64>().unwrap_or(0.0);
        }
        days * 86400 + secs as u64
    }
    let out = Command::new("ps")
        .args(["-axo", "pid=,ppid=,cputime="])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut procs: Vec<(u32, u32, u64)> = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let (Some(pid), Some(ppid)) = (
            f.first().and_then(|s| s.parse::<u32>().ok()),
            f.get(1).and_then(|s| s.parse::<u32>().ok()),
        ) else {
            continue;
        };
        procs.push((pid, ppid, f.get(2).map_or(0, |s| parse_cputime(s))));
    }
    let pairs: Vec<(u32, u32)> = procs.iter().map(|&(p, pp, _)| (p, pp)).collect();
    let tree = collect_tree(root, &pairs);
    let cpu = procs
        .iter()
        .filter(|(p, _, _)| tree.contains(p))
        .map(|&(_, _, c)| c)
        .sum();
    Some(TreeSample {
        descendants: tree.len().saturating_sub(1),
        cpu,
    })
}

