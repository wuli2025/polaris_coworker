/**
 * 手机壳远程更新检查 —— 对齐桌面 updater 的三端点镜像模式(tauri.conf.json):
 *  1. Cloudflare:llmwiki.cloud/downloads/mobile-latest.json(发版时传 R2/Pages)
 *  2. gh-proxy 镜像(国内可达)
 *  3. GitHub Release 直连
 * 发版约定:GitHub Release 固定附两个资产 —— `mobile-latest.json` + `polaris-mobile.apk`,
 * 资产名不带版本号,`releases/latest/download/<名字>` 才能永远指向最新。
 *
 * 版本比较只认 versionCode(单调整数,与 android/app/build.gradle 同步),
 * 不解析语义版本 —— 与 Android 包管理器的口径一致,永不歧义。
 *
 * 下载不走 App 内:点下载链接 = 顶层导航到外部域,Capacitor 会交给系统浏览器
 * (ACTION_VIEW),浏览器下载 APK 后用户点开即装,无需任何原生插件。
 */

export const APP_VERSION = "1.2";
/** 与 android/app/build.gradle 的 versionCode 保持同步(出包时一起 +1)。 */
export const APP_VERSION_CODE = 3;

export interface MobileLatest {
  version: string;
  versionCode: number;
  /** 首选下载地址(建议填 gh-proxy 镜像,国内直连快)。 */
  url: string;
  notes?: string;
  /** 备用下载地址(GitHub 直连等)。 */
  mirrors?: string[];
}

const ENDPOINTS = [
  "https://llmwiki.cloud/downloads/mobile-latest.json",
  "https://gh-proxy.com/https://github.com/wuli2025/polaris_coworker/releases/latest/download/mobile-latest.json",
  "https://github.com/wuli2025/polaris_coworker/releases/latest/download/mobile-latest.json",
];

async function fetchOne(url: string): Promise<MobileLatest | null> {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), 8000);
  try {
    const r = await fetch(url, { signal: ctl.signal, cache: "no-store" });
    if (!r.ok) return null;
    const j = (await r.json()) as MobileLatest;
    if (typeof j?.versionCode !== "number" || typeof j?.url !== "string") return null;
    return j;
  } catch {
    return null;
  } finally {
    clearTimeout(t);
  }
}

/** 逐端点尝试,拿到第一份合法 latest 即返回;全挂返回 null(离线/端点未布)。 */
export async function fetchLatest(): Promise<MobileLatest | null> {
  for (const u of ENDPOINTS) {
    const r = await fetchOne(u);
    if (r) return r;
  }
  return null;
}

export function hasUpdate(l: MobileLatest): boolean {
  return l.versionCode > APP_VERSION_CODE;
}

/** 下载地址候选:主 url + 镜像,过滤空串。 */
export function downloadUrls(l: MobileLatest): string[] {
  return [l.url, ...(l.mirrors ?? [])].filter(Boolean);
}
