import { captureTree, signalTree } from './process-tree.mjs';
import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { waitFor, Blocked, check, childEnv, repo } from './harness.mjs';
export function start(h, name, command, args, opts = {}, spawnImpl = spawn) {
  if (h.cancelled) throw new Error('Drill interrupted');
  const child = spawnImpl(command, args, { cwd: repo, env: childEnv(h), detached: true,
    stdio: ['ignore', 'pipe', 'pipe'], ...opts });
  let spawnError, result, writes = Promise.resolve();
  const output = [];
  const record = text => {
    const safe = h.redact(text);
    output.push(safe);
    writes = writes.then(() => h.log(name, safe));
  };
  // Buffer by line, so credentials split across pipe chunks cannot evade redaction.
  for (const stream of [child.stdout, child.stderr]) {
    let pending = '';
    stream?.on('data', chunk => {
      pending += chunk.toString();
      let end;
      while ((end = pending.indexOf('\n')) >= 0) {
        record(pending.slice(0, end + 1)); pending = pending.slice(end + 1);
      }
    });
    stream?.on('end', () => { if (pending) record(pending); pending = ''; });
  }
  child.on('error', error => { spawnError = error; });
  const exited = new Promise(resolve => {
    child.on('close', (code, signal) => { result = { code, signal }; resolve(result); });
  });
  const signal = sig => {
    if (!child.pid) return;
    try { process.kill(-child.pid, sig); } catch (e) { if (e.code !== 'ESRCH') throw e; }
  };
  const stop = async () => {
    // Never signal a completed command's numeric PID: a long drill may see PID reuse.
    if (result) { await writes; return; }
    const tree = await captureTree(child.pid);
    signal('SIGTERM');
    await signalTree(tree, 'SIGTERM');
    if (!result) await waitFor(() => result, { timeoutMs: 3000, label: `${name} shutdown` }).catch(() => signal('SIGKILL'));
    if (!result) signal('SIGKILL');
    await signalTree(tree, 'SIGKILL');
    await exited; await writes;
  };
  h.cleanup.push(stop);
  return { child, exited, stop, output,
    async done(timeoutMs = h.commandTimeout) {
      try { await waitFor(() => result || spawnError, { timeoutMs, label: name }); }
      catch (e) { await stop(); throw e; }
      await writes;
      if (spawnError?.code === 'ENOENT') throw new Blocked(`Required command missing: ${command}`);
      if (spawnError) throw spawnError;
      return { ...result, text: output.join('') };
    } };
}
export async function command(h, name, executable, args, opts = {}) {
  const result = await start(h, name, executable, args, opts).done();
  check(result.code === 0, `${name} exit ${result.code}, signal ${result.signal}`);
  return result;
}
export async function ownedAgent(h, pid) {
  check(Number.isSafeInteger(pid) && pid > 1, 'Invalid agentd PID');
  const stat = await readFile(`/proc/${pid}/status`, 'utf8');
  check(Number(stat.match(/^PPid:\s+(\d+)/m)?.[1]) === h.server.child.pid, 'Refusing to signal unrelated process');
  const cmd = await readFile(`/proc/${pid}/cmdline`, 'utf8');
  check(cmd.includes(h.agentEntry), 'PID does not reference this drill agentd');
  return Number(stat.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0);
}
