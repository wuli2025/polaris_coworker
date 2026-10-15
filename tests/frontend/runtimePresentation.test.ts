import { describe, expect, it } from 'vitest';
import { createSSRApp } from 'vue';
import { renderToString } from '@vue/server-renderer';
import RuntimeDiagnostics from '../../src/features/runtime/components/RuntimeDiagnostics.vue';
import { legacyDoctorActions, legacyProviderActions, runtimeRows, usageCost, recordedCost } from '../../src/features/runtime/presentation';
import { validateProviderConfig, modelConfigExample } from '../../src/features/runtime/providerConfig';
import { contextEditingAvailable } from '../../src/features/runtime/contextPolicy';
import { previewBackendStatus } from '../../src/types/agentRuntime';
import type { EnvReport, VerifyReport } from '../../src/tauri';

const report = {
  os: 'linux', ready: false, shellReady: false, shellBundled: false,
  node: { version: 'v22.19.0', path: '/runtime/node' },
  agentRuntime: {
    backend: 'pi', nodeReady: true, entryExists: true, handshake: true,
    protocolVersion: 6, piVersion: '1.0.4', agentdVersion: '0.1.0',
    credentialsConfigured: false, legacyActions: false, error: null,
  },
} as EnvReport;
const verify = {
  deep: false, steps: [{ key: 'shell', detail: 'source=explicit; shell=/tools/bash', status: 'ok' }],
} as VerifyReport;

describe('runtime presentation', () => {
  it('requires BOTH backend and legacyActions for doctor actions', () => {
    expect(legacyDoctorActions(null)).toBe(false);
    expect(legacyDoctorActions(report)).toBe(false);
    expect(legacyDoctorActions({ ...report, agentRuntime: { ...report.agentRuntime, legacyActions: true } })).toBe(false);
    expect(legacyDoctorActions({ ...report, agentRuntime: { ...report.agentRuntime, backend: 'claude-cli', legacyActions: true } })).toBe(true);
    expect(legacyDoctorActions({ ...report, agentRuntime: { ...report.agentRuntime, backend: 'claude-cli' } })).toBe(false);
  });
  it('only shows proxy switches for the explicitly detected legacy backend', () => {
    expect(legacyProviderActions(null)).toBe(false);
    expect(legacyProviderActions(previewBackendStatus)).toBe(false);
    expect(legacyProviderActions({ ...previewBackendStatus, backend: 'unknown' })).toBe(false);
    expect(legacyProviderActions({ ...previewBackendStatus, backend: 'claude-cli' })).toBe(true);
  });
  it('maps shell source from the verification result rather than guessing from shellBundled', () => {
    expect(runtimeRows(report).find(r => r.label === 'shell')?.detail).toBe('未知');
    expect(runtimeRows(report, verify).find(r => r.label === 'shell')?.detail).toContain('source=explicit');
    expect(runtimeRows(report).find(r => r.label === '凭据')?.value).toBe('未配置');
    expect(runtimeRows({ ...report, agentRuntime: { ...report.agentRuntime, credentialsConfigured: true } })
      .find(r => r.label === '凭据')?.value).toBe('已配置');
  });
  it('renders the diagnostic contract, preview and no-inference notice', async () => {
    const html = await renderToString(createSSRApp(RuntimeDiagnostics, { report: { ...report, os: 'browser' }, verify }));
    for (const text of ['浏览器预览', '未就绪', '未做计费推理', 'Node', 'agentd 入口', 'ping', '1.0.4', 'source=explicit', '未配置']) {
      expect(html).toContain(text);
    }
    expect(html).not.toContain('后台自动装好');
  });
  it('keeps partial or unpriced totals unknown, including mixed positive aggregates', () => {
    for (const value of [0, 1.25, -1, NaN, Infinity]) expect(usageCost(value)).toBe('未知');
    expect(usageCost(0, true)).toBe('$0.0000');
    expect(usageCost(1.25, true)).toBe('$1.2500');
    expect(usageCost(-1, true)).toBe('未知');
    expect(recordedCost(0)).toBe('未知');
    expect(recordedCost(1.25)).toBe('$1.2500');
  });
  it('blocks the fixed CLAUDE.md writer where AGENTS.md priority cannot be determined', () => {
    for (const backend of ['pi', 'unknown', undefined]) expect(contextEditingAvailable(backend)).toBe(false);
    expect(contextEditingAvailable('claude-cli')).toBe(true);
  });
});

describe('provider model configuration', () => {
  it('accepts documented models and preserves env/catalog fallback', () => {
    expect(validateProviderConfig(JSON.parse(modelConfigExample), 'openai')).toBeNull();
    expect(validateProviderConfig({ env: {} }, '')).toBeNull();
    expect(validateProviderConfig({ models: [{ id: 'm', reasoning: false, input: ['text'],
      cost: { input: 1, output: 2, cacheRead: 0, cacheWrite: 0 }, contextWindow: 128000, maxTokens: 8192 }] }, 'responses')).toBeNull();
    for (const protocol of ['anthropic', 'chatgpt', 'google']) expect(validateProviderConfig({}, protocol)).toBeNull();
  });
  it.each([null, [], { env: [] }, { models: [] }, { models: [''] }, { models: [4] },
    { models: [{ id: 'x', reasoning: 'false' }] }, { models: [{ id: 'x', maxTokens: 0 }] },
    { models: [{ id: 'x', input: ['video'] }] }, { models: [{ id: 'x', cost: { input: 0 } }] },
  ])('rejects invalid configuration: %j', value => {
    expect(validateProviderConfig(value, 'openai')).not.toBeNull();
  });
  it('rejects unsupported protocols', () => {
    expect(validateProviderConfig({}, 'typo')).not.toBeNull();
  });
});
