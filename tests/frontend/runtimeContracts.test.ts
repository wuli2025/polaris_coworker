import { beforeEach, describe, expect, it, vi } from 'vitest';
import { contextFiles, expertSettings, type ContextSelection } from '../../src/features/runtime/contracts';
import { useExpertSettings } from '../../src/features/runtime/useExpertSettings';
import { pricedCost, unknownCosts } from '../../src/features/runtime/usageCosts';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('../../src/tauri', () => ({ invoke }));
beforeEach(() => { invoke.mockReset(); });

describe('context selection contract', () => {
  it('passes the original selection back to read and write without selecting again', async () => {
    const selected: ContextSelection = { root: '/project', fileName: 'CLAUDE.md', relativePath: 'CLAUDE.md',
      agentsExists: false, claudeExists: true, exists: true };
    invoke.mockResolvedValueOnce(selected).mockResolvedValueOnce('original').mockResolvedValueOnce(undefined);
    const metadata = await contextFiles.metadata('project');
    expect(await contextFiles.read('project', metadata)).toBe('original');
    await contextFiles.write('project', metadata, 'edited');
    expect(invoke.mock.calls).toEqual([
      ['context_file_metadata', { projectId: 'project', createFile: 'AGENTS.md' }],
      ['context_file_read', { projectId: 'project', selection: selected }],
      ['context_file_write', { projectId: 'project', selection: selected, content: 'edited' }],
    ]);
  });
  it('supports an explicit CLAUDE.md creation policy and propagates permission errors', async () => {
    invoke.mockRejectedValue(new Error('ContextPermissionDenied'));
    await expect(contextFiles.metadata('p', 'CLAUDE.md')).rejects.toThrow('ContextPermissionDenied');
    expect(invoke).toHaveBeenCalledWith('context_file_metadata', { projectId: 'p', createFile: 'CLAUDE.md' });
  });
});

describe('expert settings state', () => {
  it('reads actual state and waits for successful setter response', async () => {
    const state = useExpertSettings();
    expect(state.enabled.value).toBeNull();
    invoke.mockResolvedValueOnce({ enabled: false });
    await state.refresh();
    expect(state.enabled.value).toBe(false);
    let complete!: (value: { enabled: boolean }) => void;
    invoke.mockReturnValueOnce(new Promise(resolve => { complete = resolve; }));
    const pending = state.set(true);
    expect(state.enabled.value).toBe(false);
    expect(state.busy.value).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith('expert_enabled_set', { enabled: true });
    complete({ enabled: true });
    await pending;
    expect(state.enabled.value).toBe(true);
    expect(state.busy.value).toBe(false);
  });
  it('does not report success after a rejected setter or failed read', async () => {
    const state = useExpertSettings();
    invoke.mockRejectedValue(new Error('ExpertConfigWriteFailed'));
    await state.set(true);
    expect(state.enabled.value).toBeNull();
    expect(state.error.value).toContain('ExpertConfigWriteFailed');
    await state.refresh();
    expect(state.enabled.value).toBeNull();
    await expect(expertSettings.set(false)).rejects.toThrow('ExpertConfigWriteFailed');
  });
});

describe('usage price completeness', () => {
  it.each([
    [4.5, 0, false, '$4.5000', '全部请求已标价'],
    [4.5, 2, true, '$4.5000', '另有 2 次请求未标价'],
    [0, 3, true, '$0.0000', '另有 3 次请求未标价'],
  ])('preserves subtotal %s with %s unknown requests', (cost, count, unknown, amount, message) => {
    expect(pricedCost(cost as number)).toBe(amount);
    expect(unknownCosts({ unpricedRequests: count as number, hasUnknownCosts: unknown as boolean })).toBe(message);
  });
  it('does not fabricate completeness for an old host', () => {
    expect(unknownCosts({})).toBe('价格完整性未知');
    expect(pricedCost(NaN)).toBe('未知');
  });
});
