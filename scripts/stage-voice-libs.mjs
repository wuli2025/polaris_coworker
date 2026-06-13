// 把 voice-asr 的原生运行时库(sherpa-onnx + onnxruntime)从 cargo 构建输出拷到
// src-tauri/voice-libs/<platform>/,供 tauri.<platform>.conf.json 的 bundle.resources
// 打进安装包。作为 tauri.conf.json 的 build.beforeBundleCommand 运行:此刻 cargo 编译
// 已结束(库已在 target 下落地),打包尚未开始。
//
//   · Windows → 4 个 DLL,打到 exe 同目录(Win 加载器首查 exe 目录,隐式依赖即可解析)
//   · macOS   → dylib 打到 Contents/Resources/voice-libs/,配 @rpath(见 .cargo/config.toml)
//
// 仅当以 `--features voice-asr` 构建时这些库才存在。找不到即非零退出 —— 宁可让发版
// 构建早早失败,也绝不静默出一个「启动即崩」的安装包:sherpa-onnx-c-api 是 exe 的隐式
// 依赖,缺它整个 app(不止语音)起不来。
//
// 非 Win/mac(或本就没开 voice-asr 的构建)直接放行,不阻塞。

import {
  existsSync,
  mkdirSync,
  readdirSync,
  copyFileSync,
  statSync,
  rmSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const srcTauri = resolve(here, "..", "src-tauri");
const targetRoot = join(srcTauri, "target");

const isWin = process.platform === "win32";
const isMac = process.platform === "darwin";
if (!isWin && !isMac) {
  console.log(`[stage-voice-libs] 平台 ${process.platform} 无需暂存,跳过`);
  process.exit(0);
}

// 需要随包分发的运行时库:Win 用精确名(已实测),mac 用前缀匹配(命名带版本号时也兜得住)。
const winExact = [
  "onnxruntime.dll",
  "onnxruntime_providers_shared.dll",
  "sherpa-onnx-c-api.dll",
  "sherpa-onnx-cxx-api.dll",
];
const macMatch = (name) =>
  name.endsWith(".dylib") && /(^lib)?(sherpa-onnx|onnxruntime)/.test(name);

const stageDir = join(srcTauri, "voice-libs");

// 在 target/ 下递归找文件,返回 名→所有命中路径(按 mtime 倒序) 的映射。
// (release/universal-apple-darwin 等输出目录因平台/profile 而异,直接全树扫最稳;
//  mac universal 会在 aarch64-/x86_64-apple-darwin 各产一份同名 dylib,故按名归组。)
function scanLibs(predicate) {
  const found = new Map(); // name -> [{path, mtime}, ...]
  const stack = [targetRoot];
  while (stack.length) {
    const dir = stack.pop();
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const e of entries) {
      const p = join(dir, e.name);
      if (e.isDirectory()) {
        // 跳过明显无关的子树,省时间
        if (e.name === "incremental" || e.name === ".fingerprint") continue;
        stack.push(p);
      } else if (predicate(e.name)) {
        const m = statSync(p).mtimeMs;
        const list = found.get(e.name) ?? [];
        list.push({ path: p, mtime: m });
        found.set(e.name, list);
      }
    }
  }
  for (const list of found.values()) list.sort((a, b) => b.mtime - a.mtime);
  return found;
}

// 把同名 dylib 的各架构版本 lipo 成一个 universal,落到 dest。
// 只在 mac 用;若 lipo 不可用或本就单架构,退回拷最新一份。
function stageMacLib(name, paths, dest) {
  // 区分架构:universal 构建下 aarch64-/x86_64-apple-darwin 各一份。同一目录里的同名文件
  // (mtime 最近的)各取一,避免把 release 与 deps 里的同一份重复喂给 lipo。
  const byDir = new Map();
  for (const { path } of paths) {
    const arch = /aarch64-apple-darwin/.test(path)
      ? "arm64"
      : /x86_64-apple-darwin/.test(path)
      ? "x86_64"
      : "host";
    if (!byDir.has(arch)) byDir.set(arch, path);
  }
  const slices = [...byDir.values()];
  if (slices.length >= 2) {
    try {
      execFileSync("lipo", ["-create", ...slices, "-output", dest]);
      console.log(`[stage-voice-libs]  + ${name} (universal: ${slices.length} 架构)`);
      return;
    } catch (e) {
      console.warn(`[stage-voice-libs]  ! lipo ${name} 失败,退回单架构: ${e.message}`);
    }
  }
  copyFileSync(paths[0].path, dest);
  console.log(`[stage-voice-libs]  + ${name}`);
}

if (!existsSync(targetRoot)) {
  console.error(
    `[stage-voice-libs] 找不到 target 目录(${targetRoot})—— 还没编译?`
  );
  process.exit(1);
}

// 每次重建暂存目录,避免上轮残留库混进包
rmSync(stageDir, { recursive: true, force: true });
mkdirSync(stageDir, { recursive: true });

const predicate = isWin ? (n) => winExact.includes(n) : macMatch;
const libs = scanLibs(predicate);

if (libs.size === 0) {
  console.error(
    "[stage-voice-libs] 在 target/ 下没找到任何 sherpa/onnxruntime 运行时库。\n" +
      "  发版构建必须带 `--features voice-asr`(否则不会产出这些库)。\n" +
      "  拒绝出一个会启动即崩的安装包,这里直接失败。"
  );
  process.exit(1);
}

// Windows:四个库一个都不能少(都是隐式依赖)。
if (isWin) {
  const missing = winExact.filter((n) => !libs.has(n));
  if (missing.length) {
    console.error(
      `[stage-voice-libs] 缺少 Windows 运行时库: ${missing.join(", ")}`
    );
    process.exit(1);
  }
}

let n = 0;
for (const [name, paths] of libs) {
  const dest = join(stageDir, name);
  if (isMac) {
    stageMacLib(name, paths, dest);
  } else {
    copyFileSync(paths[0].path, dest);
    console.log(`[stage-voice-libs]  + ${name}`);
  }
  n++;
}
console.log(`[stage-voice-libs] 已暂存 ${n} 个运行时库 → ${stageDir}`);
