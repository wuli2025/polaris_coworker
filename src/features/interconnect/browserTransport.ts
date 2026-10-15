import type { Command } from './contracts';
import { interconnectCopy as c } from './copy';

const routes: Record<string, [string, string]> = {
  account_login_password: ['POST', '/api/interconnect/login'],
  mesh_status: ['GET', '/api/interconnect/status'],
  mesh_sync: ['POST', '/api/interconnect/retry'],
  mesh_security_inbox: ['GET', '/api/interconnect/security/inbox'],
  mesh_security_ack: ['POST', '/api/interconnect/security/ack'],
  mesh_risk_decide: ['POST', '/api/interconnect/security/decide'],
  mesh_device_remove: ['POST', '/api/interconnect/devices/revoke'],
};
interface Transport {
  native: boolean; invoke: Command; backend: () => Promise<boolean>;
  token: () => string; fetch: typeof fetch;
}
export function createInterconnectCommand(io: Transport): Command {
  let previewLoggedIn = false;
  return async <T>(name: string, args?: Record<string, unknown>): Promise<T> => {
    if (io.native) return io.invoke<T>(name, args);
    if (name === 'collab_host_start') return {} as T;
    const route = routes[name];
    if (!route) return io.invoke<T>(name, args);
    if (!await io.backend()) {
      if (name === 'account_login_password') previewLoggedIn = true;
      if (name === 'mesh_security_inbox') return { notifications: [], approvals: [] } as T;
      // No real credentials, session token or account adoption in a frontend-only preview.
      return { enrolled: previewLoggedIn, peers: [], preview: true, configurationError: c.preview,
        interconnect: previewLoggedIn ? { stage: 'connect', state: 'ready', error: null, retryable: false } : null,
        relayConfig: { urls: [], state: 'unavailable' } } as T;
    }
    const token = io.token();
    if (name === 'mesh_status' && !token) return { enrolled: false } as T;
    const [method, url] = route;
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (token && name !== 'account_login_password') headers.Authorization = `Bearer ${token}`;
    const response = await io.fetch(url, { method, headers, cache: 'no-store', credentials: 'omit',
      body: method === 'GET' ? undefined : JSON.stringify(args || {}) });
    const value = await response.json();
    if (!response.ok) throw new Error(value.error || c.failed);
    return value as T;
  };
}
