export type Stage = 'register' | 'discover' | 'relay' | 'connect';
export interface Progress { stage: Stage; state: string; error?: string | null; retryable: boolean }
export interface RelayConfig { urls: string[]; state: string; source?: string }
export interface Notice { kind: 'device_enrolled'; deviceId: string; nodeId: string; name: string; platform: string; time: number }
export interface Approval { id: string; operation: string; actor: string; actorName?: string | null; actorPlatform?: string | null; host: string; expiresAt: number; state: string }
export interface Inbox { notifications: Notice[]; approvals: Approval[] }
export interface Device {
  storageMode?: 'http';
  nodeId: string; deviceId?: string; name: string; os: string; online: boolean; isSelf: boolean;
  connected: boolean; mounted: boolean; drive: string; error: string; port?: number; revoked: boolean;
}
export type Command = <T>(name: string, args?: Record<string, unknown>) => Promise<T>;
export interface Timing { pollMs: number; retryMs: number; slowMs: number }
export function timing(input: Partial<Timing> = {}): Timing {
  const result = { pollMs: 10000, retryMs: 5000, slowMs: 30000, ...input };
  for (const [key, value] of Object.entries(result)) {
    if (!Number.isInteger(value) || value < 1000 || value > 300000) throw new Error(`Invalid ${key}`);
  }
  return result;
}
