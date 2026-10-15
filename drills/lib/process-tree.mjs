import { readdir, readFile } from 'node:fs/promises';
export function procIdentity(text) {
  // comm can contain spaces and ')'; remaining fields start after its final ')'.
  const fields = text.slice(text.lastIndexOf(')') + 2).trim().split(/\s+/);
  return { parent: Number(fields[1]), started: fields[19] };
}
export function descendants(parent, records) {
  const out = [];
  const pending = [parent];
  while (pending.length) {
    const next = pending.shift();
    for (const [pid, info] of records) if (info.parent === next && !out.some(r => r.pid === pid)) {
      out.push({ pid, ...info }); pending.push(pid);
    }
  }
  return out.reverse();
}
export async function captureTree(parent) {
  if (!parent) return [];
  const records = new Map();
  for (const pid of await readdir('/proc')) {
    if (!/^\d+$/.test(pid)) continue;
    try { records.set(Number(pid), procIdentity(await readFile(`/proc/${pid}/stat`, 'utf8'))); } catch { /* exited */ }
  }
  return descendants(parent, records);
}
export async function signalTree(records, signal, read = readFile, kill = process.kill) {
  for (const entry of records) {
    try {
      const now = procIdentity(await read(`/proc/${entry.pid}/stat`, 'utf8'));
      // PID alone is insufficient: never signal a reused PID during a long drill.
      if (entry.started === now.started) kill(entry.pid, signal);
    } catch (error) { if (!['ESRCH', 'ENOENT'].includes(error.code)) throw error; }
  }
}
