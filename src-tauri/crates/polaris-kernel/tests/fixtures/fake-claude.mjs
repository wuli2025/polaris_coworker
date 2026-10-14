// Legacy stream-json fixture; accepts the old CLI flags but launches no children.
let text = '';
for await (const chunk of process.stdin) text += chunk;
if (text === 'hang') setInterval(() => {}, 1000);
else {
  process.stdout.write(JSON.stringify({ type: 'assistant', message: { content: [
    { type: 'tool_use', name: 'Read', input: { file_path: '/tmp/test.md' } },
    { type: 'text', text: ' hello \nworld ' },
  ] } }) + '\n');
  process.stdout.write(JSON.stringify({ type: 'result', subtype: 'success', result: 'must not duplicate' }) + '\n');
}
