// Real process helper; launched with the existing agentd tsx preloader.
const { acquire } = await import('../../agentd/src/models/credential-lock.ts');
const { FileCredentials } = await import('../../agentd/src/models/file-credentials.ts');
const { Redactor } = await import('../../agentd/src/models/credentials.ts');
const [mode, file] = process.argv.slice(2);
if (mode === 'hold') {
  await acquire(`${file}.lock`, { timeoutMs: 30000, pollMs: 25 });
  process.stdout.write('HELD\n');
  setInterval(() => {}, 1000);
} else if (mode === 'write') {
  let blocked = false;
  try {
    const unexpected = await acquire(`${file}.lock`, { timeoutMs: 200, pollMs: 25 });
    await unexpected.release();
  } catch (error) { if (error.message !== 'CredentialStoreBusy') throw error; blocked = true; }
  if (!blocked) throw new Error('Live holder did not exclude second process');
  process.stdout.write('CONTENDING\n');
  await new FileCredentials(file, new Redactor(), { timeoutMs: 30000, pollMs: 25 })
    .modify('p', async () => ({ type: 'api_key', key: 'drill-recovered' }));
  process.stdout.write('WRITTEN\n');
} else throw new Error('Unknown lock helper mode');
