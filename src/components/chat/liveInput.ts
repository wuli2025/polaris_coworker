import { invoke } from '../../tauri';

export type LiveInputMode = 'steer' | 'followUp';
export interface LiveInputTarget { reqId: string; conversationId: string }
export interface LiveInputReceipt { turnId: string; disposition: string }
const commands: Record<LiveInputMode, string> = {
  steer: 'chat_steer',
  followUp: 'chat_follow_up',
};
export async function sendLiveInput(target: LiveInputTarget, text: string, mode: LiveInputMode) {
  if (!target.reqId || !target.conversationId) throw new Error('ChatNotRunning');
  if (!text.trim()) throw new Error('ChatEmptyInput');
  if (!Object.hasOwn(commands, mode)) throw new Error('ChatInvalidMode');
  return invoke<LiveInputReceipt>(commands[mode], { ...target, text });
}
