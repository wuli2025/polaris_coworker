import { join } from 'node:path';
import { run, check, repo, save } from './lib/harness.mjs';
import { start } from './lib/process.mjs';
import { buildEnv } from './lib/agent.mjs';
import { regressionConfig, rustSuites, warnings, summary, lineCheck } from './lib/regression.mjs';
async function execute(h, suite, suffix) {
  const [command, ...args] = suite.command;
  return start(h, `${suite.id}-${suffix}`, command, args, { cwd: join(repo, suite.cwd), env: { ...buildEnv(h), POLARIS_NET_TEST: '1' } }).done();
}
await run('D6', {
  async suites(h) {
    const config = await regressionConfig();
    const suites = [...await rustSuites(), ...config.suites];
    const rows = [];
    for (let round = 1; round <= h.options.repeat; round++) {
      for (const suite of suites) rows.push(await h.step(`suites/${round}/${suite.id}`, async () => {
        const r = await execute(h, suite, round);
        if (r.code !== 0) throw Object.assign(new Error('Regression command failed'), {
          safeReason: 'Regression command failed', evidence: { round, suite: suite.id, command: suite.command, exitCode: r.code, signal: r.signal },
        });
        const ignored = [...r.text.matchAll(/(\d+) ignored/g)].reduce((n, m) => n + Number(m[1]), 0);
        return { round, suite: suite.id, command: suite.command, exitCode: r.code, ignored, warningCount: warnings(r.text) };
      }));
    }
    const report = { rounds: h.options.repeat, suitesPerRound: suites.length, ...summary(rows) };
    await save(join(h.root, 'regression-summary.json'), report);
    await h.emit('pass-rate', report.fail ? 'fail' : report.blocked ? 'blocked' : 'pass', report);
    check(report.pass === report.attempts && report.attempts > 0, 'Not every regression attempt passed');
    return report;
  },
  async builds(h) {
    const config = await regressionConfig(); const results = [];
    for (const suite of config.builds) results.push(await h.step(`builds/${suite.id}`, async () => {
      const r = await execute(h, suite, 'build'); const count = warnings(r.text);
      await h.emit(`warning-count/${suite.id}`, r.code === 0 && count === 0 ? 'pass' : 'fail', { warnings: count, exitCode: r.code });
      check(r.code === 0 && count === 0, 'Build failed or emitted warnings');
      return { command: suite.command, warnings: count, exitCode: r.code };
    }));
    check(results.every(r => r.status === 'pass'), 'Three builds did not all pass without warnings');
    return { builds: 3, warningCount: 0 };
  },
  async lines(h) {
    const result = await lineCheck(await regressionConfig());
    await save(join(h.root, 'line-violations.json'), result);
    await h.emit('line-count', result.violations.length ? 'fail' : 'pass', result);
    check(!result.violations.length, 'Repository contains files exceeding 500 lines or columns; see line-count');
    return result;
  },
});
