/** P5a runtime contract; P5c renders backend-specific doctor actions. */
export interface AgentRuntimeReport {
  backend: string;
  nodeReady: boolean;
  entryExists: boolean;
  handshake: boolean;
  protocolVersion: number | null;
  piVersion: string | null;
  agentdVersion: string | null;
  credentialsConfigured: boolean;
  legacyActions: boolean;
  error: string | null;
}
