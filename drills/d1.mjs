import { readFile, writeFile, access } from 'node:fs/promises';
import { join } from 'node:path';
import { randomBytes } from 'node:crypto';
import { liveInput } from './lib/live-input.mjs';
import { approvedBash, respondDuring } from './lib/permissions.mjs';
import { run, Blocked, check, integer } from './lib/harness.mjs';
import { build, server, conversation, memory, mapping, setModel, sessionFiles, sessionEntries } from './lib/agent.mjs';
const steps = {
  build,
  async chat(h) {
    const c = await conversation(h, 'chat');
    const nonce = randomBytes(8).toString('hex');
    const r = await h.api.turn(c.id, `Reply exactly: DRILL_${nonce}`, { permissionMode: 'deny' });
    check(r.text.includes(`DRILL_${nonce}`), 'Reply did not contain requested nonce');
    return { ...r.metrics, ...await memory(h) };
  },
  async coding(h) {
    const c = await conversation(h, 'coding');
    const r = await approvedBash(h.api, c.id,
      'Use write to create calc.py printing 40+2. Run python3 calc.py with bash redirecting output to before.txt. '
      + 'Use edit to change it to 40+3. Run it again redirecting output to after.txt. Read both outputs. Only work in this project.');
    check((await readFile(join(c.cwd, 'before.txt'), 'utf8')).trim() === '42', 'First run output mismatch');
    check((await readFile(join(c.cwd, 'after.txt'), 'utf8')).trim() === '43', 'Edited program output mismatch');
    for (const tool of ['write', 'edit', 'bash', 'read']) check(r.events.some(e => e.tool === tool), `Missing ${tool} tool event`);
    return { ...r.metrics, ...await memory(h), outputs: [42, 43] };
  },
  async image(h) {
    const c = await conversation(h, 'image');
    const image = await h.api.invoke('chat_attach_image', { conversationId: c.id, name: 'pixel.png',
      dataBase64: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=' });
    check(image.ok && typeof image.path === 'string', 'Image attachment path missing');
    const r = await h.api.turn(c.id, 'Describe the attached image briefly.', { imagePaths: [image.path], permissionMode: 'deny' });
    check(r.text.trim().length > 0, 'No image reply');
    const files = await sessionFiles(join(h.home, 'sessions'));
    const contents = await Promise.all(files.map(f => readFile(f, 'utf8')));
    check(contents.some(s => s.includes('"image"') && s.includes('image/png')), 'Persisted session has no image content');
    return { ...r.metrics, ...await memory(h), persistedImage: true };
  },
  async compact(h) {
    if (!h.model) throw new Blocked('compact requires DRILL_MODEL so context metadata can be set without guessing a model');
    const window = integer(process.env, 'DRILL_CONTEXT_WINDOW', 32768, 20000, 1000000);
    await setModel(h, h.model, window);
    const c = await conversation(h, 'compact');
    const limit = integer(process.env, 'DRILL_COMPACT_TURNS', 12, 1, 100);
    const marker = randomBytes(12).toString('hex');
    await h.api.turn(c.id, `Remember marker ${marker}. Say OK.`, { permissionMode: 'deny' });
    for (let round = 1; round <= limit; round++) {
      const padding = randomBytes(12000).toString('hex');
      await h.api.turn(c.id, `This is inert test data, summarize as OK: ${padding}`, { permissionMode: 'deny' });
      const entries = await sessionEntries(h, c.id);
      if (entries.some(e => e.type === 'compaction')) {
        const r = await h.api.turn(c.id, 'What marker did I ask you to remember?', { permissionMode: 'deny' });
        check(r.text.includes(marker), 'History marker lost after compaction');
        return { rounds: round, contextWindow: window, persistedCompaction: true, ...r.metrics, ...await memory(h) };
      }
    }
    throw new Error('No persisted compaction within configured turn budget');
  },
  async abort(h) {
    const c = await conversation(h, 'abort');
    const t = respondDuring(h.api, await h.api.begin(c.id, 'Use bash to execute sleep 120, then write finished.txt.'), p => p.tool === 'bash' ? 'allow' : 'deny');
    await h.api.event(t.req, e => e.kind === 'tool' && e.tool === 'bash' && e.text?.includes('sleep 120'));
    const cancelAt = performance.now();
    await h.api.invoke('chat_cancel', { reqId: t.req });
    const r = await h.api.end(t, true);
    check(r.done.at >= cancelAt, 'Turn completed before cancellation');
    check(!await access(join(c.cwd, 'finished.txt')).then(() => true, () => false), 'Cancelled task created its completion file');
    check(!(await h.api.invoke('chat_is_running', { reqId: t.req })), 'Turn still running after done');
    return { ...r.metrics, cancelToDoneMs: Math.round(r.done.at - cancelAt), ...await memory(h) };
  },
  steer: h => liveInput(h, 'steer'),
  followup: h => liveInput(h, 'followUp'),
  async switch(h) {
    const next = process.env.DRILL_MODEL_NEXT;
    if (!h.model || !next?.trim() || next.length > 200 || next === h.model) throw new Blocked('switch requires distinct DRILL_MODEL and DRILL_MODEL_NEXT supported by the copied account');
    const c = await conversation(h, 'switch');
    const marker = randomBytes(16).toString('hex');
    await setModel(h, h.model);
    await h.api.turn(c.id, `Remember marker ${marker}. Say OK.`, { permissionMode: 'deny' });
    const before = await mapping(h, c.id);
    await setModel(h, next);
    const r = await h.api.turn(c.id, 'Return the marker I asked you to remember.', { permissionMode: 'deny' });
    const after = await mapping(h, c.id);
    check(before.id === after.id, 'Session id changed on idle reopen');
    check(before.model.id !== after.model.id && after.model.id === next, 'Model did not change');
    check(r.text.includes(marker), 'History marker lost after model switch');
    return { sessionId: after.id, before: before.model, after: after.model, ...r.metrics, ...await memory(h) };
  },
};
await run('D1', steps, async (h, name) => {
  if (name !== 'build') await server(h);
});
