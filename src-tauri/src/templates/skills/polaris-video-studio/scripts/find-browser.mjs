/* polaris :: find-browser.mjs
 *
 * 找一个本机已有的 Chromium 系浏览器给 Playwright 用 —— 不触发 Playwright 的自动下载。
 * 与 Rust 侧 forge.rs / forge_capture.rs 的 find_chromium 同一条优先级链，让 Node 脚本
 * 复用北极星已有的浏览器分发（app 通过 POLARIS_CHROMIUM* 注入 ureq 下载/自带的 headless-shell）：
 *
 *   1. 显式 env（app 注入；Docker 注入 headless-shell；用户也可手动指）
 *   2. 本机已装的 Edge / Chrome / Chromium 固定路径
 *   3. Playwright channel（按名字驱动系统 Edge/Chrome，仍不下载）
 *
 * 返回值直接展开进 chromium.launch(...)：要么 {executablePath}，要么 {channel}。
 */
import { existsSync } from "node:fs";

export function findLocalBrowser() {
  for (const v of [
    process.env.POLARIS_CHROMIUM_HEADLESS_SHELL,
    process.env.POLARIS_CHROMIUM,
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
  ]) {
    if (v && existsSync(v)) return { executablePath: v };
  }

  const plat = process.platform;
  const candidates =
    plat === "win32"
      ? [
          "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
          "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
          "C:/Program Files/Google/Chrome/Application/chrome.exe",
          "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
        ]
      : plat === "darwin"
        ? [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
          ]
        : [
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/microsoft-edge",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
          ];
  for (const p of candidates) {
    if (existsSync(p)) return { executablePath: p };
  }

  return { channel: plat === "win32" ? "msedge" : "chrome" };
}

export function describeBrowser(opt) {
  return opt.executablePath ? `本机浏览器 ${opt.executablePath}` : `系统 channel: ${opt.channel}`;
}
