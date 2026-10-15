import { afterEach, describe, expect, it, vi } from 'vitest';
import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';
import ApprovalPanel from './ApprovalPanel.vue';
import { useSecurity } from './useSecurity';
import type { Approval, Command } from './contracts';
import { interconnectCopy as c } from './copy';

const approval = (id: string, expiresAt: number): Approval =>
  ({ id, expiresAt, operation: 'delete_account', actor: id, host: 'host', state: 'pending' });
afterEach(() => vi.useRealTimers());

describe('approval expiry', () => {
  it('drops expired events and stale inbox entries, retaining future requests', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(100000);
    const command = vi.fn().mockResolvedValue({ notifications: [], approvals:
      [approval('past', 99), approval('boundary', 100), approval('future', 101)] });
    const state = useSecurity(command as Command);
    state.receive(approval('expired-event', 100));
    expect(state.approvals.value).toEqual([]);
    await state.refresh();
    expect(state.approvals.value.map(a => a.id)).toEqual(['future']);
  });

  it('cleans up locally during offline polling and while a risky action is busy', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(100000);
    const command = vi.fn().mockRejectedValue('offline');
    const state = useSecurity(command as Command);
    state.receive(approval('expires', 101));
    state.receive(approval('later', 110));
    state.busy.value = true;
    vi.setSystemTime(101000);
    await state.tick();
    expect(state.approvals.value.map(a => a.id)).toEqual(['later']);
    expect(command).not.toHaveBeenCalled();
    vi.setSystemTime(110000);
    await state.refresh();
    expect(state.approvals.value).toEqual([]);
    expect(state.error.value).toBe('offline');
  });

  it('does not render expired approval rows or an empty heading', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(100000);
    const render = (approvals: Approval[]) => renderToString(createSSRApp({ render: () =>
      h(ApprovalPanel, { approvals, pending: '', error: '', busy: false }) }));
    const html = await render([approval('expired-actor', 100), approval('future-actor', 101)]);
    expect(html).not.toContain('expired-actor');
    expect(html).toContain('future-actor');
    expect(await render([approval('expired-actor', 99)])).not.toContain(c.approvals);
  });
});
