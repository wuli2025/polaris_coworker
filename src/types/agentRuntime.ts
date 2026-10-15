/** agent_backend_status: supervisor snapshot, separate from doctor ping. */
export interface AgentBackendStatus {
  backend: string;
  agentdVersion: string | null;
  pid: number | null;
  restarts: number;
  usagePersistFailures: number;
  circuitOpen: boolean;
  healthy: boolean;
  lastError: string | null;
}

export const previewBackendStatus: AgentBackendStatus = {
  backend: 'pi', agentdVersion: null, pid: null, restarts: 0,
  usagePersistFailures: 0, circuitOpen: false, healthy: false,
  lastError: 'BrowserPreview',
};
