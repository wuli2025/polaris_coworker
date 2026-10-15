import { beforeEach, describe, expect, it, vi } from 'vitest';
vi.mock('../../tauri', () => ({ invoke: vi.fn() }));
import { invoke } from '../../tauri';
import { sendLiveInput, type LiveInputMode } from './liveInput';
const target = { reqId: 'req-original', conversationId: 'conv' };
describe('running input transport', () => {
  beforeEach(() => { vi.mocked(invoke).mockReset(); });
  it.each([['steer', 'chat_steer'], ['followUp', 'chat_follow_up']] as const)(
    '%s captures the original request and conversation', async (mode, command) => {
      vi.mocked(invoke).mockResolvedValue({ turnId: 'turn', disposition: 'queued' });
      expect(await sendLiveInput(target, 'keep this text', mode)).toEqual({ turnId: 'turn', disposition: 'queued' });
      expect(invoke).toHaveBeenCalledWith(command, { ...target, text: 'keep this text' });
    });
  it('rejects idle, empty and invalid modes without network activity', async () => {
    await expect(sendLiveInput({ ...target, reqId: '' }, 'hello', 'steer')).rejects.toThrow('ChatNotRunning');
    await expect(sendLiveInput(target, '  ', 'steer')).rejects.toThrow('ChatEmptyInput');
    await expect(sendLiveInput(target, 'hello', 'invalid' as LiveInputMode)).rejects.toThrow('ChatInvalidMode');
    expect(invoke).not.toHaveBeenCalled();
  });
  it('surfaces a done race instead of sending a new prompt', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('SESSION_NOT_RUNNING'));
    await expect(sendLiveInput(target, 'hello', 'followUp')).rejects.toThrow('SESSION_NOT_RUNNING');
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
